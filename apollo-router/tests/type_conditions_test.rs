//!
//! Please ensure that any tests added to this file use the tokio multi-threaded test executor.
//!

use apollo_compiler::execution::JsonMap;
use apollo_router::plugin::test::MockSubgraph;
use apollo_router::services::supergraph;
use apollo_router::MockedSubgraphs;
use apollo_router::TestHarness;
use serde_json::json;
use tower::ServiceExt;

mod integration;

#[tokio::test(flavor = "multi_thread")]
async fn test_type_conditions_enabled() {
    let harness = setup(json! {{
        "experimental_type_conditioned_fetching": true,
        // will make debugging easier
        "plugins": {
            "experimental.expose_query_plan": true
        },
        "include_subgraph_errors": {
            "all": true
        }
    }});
    let supergraph_service = harness.build_supergraph().await.unwrap();
    let mut variables = JsonMap::new();
    variables.insert("movieResultParam", "movieResultEnabled".into());
    variables.insert("articleResultParam", "articleResultEnabled".into());
    let request = supergraph::Request::fake_builder()
        .query(QUERY.to_string())
        .header("Apollo-Expose-Query-Plan", "true")
        .variables(variables)
        .build()
        .expect("expecting valid request");

    let response = supergraph_service
        .oneshot(request)
        .await
        .unwrap()
        .next_response()
        .await
        .unwrap();

    insta::assert_json_snapshot!(response);
}

#[tokio::test(flavor = "multi_thread")]
async fn test_type_conditions_enabled_list_of_list() {
    let harness = setup(json! {{
        "experimental_type_conditioned_fetching": true,
        // will make debugging easier
        "plugins": {
            "experimental.expose_query_plan": true
        },
        "include_subgraph_errors": {
            "all": true
        }
    }});
    let supergraph_service = harness.build_supergraph().await.unwrap();
    let mut variables = JsonMap::new();
    variables.insert("movieResultParam", "movieResultEnabled".into());
    variables.insert("articleResultParam", "articleResultEnabled".into());
    let request = supergraph::Request::fake_builder()
        .query(QUERY_LIST_OF_LIST.to_string())
        .header("Apollo-Expose-Query-Plan", "true")
        .variables(variables)
        .build()
        .expect("expecting valid request");

    let response = supergraph_service
        .oneshot(request)
        .await
        .unwrap()
        .next_response()
        .await
        .unwrap();

    insta::assert_json_snapshot!(response);
}

#[tokio::test(flavor = "multi_thread")]
async fn test_type_conditions_disabled() {
    let harness = setup(json! {{
        "experimental_type_conditioned_fetching": false,
        // will make debugging easier
        "plugins": {
            "experimental.expose_query_plan": true
        },
        "include_subgraph_errors": {
            "all": true
        }
    }});
    let supergraph_service = harness.build_supergraph().await.unwrap();
    let mut variables = JsonMap::new();
    variables.insert("movieResultParam", "movieResultDisabled".into());
    variables.insert("articleResultParam", "articleResultDisabled".into());
    let request = supergraph::Request::fake_builder()
        .query(QUERY.to_string())
        .header("Apollo-Expose-Query-Plan", "true")
        .build()
        .expect("expecting valid request");

    let response = supergraph_service
        .oneshot(request)
        .await
        .unwrap()
        .next_response()
        .await
        .unwrap();

    insta::assert_json_snapshot!(response);
}

#[tokio::test(flavor = "multi_thread")]
async fn test_type_conditions_enabled_shouldnt_make_article_fetch() {
    let harness = setup_no_articles(json! {{
        "experimental_type_conditioned_fetching": true,
        // will make debugging easier
        "plugins": {
            "experimental.expose_query_plan": true
        },
        "include_subgraph_errors": {
            "all": true
        }
    }});
    let supergraph_service = harness.build_supergraph().await.unwrap();
    let mut variables = JsonMap::new();
    variables.insert("movieResultParam", "movieResultEnabled".into());
    variables.insert("articleResultParam", "articleResultEnabled".into());
    let request = supergraph::Request::fake_builder()
        .query(QUERY.to_string())
        .header("Apollo-Expose-Query-Plan", "true")
        .variables(variables)
        .build()
        .expect("expecting valid request");

    let response = supergraph_service
        .oneshot(request)
        .await
        .unwrap()
        .next_response()
        .await
        .unwrap();

    insta::assert_json_snapshot!(response);
}

