use apollo_compiler::schema::ExtendedType;
use serde::Deserialize;
use serde::Serialize;
use serde_json_bytes::ByteString;
use serde_json_bytes::Entry;

use crate::json_ext::Object;
use crate::json_ext::Value;
use crate::json_ext::ValueExt;
use crate::spec::Schema;

/// A selection that is part of a fetch.
/// Selections are used to propagate data to subgraph fetches.
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "PascalCase", tag = "kind")]
pub(crate) enum Selection {
    /// A field selection.
    Field(Field),

    /// An inline fragment selection.
    InlineFragment(InlineFragment),
}

/// The field that is used
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Field {
    /// An optional alias for the field.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) alias: Option<String>,

    /// The name of the field.
    pub(crate) name: String,

    /// The selections for the field.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) selections: Option<Vec<Selection>>,
}

/// An inline fragment.
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct InlineFragment {
    /// The required fragment type.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) type_condition: Option<String>,

    /// The selections from the fragment.
    pub(crate) selections: Vec<Selection>,
}

pub(crate) fn execute_selection_set<'a>(
    input_content: &'a Value,
    selections: &[Selection],
    schema: &Schema,
    mut current_type: Option<&'a str>,
) -> Value {
    let content = match input_content.as_object() {
        Some(o) => o,
        None => return Value::Null,
    };

    current_type = current_type.or_else(|| content.get("__typename").and_then(|v| v.as_str()));

    let mut output = Object::with_capacity(selections.len());
    for selection in selections {
        match selection {
            Selection::Field(Field {
                alias,
                name,
                selections,
            }) => {
                let selection_name = alias.as_deref().unwrap_or(name.as_str());
                let field_type = current_type.and_then(|t| {
                    schema.definitions.types.get(t).and_then(|ty| match ty {
                        apollo_compiler::schema::ExtendedType::Object(o) => {
                            o.fields.get(name.as_str()).map(|f| &f.ty)
                        }
                        apollo_compiler::schema::ExtendedType::Interface(i) => {
                            i.fields.get(name.as_str()).map(|f| &f.ty)
                        }
                        _ => None,
                    })
                });

                match content.get_key_value(selection_name) {
                    None => {
                        if name == "__typename" {
                            // if the __typename field was missing but we can infer it, fill it
                            if let Some(ty) = current_type {
                                output.insert(
                                    ByteString::from(selection_name.to_owned()),
                                    Value::String(ByteString::from(ty.to_owned())),
                                );
                                continue;
                            }
                        }
                        // the behaviour here does not align with the gateway: we should instead assume that
                        // data is in the correct shape, and return a null (or even no value at all) on
                        // missing fields. If a field was missing, it should have been nullified,
                        // and if it was non nullable, the parent object would have been nullified.
                        // Unfortunately, we don't validate subgraph responses yet
                        if field_type
                            .as_ref()
                            .map(|ty| !ty.is_non_null())
                            .unwrap_or(false)
                        {
                            output.insert(ByteString::from(selection_name.to_owned()), Value::Null);
                        } else {
                            return Value::Null;
                        }
                    }
                    Some((key, value)) => {
                        if let Some(elements) = value.as_array() {
                            let selected = elements
                                .iter()
                                .map(|element| match selections {
                                    Some(sels) => execute_selection_set(
                                        element,
                                        sels,
                                        schema,
                                        field_type
                                            .as_ref()
                                            .map(|ty| ty.inner_named_type().as_str()),
                                    ),
                                    None => element.clone(),
                                })
                                .collect::<Vec<_>>();
                            output.insert(key.clone(), Value::Array(selected));
                        } else if let Some(sels) = selections {
                            output.insert(
                                key.clone(),
                                execute_selection_set(
                                    value,
                                    sels,
                                    schema,
                                    field_type.as_ref().map(|ty| ty.inner_named_type().as_str()),
                                ),
                            );
                        } else {
                            output.insert(key.clone(), value.clone());
                        }
                    }
                }
            }
            Selection::InlineFragment(InlineFragment {
                type_condition,
                selections,
            }) => match type_condition {
                None => continue,
                Some(condition) => {
                    if condition_matches(schema, current_type.unwrap(), condition) {
                        if let Value::Object(selected) =
                            execute_selection_set(input_content, selections, schema, current_type)
                        {
                            for (key, value) in selected.into_iter() {
                                match output.entry(key) {
                                    Entry::Vacant(e) => {
                                        e.insert(value);
                                    }
                                    Entry::Occupied(e) => {
                                        e.into_mut().deep_merge(value);
                                    }
                                }
                            }
                        }
                    }
                }
            },
        }
    }

    Value::Object(output)
}