fn setup_no_articles(configuration: serde_json::Value) -> TestHarness<'static> {
    let search_service =  MockSubgraph::builder().with_json(json!{{
        "query":"query Search__searchSubgraph__0{search{__typename ...on MovieResult{sections{__typename ...on EntityCollectionSection{__typename id}...on GallerySection{__typename id}}id}...on ArticleResult{id sections{__typename ...on GallerySection{__typename id}...on EntityCollectionSection{__typename id}}}}}",
        "operationName":"Search__searchSubgraph__0"
    }},
json!{{
        "data":{
            "search":[
                {
                    "__typename":"MovieResult",
                    "id":"c5f4985f-8fb6-4414-a3f5-56f7f58dd043",
                    "sections":[
                        {
                            "__typename":"EntityCollectionSection",
                            "id":"d9077ad2-d79a-45b5-b5ee-25ded226f03c"
                        },
                        {
                            "__typename":"EntityCollectionSection",
                            "id":"9f1f1ebb-21d3-4afe-bb7d-6de706f78f02"
                        }
                    ]
                },
                {
                    "__typename":"MovieResult",
                    "id":"ff140d35-ce5d-48fe-bad7-1cfb2c3e310a",
                    "sections":[
                        {
                            "__typename":"EntityCollectionSection",
                            "id":"24cea0de-2ac8-4cbe-85b6-8b1b80647c12"
                        },
                        {
                            "__typename":"GallerySection",
                            "id":"2f772201-42ca-4376-9871-2252cc052262"
                        }
                    ]
                }
            ]
        }
    }}).build();

    let artwork_service = MockSubgraph::builder()
    // Enabled has 1 query: on MovieResult only
    .with_json(json!{{
        "query":"query Search__artworkSubgraph__1($representations:[_Any!]!$movieResultParam:String){_entities(representations:$representations){...on EntityCollectionSection{title artwork(params:$movieResultParam)}...on GallerySection{artwork(params:$movieResultParam)}}}",
        "operationName":"Search__artworkSubgraph__1",
        "variables":{
            "movieResultParam":"movieResultEnabled",
            "representations":[
                {
                    "__typename":"EntityCollectionSection",
                    "id":"d9077ad2-d79a-45b5-b5ee-25ded226f03c"
                },
                {
                    "__typename":"EntityCollectionSection",
                    "id":"9f1f1ebb-21d3-4afe-bb7d-6de706f78f02"
                },
                {
                    "__typename":"EntityCollectionSection",
                    "id":"24cea0de-2ac8-4cbe-85b6-8b1b80647c12"
                },
                {
                    "__typename":"GallerySection",
                    "id":"2f772201-42ca-4376-9871-2252cc052262"
                }
            ]
        }
    }},
json!{{
    "data":{
        "_entities":[
            {
                "title": "d9077ad2-d79a-45b5-b5ee-25ded226f03c title",
                "artwork":"movieResultEnabled artwork"
            },
            {
                "title": "9f1f1ebb-21d3-4afe-bb7d-6de706f78f02 title",
                "artwork":"movieResultEnabled artwork"
            },
            {
                "title": "24cea0de-2ac8-4cbe-85b6-8b1b80647c12 title",
                "artwork":"movieResultEnabled artwork"
            },
            {
                "artwork":"movieResultEnabled artwork"
            }
        ]
    }
    }}).build();

    let mut mocks = MockedSubgraphs::default();
    mocks.insert("searchSubgraph", search_service);
    mocks.insert("artworkSubgraph", artwork_service);

    let schema = include_str!("fixtures/type_conditions.graphql");
    TestHarness::builder()
        .try_log_level("info")
        .configuration_json(configuration)
        .unwrap()
        .schema(schema)
        .extra_plugin(mocks)
}

fn setup(configuration: serde_json::Value) -> TestHarness<'static> {
    let search_service =  MockSubgraph::builder().with_json(json!{{
        "query":"query Search__searchSubgraph__0{search{__typename ...on MovieResult{sections{__typename ...on EntityCollectionSection{__typename id}...on GallerySection{__typename id}}id}...on ArticleResult{id sections{__typename ...on GallerySection{__typename id}...on EntityCollectionSection{__typename id}}}}}",
        "operationName":"Search__searchSubgraph__0"
    }},
json!{{
        "data":{
            "search":[
                {
                    "__typename":"ArticleResult",
                    "id":"a7052397-b605-414a-aba4-408d51c8eef0",
                    "sections":[
                        {
                            "__typename":"EntityCollectionSection",
                            "id":"d0182b8a-a671-4244-ba1c-905274b0d198"
                        },
                        {
                            "__typename":"EntityCollectionSection",
                            "id":"e6eec2fc-05ce-40a2-956b-f1335e615204"
                        }
                    ]
                },
                {
                    "__typename":"ArticleResult",
                    "id":"3a7b08c9-d8c0-4c55-b55d-596a272392e0",
                    "sections":[
                        {
                            "__typename":"EntityCollectionSection",
                            "id":"f44f584e-5d3d-4466-96f5-9afc3f5d5a54"
                        },
                        {
                            "__typename":"GallerySection",
                            "id":"e065e2b1-8454-4db9-89c8-48e66ec838c4"
                        }
                    ]
                },
                {
                    "__typename":"MovieResult",
                    "id":"c5f4985f-8fb6-4414-a3f5-56f7f58dd043",
                    "sections":[
                        {
                            "__typename":"EntityCollectionSection",
                            "id":"d9077ad2-d79a-45b5-b5ee-25ded226f03c"
                        },
                        {
                            "__typename":"EntityCollectionSection",
                            "id":"9f1f1ebb-21d3-4afe-bb7d-6de706f78f02"
                        }
                    ]
                },
                {
                    "__typename":"MovieResult",
                    "id":"ff140d35-ce5d-48fe-bad7-1cfb2c3e310a",
                    "sections":[
                        {
                            "__typename":"EntityCollectionSection",
                            "id":"24cea0de-2ac8-4cbe-85b6-8b1b80647c12"
                        },
                        {
                            "__typename":"GallerySection",
                            "id":"2f772201-42ca-4376-9871-2252cc052262"
                        }
                    ]
                }
            ]
        }
    }})
    .with_json(json!{{
        "query":"query Search__searchSubgraph__0{searchListOfList{__typename ...on MovieResult{sections{__typename ...on EntityCollectionSection{__typename id}...on GallerySection{__typename id}}id}...on ArticleResult{id sections{__typename ...on GallerySection{__typename id}...on EntityCollectionSection{__typename id}}}}}",
        "operationName":"Search__searchSubgraph__0"
    }},

    /*
    
    {"data":{"searchListOfList":[[{"__typename":"MovieResult","sections":[{"__typename":"EntityCollectionSection","id":"bbb583fa-b83c-4c17-9f7a-d4c8552146c4"},{"__typename":"EntityCollectionSection","id":"78558a2e-f8eb-45dd-8545-5eccede5db5d"}],"id":"e1a00eee-170b-466f-bbac-8162b97e6ae0"},{"__typename":"MovieResult","sections":[{"__typename":"GallerySection","id":"4cbf1986-a04c-4cb2-b33a-e515739f1bc3"},{"__typename":"GallerySection","id":"122414dd-ea4e-462a-93a0-08fceac8d5dc"}],"id":"febff033-8db0-4678-b519-be6d74eb1a02"}],[{"__typename":"ArticleResult","id":"67f0ae48-f3f3-4ec2-9d52-1cb5c1c5e881","sections":[{"__typename":"GallerySection","id":"aac2f723-f83a-4fbf-ac06-ab8122e43910"},{"__typename":"EntityCollectionSection","id":"b6a79481-6817-4fef-9d7e-d87be5f756fd"}]},{"__typename":"ArticleResult","id":"2aa5fd55-1a02-478b-82ee-978c288a4591","sections":[{"__typename":"GallerySection","id":"68b17d77-7a9b-4150-bdc0-435e35d6888c"},{"__typename":"GallerySection","id":"b6765df5-4fd7-4c4b-bdbe-bddf8c77251e"}]
    
     */
json!{{
        "data":{
            "searchListOfList":[
                [
                    {
                        "__typename":"ArticleResult",
                        "id":"a7052397-b605-414a-aba4-408d51c8eef0",
                        "sections":[
                            {
                                "__typename":"EntityCollectionSection",
                                "id":"d0182b8a-a671-4244-ba1c-905274b0d198"
                            },
                            {
                                "__typename":"EntityCollectionSection",
                                "id":"e6eec2fc-05ce-40a2-956b-f1335e615204"
                            }
                        ]
                    },
                    {
                        "__typename":"ArticleResult",
                        "id":"3a7b08c9-d8c0-4c55-b55d-596a272392e0",
                        "sections":[
                            {
                                "__typename":"EntityCollectionSection",
                                "id":"f44f584e-5d3d-4466-96f5-9afc3f5d5a54"
                            },
                            {
                                "__typename":"GallerySection",
                                "id":"e065e2b1-8454-4db9-89c8-48e66ec838c4"
                            }
                        ]
                    },
                    {
                        "__typename":"MovieResult",
                        "id":"c5f4985f-8fb6-4414-a3f5-56f7f58dd043",
                        "sections":[
                            {
                                "__typename":"EntityCollectionSection",
                                "id":"d9077ad2-d79a-45b5-b5ee-25ded226f03c"
                            },
                            {
                                "__typename":"EntityCollectionSection",
                                "id":"9f1f1ebb-21d3-4afe-bb7d-6de706f78f02"
                            }
                        ]
                    },
                    {
                        "__typename":"MovieResult",
                        "id":"ff140d35-ce5d-48fe-bad7-1cfb2c3e310a",
                        "sections":[
                            {
                                "__typename":"EntityCollectionSection",
                                "id":"24cea0de-2ac8-4cbe-85b6-8b1b80647c12"
                            },
                            {
                                "__typename":"GallerySection",
                                "id":"2f772201-42ca-4376-9871-2252cc052262"
                            }
                        ]
                    }
                ],
                [
                    {
                        "__typename":"ArticleResult",
                        "id":"a7052397-b605-414a-aba4-408d51c8eef0",
                        "sections":[
                            {
                                "__typename":"EntityCollectionSection",
                                "id":"d0182b8a-a671-4244-ba1c-905274b0d198"
                            },
                            {
                                "__typename":"EntityCollectionSection",
                                "id":"e6eec2fc-05ce-40a2-956b-f1335e615204"
                            }
                        ]
                    },
                    {
                        "__typename":"ArticleResult",
                        "id":"3a7b08c9-d8c0-4c55-b55d-596a272392e0",
                        "sections":[
                            {
                                "__typename":"EntityCollectionSection",
                                "id":"f44f584e-5d3d-4466-96f5-9afc3f5d5a54"
                            },
                            {
                                "__typename":"GallerySection",
                                "id":"e065e2b1-8454-4db9-89c8-48e66ec838c4"
                            }
                        ]
                    }
                ],
                [
                    {
                        "__typename":"MovieResult",
                        "id":"c5f4985f-8fb6-4414-a3f5-56f7f58dd043",
                        "sections":[
                            {
                                "__typename":"EntityCollectionSection",
                                "id":"d9077ad2-d79a-45b5-b5ee-25ded226f03c"
                            },
                            {
                                "__typename":"EntityCollectionSection",
                                "id":"9f1f1ebb-21d3-4afe-bb7d-6de706f78f02"
                            }
                        ]
                    }
                ],
                [
                    {
                        "__typename":"MovieResult",
                        "id":"ff140d35-ce5d-48fe-bad7-1cfb2c3e310a",
                        "sections":[
                            {
                                "__typename":"EntityCollectionSection",
                                "id":"24cea0de-2ac8-4cbe-85b6-8b1b80647c12"
                            },
                            {
                                "__typename":"GallerySection",
                                "id":"2f772201-42ca-4376-9871-2252cc052262"
                            }
                        ]
                    }
                ]
            ]
        }
    }}).build();

    let artwork_service = MockSubgraph::builder()
    // Enabled has 2 queries: first one is on MovieResult only
    .with_json(json!{{
        "query":"query Search__artworkSubgraph__1($representations:[_Any!]!$movieResultParam:String){_entities(representations:$representations){...on EntityCollectionSection{title artwork(params:$movieResultParam)}...on GallerySection{artwork(params:$movieResultParam)}}}",
        "operationName":"Search__artworkSubgraph__1",
        "variables":{
            "movieResultParam":"movieResultEnabled",
            "representations":[
                {
                    "__typename":"EntityCollectionSection",
                    "id":"d9077ad2-d79a-45b5-b5ee-25ded226f03c"
                },
                {
                    "__typename":"EntityCollectionSection",
                    "id":"9f1f1ebb-21d3-4afe-bb7d-6de706f78f02"
                },
                {
                    "__typename":"EntityCollectionSection",
                    "id":"24cea0de-2ac8-4cbe-85b6-8b1b80647c12"
                },
                {
                    "__typename":"GallerySection",
                    "id":"2f772201-42ca-4376-9871-2252cc052262"
                }
            ]
        }
    }},
json!{{
    "data":{
        "_entities":[
            {
                "title": "d9077ad2-d79a-45b5-b5ee-25ded226f03c title",
                "artwork":"movieResultEnabled artwork"
            },
            {
                "title": "9f1f1ebb-21d3-4afe-bb7d-6de706f78f02 title",
                "artwork":"movieResultEnabled artwork"
            },
            {
                "title": "24cea0de-2ac8-4cbe-85b6-8b1b80647c12 title",
                "artwork":"movieResultEnabled artwork"
            },
            {
                "artwork":"movieResultEnabled artwork"
            }
        ]
    }
    }})
    // ... and second one  one is on ArticleResult only
    .with_json(json!{{
        "query": "query Search__artworkSubgraph__2($representations:[_Any!]!$articleResultParam:String){_entities(representations:$representations){...on GallerySection{artwork(params:$articleResultParam)}...on EntityCollectionSection{artwork(params:$articleResultParam)title}}}",
        "operationName": "Search__artworkSubgraph__2",
        "variables":{
            "articleResultParam":"articleResultEnabled",
            "representations":[
                {
                    "__typename":"EntityCollectionSection",
                    "id":"d0182b8a-a671-4244-ba1c-905274b0d198"
                },
                {
                    "__typename":"EntityCollectionSection",
                    "id":"e6eec2fc-05ce-40a2-956b-f1335e615204"
                },
                {
                    "__typename":"EntityCollectionSection",
                    "id":"f44f584e-5d3d-4466-96f5-9afc3f5d5a54"
                },
                {
                    "__typename":"GallerySection",
                    "id":"e065e2b1-8454-4db9-89c8-48e66ec838c4"
                },
            ]
        }
        }},
        json!{
        {
            "data":{
                "_entities":[
                    {
                        "artwork":"articleResultEnabled artwork",
                        "title": "d0182b8a-a671-4244-ba1c-905274b0d198 title"
                    },
                    {
                        "artwork":"articleResultEnabled artwork",
                        "title": "e6eec2fc-05ce-40a2-956b-f1335e615204 title"
                    },

                    {
                        "artwork":"articleResultEnabled artwork",
                        "title": "f44f584e-5d3d-4466-96f5-9afc3f5d5a54 title"
                    },
                    {"artwork":"articleResultEnabled artwork"}
                ]
            }
        }
    })
    // Disabled, not great
    .with_json(json!{{
            "query":"query Search__artworkSubgraph__1($representations:[_Any!]!$movieResultParam:String){_entities(representations:$representations){...on EntityCollectionSection{title artwork(params:$movieResultParam)}...on GallerySection{artwork(params:$movieResultParam)}}}",
            "operationName":"Search__artworkSubgraph__1",
            "variables":{
                "representations":[
                    {
                        "__typename":"EntityCollectionSection",
                        "id":"d0182b8a-a671-4244-ba1c-905274b0d198"
                    },
                    {
                        "__typename":"EntityCollectionSection",
                        "id":"e6eec2fc-05ce-40a2-956b-f1335e615204"
                    },
                    {
                        "__typename":"EntityCollectionSection",
                        "id":"f44f584e-5d3d-4466-96f5-9afc3f5d5a54"
                    },
                    {
                        "__typename":"GallerySection",
                        "id":"e065e2b1-8454-4db9-89c8-48e66ec838c4"
                    },
                    {
                        "__typename":"EntityCollectionSection",
                        "id":"d9077ad2-d79a-45b5-b5ee-25ded226f03c"
                    },
                    {
                        "__typename":"EntityCollectionSection",
                        "id":"9f1f1ebb-21d3-4afe-bb7d-6de706f78f02"
                    },
                    {
                        "__typename":"EntityCollectionSection",
                        "id":"24cea0de-2ac8-4cbe-85b6-8b1b80647c12"
                    },
                    {
                        "__typename":"GallerySection",
                        "id":"2f772201-42ca-4376-9871-2252cc052262"
                    }
                ]
            }
        }},
        // can't mock according to variables because they're not even propagated here...
    json!{
        {
            "data": {
              "_entities": [
                {
                    "title":"d0182b8a-a671-4244-ba1c-905274b0d198 title",
                    "artwork":"Hello World",
                },
                {
                    "title":"e6eec2fc-05ce-40a2-956b-f1335e615204 title",
                    "artwork":"Hello World",
                },
                {
                    "title":"f44f584e-5d3d-4466-96f5-9afc3f5d5a54 title",
                    "artwork":"Hello World",
                },
                {
                    "artwork":"Hello World"
                },
                {
                    "title":"d9077ad2-d79a-45b5-b5ee-25ded226f03c title",
                    "artwork":"Hello World",
                },
                {
                    "title":"9f1f1ebb-21d3-4afe-bb7d-6de706f78f02 title",
                    "artwork":"Hello World",
                },
                {
                    "title":"24cea0de-2ac8-4cbe-85b6-8b1b80647c12 title",
                    "artwork":"Hello World",
                },
                {
                    "artwork":"Hello World"
                }
              ]
            }
        }
    }).build();

    let mut mocks = MockedSubgraphs::default();
    mocks.insert("searchSubgraph", search_service);
    mocks.insert("artworkSubgraph", artwork_service);

    let schema = include_str!("fixtures/type_conditions.graphql");
    TestHarness::builder()
        .try_log_level("info")
        .configuration_json(configuration)
        .unwrap()
        .schema(schema)
        .extra_plugin(mocks)
}

static QUERY: &str = r#"
query Search($movieResultParam: String, $articleResultParam: String) {
    search {
      ... on MovieResult {
        sections {
          ... on EntityCollectionSection {
            id
            title
            artwork(params: $movieResultParam)
          }
          ... on GallerySection {
            artwork(params: $movieResultParam)
            id
          }
        }
        id
      }
      ... on ArticleResult {
        id
        sections {
          ... on GallerySection {
            artwork(params: $articleResultParam)
          }
          ... on EntityCollectionSection {
            artwork(params: $articleResultParam)
            title
          }
        }
      }
    }
}"#;

static QUERY_LIST_OF_LIST: &str = r#"
query Search($movieResultParam: String, $articleResultParam: String) {
    searchListOfList {
      ... on MovieResult {
        sections {
          ... on EntityCollectionSection {
            id
            title
            artwork(params: $movieResultParam)
          }
          ... on GallerySection {
            artwork(params: $movieResultParam)
            id
          }
        }
        id
      }
      ... on ArticleResult {
        id
        sections {
          ... on GallerySection {
            artwork(params: $articleResultParam)
          }
          ... on EntityCollectionSection {
            artwork(params: $articleResultParam)
            title
          }
        }
      }
    }
}"#;