/// This is similar to DoesFragmentTypeApply from the GraphQL spec, but the
/// `current_type` could be an abstract type. So we'll be more flexible in our
/// tests, checking if the condition is a subtype of the current type, or vice
/// versa.
/// <https://spec.graphql.org/October2021/#DoesFragmentTypeApply()>
fn condition_matches(schema: &Schema, current_type: &str, condition: &str) -> bool {
    let current_type = match schema.definitions.types.get(current_type) {
        None => return false,
        Some(t) => t,
    };

    let conditional_type = match schema.definitions.types.get(condition) {
        None => return false,
        Some(t) => t,
    };

    match current_type {
        ExtendedType::Object(object_type) => match conditional_type {
            ExtendedType::Interface(interface_type) => {
                return object_type
                    .implements_interfaces
                    .contains(&interface_type.name)
            }
            ExtendedType::Union(union_type) => {
                return union_type.members.contains(&object_type.name)
            }
            ExtendedType::Object(_) => {
                return object_type.name == condition;
            }
            _ => return false,
        },
        ExtendedType::Interface(interface_type) => match conditional_type {
            ExtendedType::Interface(conditional_type) => {
                return conditional_type
                    .implements_interfaces
                    .contains(&interface_type.name)
            }
            ExtendedType::Union(union_type) => {
                return union_type.members.contains(&interface_type.name)
            }
            ExtendedType::Object(object_type) => {
                return interface_type
                    .implements_interfaces
                    .contains(&object_type.name)
            }
            _ => return false,
        },
        ExtendedType::Union(union_type) => match conditional_type {
            ExtendedType::Interface(interface_type) => {
                return union_type.members.contains(&interface_type.name)
            }
            ExtendedType::Object(object_type) => {
                return union_type.members.contains(&object_type.name)
            }
            _ => return false,
        },
        _ => return false,
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use serde_json_bytes::json as bjson;

    use super::Selection;
    use super::*;
    use crate::error::FetchError;
    use crate::graphql::Response;
    use crate::json_ext::Path;

    fn select(
        response: &Response,
        path: &Path,
        selections: &[Selection],
        schema: &Schema,
    ) -> Result<Value, FetchError> {
        let mut values = Vec::new();
        response
            .data
            .as_ref()
            .unwrap()
            .select_values_and_paths(schema, path, |_path, value| {
                values.push(value);
            });

        Ok(Value::Array(
            values
                .into_iter()
                .map(|value| execute_selection_set(value, selections, schema, None))
                .collect::<Vec<_>>(),
        ))
    }

    macro_rules! select {
        ($schema:expr, $content:expr $(,)?) => {{
            let schema = Schema::parse_test(&$schema, &Default::default()).unwrap();
            let response = Response::builder()
                .data($content)
                .build();
            // equivalent to "... on OtherStuffToIgnore {} ... on User { __typename id job { name } }"
            let stub = json!([
                {
                    "kind": "InlineFragment",
                    "typeCondition": "OtherStuffToIgnore",
                    "selections": [],
                },
                {
                    "kind": "InlineFragment",
                    "typeCondition": "User",
                    "selections": [
                        {
                            "kind": "Field",
                            "name": "__typename",
                        },
                        {
                            "kind": "Field",
                            "name": "id",
                        },
                        {
                            "kind": "Field",
                            "name": "job",
                            "selections": [
                                {
                                    "kind": "Field",
                                    "name": "name",
                                }
                            ],
                        }
                      ]
                },
            ]);
            let selection: Vec<Selection> = serde_json::from_value(stub).unwrap();
            select(&response, &Path::empty(), &selection, &schema)
        }};
    }

    #[test]
    fn test_selection() {
        assert_eq!(
            select!(
                include_str!("testdata/schema.graphql"),
                bjson!({"__typename": "User", "id":2, "name":"Bob", "job":{"name":"astronaut"}}),
            )
            .unwrap(),
            bjson!([{
                "__typename": "User",
                "id": 2,
                "job": {
                    "name": "astronaut"
                }
            }]),
        );
    }

    #[test]
    fn test_selection_subtype() {
        assert_eq!(
            select!(
                with_supergraph_boilerplate(
                    "type Query { me: String } type Author { name: String } type Reviewer { name: String } \
                    union User = Author | Reviewer"
                ),
                bjson!({"__typename": "Author", "id":2, "name":"Bob", "job":{"name":"astronaut"}}),
            )
            .unwrap(),
            bjson!([{
                "__typename": "Author",
                "id": 2,
                "job": {
                    "name": "astronaut"
                }
            }]),
        );
    }

    #[test]
    fn test_selection_missing_field() {
        // equivalent to "... on OtherStuffToIgnore {} ... on User { __typename id job { name } }"

        assert_eq!(
            select!(
                include_str!("testdata/schema.graphql"),
                json!({"__typename": "User", "name":"Bob", "job":{"name":"astronaut"}}),
            )
            .unwrap(),
            bjson!([{}])
        );
    }

    #[test]
    fn test_array() {
        let schema = with_supergraph_boilerplate(
            "type Query { me: String }
            type MainObject { mainObjectList: [SubObject] }
            type SubObject { key: String name: String }",
        );
        let schema = Schema::parse_test(&schema, &Default::default()).unwrap();

        let response = bjson!({
            "__typename": "MainObject",
            "mainObjectList": [
                {
                    "key": "a",
                    "name": "A"
                },
                {
                    "key": "b",
                    "name": "B"
                }
            ]
        });

        let requires = json!([
            {
                "kind": "InlineFragment",
                "typeCondition": "MainObject",
                "selections": [
                    {
                        "kind": "Field",
                        "name": "__typename",
                    },
                    {
                        "kind": "Field",
                        "name": "mainObjectList",
                        "selections": [
                            {
                                "kind": "Field",
                                "name": "key",
                            }
                        ],
                    }
                ],
            },
        ]);
        let selection: Vec<Selection> = serde_json::from_value(requires).unwrap();

        let value = execute_selection_set(&response, &selection, &schema, None);
        println!(
            "response\n{}\nand selection\n{:?}\n returns:\n{}",
            serde_json::to_string_pretty(&response).unwrap(),
            selection,
            serde_json::to_string_pretty(&value).unwrap()
        );

        assert_eq!(
            value,
            bjson!({
                "__typename": "MainObject",
                "mainObjectList": [
                    {
                        "key": "a"
                    },
                    {
                        "key": "b"
                    }
                ]
            })
        );
    }

    #[test]
    fn test_customer_issue() {
        let schema = with_supergraph_boilerplate(
            "type Query { hello: String }
            type CategoryExperience {
              categoryId: Int!
              market: String!
              status: PossibleProductCategory
            }
            union PossibleProductCategory = ProductCategory | UnavailableCategory
            type ProductCategory {
              categoryId: Int!
              brandCatalogId: Int!
              aisleId: Int!
              url: String! # @external
            }
            type UnavailableCategory {
              categoryId: Int! # @external
              target: PossibleTargetForUnavailableCategory # @external
            }
            union PossibleTargetForUnavailableCategory = CategoryPageURL | NavigationPath
            type CategoryPageURL  {
              categoryId: Int!
              selectedFilters: [FilterSelection!]
            }
            type FilterSelection {
              filterId: String!
              optionId: String
              optionRangeStart: Int
              optionRangeEnd: Int
            }
            type NavigationPath { # @shareable {
              urlTarget: String! # URL!
            }",
        );
        let schema = Schema::parse_test(&schema, &Default::default()).unwrap();

        let response = bjson!({
          "__typename": "CategoryExperience",
          "categoryId": 1780384,
          "market": "{\"brandCatalogId\":1,\"storeId\":49,\"brand\":\"WF\",\"channel\":\"ECM\",\"country\":\"US\",\"locale\":\"en-US\",\"location\":null,\"segment\":\"B2C\"}",
          "status": {
            "__typename": "ProductCategory",
            "categoryId": 1780384,
            "brandCatalogId": 1,
            "aisleId": -1,
          },
        });

        let requires = json!([
          {
            "kind": "InlineFragment",
            "typeCondition": "CategoryExperience",
            "selections": [
              {
                "kind": "Field",
                "name": "__typename"
              },
              {
                "kind": "Field",
                "name": "status",
                "selections": [
                  {
                    "kind": "InlineFragment",
                    "typeCondition": "ProductCategory",
                    "selections": [
                      {
                        "kind": "Field",
                        "name": "__typename"
                      },
                      {
                        "kind": "Field",
                        "name": "categoryId"
                      },
                      {
                        "kind": "Field",
                        "name": "brandCatalogId"
                      },
                      {
                        "kind": "Field",
                        "name": "aisleId"
                      }
                    ]
                  },
                  {
                    "kind": "InlineFragment",
                    "typeCondition": "UnavailableCategory",
                    "selections": [
                      {
                        "kind": "Field",
                        "name": "__typename"
                      },
                      {
                        "kind": "Field",
                        "name": "categoryId"
                      },
                      {
                        "kind": "Field",
                        "name": "target",
                        "selections": [
                          {
                            "kind": "InlineFragment",
                            "typeCondition": "NavigationPath",
                            "selections": [
                              {
                                "kind": "Field",
                                "name": "__typename"
                              },
                              {
                                "kind": "Field",
                                "name": "urlTarget"
                              }
                            ]
                          }
                        ]
                      }
                    ]
                  }
                ]
              },
              {
                "kind": "Field",
                "name": "categoryId"
              },
              {
                "kind": "Field",
                "name": "market"
              }
            ]
          }
        ]);

        let selection: Vec<Selection> = serde_json::from_value(requires).unwrap();

        let value = execute_selection_set(&response, &selection, &schema, None);
        println!(
            "response\n{}\nand selection\n{:?}\n returns:\n{}",
            serde_json::to_string_pretty(&response).unwrap(),
            selection,
            serde_json::to_string_pretty(&value).unwrap()
        );

        assert_eq!(
            value,
            bjson!({
                "__typename": "CategoryExperience",
                "categoryId": 1780384,
                "market": "{\"brandCatalogId\":1,\"storeId\":49,\"brand\":\"WF\",\"channel\":\"ECM\",\"country\":\"US\",\"locale\":\"en-US\",\"location\":null,\"segment\":\"B2C\"}",
                "status": {
                    "__typename": "ProductCategory",
                    "categoryId": 1780384,
                    "brandCatalogId": 1,
                    "aisleId": -1,
                },
            })
        );
    }

    fn with_supergraph_boilerplate(content: &str) -> String {
        format!(
            "{}\n{}",
            r#"
        schema
            @core(feature: "https://specs.apollo.dev/core/v0.1")
            @core(feature: "https://specs.apollo.dev/join/v0.1") {
            query: Query
        }
        directive @core(feature: String!) repeatable on SCHEMA
        directive @join__graph(name: String!, url: String!) on ENUM_VALUE
        enum join__Graph {
            TEST @join__graph(name: "test", url: "http://localhost:4001/graphql")
        }

        "#,
            content
        )
    }
}
