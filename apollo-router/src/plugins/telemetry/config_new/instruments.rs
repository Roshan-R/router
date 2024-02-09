use std::collections::LinkedList;
use std::fmt::Debug;

use http::header::CONTENT_LENGTH;
use opentelemetry_api::metrics::Counter;
use opentelemetry_api::metrics::Histogram;
use opentelemetry_api::metrics::MeterProvider;
use opentelemetry_api::metrics::Unit;
use opentelemetry_api::metrics::UpDownCounter;
use opentelemetry_api::KeyValue;
use schemars::JsonSchema;
use serde::Deserialize;
use tower::BoxError;

use super::Selector;
use crate::metrics;
use crate::plugins::telemetry::config_new::attributes::DefaultAttributeRequirementLevel;
use crate::plugins::telemetry::config_new::attributes::RouterAttributes;
use crate::plugins::telemetry::config_new::attributes::SubgraphAttributes;
use crate::plugins::telemetry::config_new::attributes::SupergraphAttributes;
use crate::plugins::telemetry::config_new::conditions::Condition;
use crate::plugins::telemetry::config_new::extendable::Extendable;
use crate::plugins::telemetry::config_new::selectors::RouterSelector;
use crate::plugins::telemetry::config_new::selectors::SubgraphSelector;
use crate::plugins::telemetry::config_new::selectors::SupergraphSelector;
use crate::plugins::telemetry::config_new::Selectors;
use crate::services::router;

#[allow(dead_code)]
#[derive(Clone, Deserialize, JsonSchema, Debug, Default)]
#[serde(deny_unknown_fields, default)]
pub(crate) struct InstrumentsConfig {
    /// The attributes to include by default in instruments based on their level as specified in the otel semantic conventions and Apollo documentation.
    pub(crate) default_attribute_requirement_level: DefaultAttributeRequirementLevel,

    /// Router service instruments. For more information see documentation on Router lifecycle.
    pub(crate) router:
        Extendable<RouterInstrumentsConfig, Instrument<RouterAttributes, RouterSelector>>,
    /// Supergraph service instruments. For more information see documentation on Router lifecycle.
    pub(crate) supergraph:
        Extendable<SupergraphInstruments, Instrument<SupergraphAttributes, SupergraphSelector>>,
    /// Subgraph service instruments. For more information see documentation on Router lifecycle.
    pub(crate) subgraph:
        Extendable<SubgraphInstruments, Instrument<SubgraphAttributes, SubgraphSelector>>,
}

#[allow(dead_code)]
#[derive(Clone, Deserialize, JsonSchema, Debug, Default)]
#[serde(deny_unknown_fields, default)]
pub(crate) struct RouterInstrumentsConfig {
    /// Histogram of server request duration
    #[serde(rename = "http.server.request.duration")]
    http_server_request_duration:
        DefaultedStandardInstrument<Extendable<RouterAttributes, RouterSelector>>,

    /// Gauge of active requests
    #[serde(rename = "http.server.active_requests")]
    http_server_active_requests:
        DefaultedStandardInstrument<Extendable<RouterAttributes, RouterSelector>>,

    /// Histogram of server request body size
    #[serde(rename = "http.server.request.body.size")]
    http_server_request_body_size:
        DefaultedStandardInstrument<Extendable<RouterAttributes, RouterSelector>>,

    /// Histogram of server response body size
    #[serde(rename = "http.server.response.body.size")]
    http_server_response_body_size:
        DefaultedStandardInstrument<Extendable<RouterAttributes, RouterSelector>>,
}

#[allow(dead_code)]
#[derive(Clone, Deserialize, JsonSchema, Debug)]
#[serde(deny_unknown_fields, untagged)]
enum DefaultedStandardInstrument<T> {
    Bool(bool),
    Extendable { attributes: T },
}

impl<T> Default for DefaultedStandardInstrument<T> {
    fn default() -> Self {
        DefaultedStandardInstrument::Bool(true)
    }
}

impl<T> DefaultedStandardInstrument<T> {
    fn is_enabled(&self) -> bool {
        match self {
            Self::Bool(enabled) => *enabled,
            Self::Extendable { .. } => true,
        }
    }
}

impl<T, Request, Response> Selectors for DefaultedStandardInstrument<T>
where
    T: Selectors<Request = Request, Response = Response>,
{
    type Request = Request;
    type Response = Response;

    fn on_request(&self, request: &Self::Request) -> LinkedList<opentelemetry_api::KeyValue> {
        match self {
            Self::Bool(_) => LinkedList::new(),
            Self::Extendable { attributes } => attributes.on_request(request),
        }
    }

    fn on_response(&self, response: &Self::Response) -> LinkedList<opentelemetry_api::KeyValue> {
        match self {
            Self::Bool(_) => LinkedList::new(),
            Self::Extendable { attributes } => attributes.on_response(response),
        }
    }

    fn on_error(&self, error: &BoxError) -> LinkedList<opentelemetry_api::KeyValue> {
        match self {
            Self::Bool(_) => LinkedList::new(),
            Self::Extendable { attributes } => attributes.on_error(error),
        }
    }
}

#[allow(dead_code)]
#[derive(Clone, Deserialize, JsonSchema, Debug, Default)]
#[serde(deny_unknown_fields, default)]
struct SupergraphInstruments {}

#[allow(dead_code)]
#[derive(Clone, Deserialize, JsonSchema, Debug, Default)]
#[serde(deny_unknown_fields, default)]
struct SubgraphInstruments {
    /// Histogram of client request duration
    #[serde(rename = "http.client.request.duration")]
    http_client_request_duration: bool,

    /// Histogram of client request body size
    #[serde(rename = "http.client.request.body.size")]
    http_client_request_body_size: bool,

    /// Histogram of client response body size
    #[serde(rename = "http.client.response.body.size")]
    http_client_response_body_size: bool,
}

#[allow(dead_code)]
#[derive(Clone, Deserialize, JsonSchema, Debug)]
pub(crate) struct Instrument<A, E>
where
    A: Default + Debug,
    E: Debug,
{
    /// The type of instrument.
    #[serde(rename = "type")]
    ty: InstrumentType,

    /// The value of the instrument.
    value: InstrumentValue<E>,

    /// The description of the instrument.
    description: String,

    /// The units of the instrument, e.g. "ms", "bytes", "requests".
    unit: String,

    /// Attributes to include on the instrument.
    #[serde(default = "Extendable::empty::<A, E>")]
    attributes: Extendable<A, E>,

    /// The instrument conditions.
    #[serde(default = "Condition::empty::<E>")]
    condition: Condition<E>,
}

impl<A, E, Request, Response> Selectors for Instrument<A, E>
where
    A: Debug + Default + Selectors<Request = Request, Response = Response>,
    E: Debug + Selector<Request = Request, Response = Response>,
{
    type Request = Request;

    type Response = Response;

    fn on_request(&self, request: &Self::Request) -> LinkedList<opentelemetry_api::KeyValue> {
        todo!()
    }

    fn on_response(&self, response: &Self::Response) -> LinkedList<opentelemetry_api::KeyValue> {
        todo!()
    }

    fn on_error(&self, error: &BoxError) -> LinkedList<opentelemetry_api::KeyValue> {
        todo!()
    }
}

#[allow(dead_code)]
#[derive(Clone, Deserialize, JsonSchema, Debug)]
#[serde(deny_unknown_fields, rename_all = "snake_case")]
pub(crate) enum InstrumentType {
    /// A monotonic counter https://opentelemetry.io/docs/specs/otel/metrics/data-model/#sums
    Counter,

    // /// A counter https://opentelemetry.io/docs/specs/otel/metrics/data-model/#sums
    // UpDownCounter,
    /// A histogram https://opentelemetry.io/docs/specs/otel/metrics/data-model/#histogram
    Histogram,
    // /// A gauge https://opentelemetry.io/docs/specs/otel/metrics/data-model/#gauge
    // Gauge,
}

#[allow(dead_code)]
#[derive(Clone, Deserialize, JsonSchema, Debug)]
#[serde(deny_unknown_fields, rename_all = "snake_case", untagged)]
pub(crate) enum InstrumentValue<T> {
    Standard(Standard),
    Custom(T),
}

#[allow(dead_code)]
#[derive(Clone, Deserialize, JsonSchema, Debug)]
#[serde(deny_unknown_fields, rename_all = "snake_case")]
pub(crate) enum Standard {
    Duration,
    Unit,
    // Active,
}

struct ActiveRequestGuard(UpDownCounter<i64>, Vec<KeyValue>);

impl ActiveRequestGuard {
    fn new(counter: UpDownCounter<i64>, attrs: Vec<KeyValue>) -> Self {
        counter.add(1, &attrs);
        Self(counter, attrs)
    }
}

impl Drop for ActiveRequestGuard {
    fn drop(&mut self) {
        self.0.add(-1, &self.1);
    }
}

pub(crate) trait Instrumented {
    type Request;
    type Response;

    fn on_request(&self, request: &Self::Request);
    fn on_response(&self, response: &Self::Response);
    fn on_error(&self, error: &BoxError);
}

impl Instrumented for RouterInstrumentsConfig {
    type Request = router::Request;
    type Response = router::Response;

    fn on_request(&self, request: &Self::Request) {
        let meter = metrics::meter_provider().meter("apollo/router");
        if self.http_server_active_requests.is_enabled() {
            let attrs = self
                .http_server_active_requests
                .on_request(request)
                .into_iter()
                .collect::<Vec<_>>();
            let active_req_guard = ActiveRequestGuard::new(
                meter
                    .i64_up_down_counter("http.server.active_requests")
                    .init(),
                attrs,
            );
            request.context.extensions().lock().insert(active_req_guard);
        }

        if self.http_server_request_body_size.is_enabled() {
            let body_size = request
                .router_request
                .headers()
                .get(&CONTENT_LENGTH)
                .and_then(|val| val.to_str().ok()?.parse::<u64>().ok());
            if let Some(body_size) = body_size {
                match meter
                    .u64_histogram("http.server.request.body.size")
                    .try_init()
                {
                    Ok(histogram) => {
                        let attrs = self
                            .http_server_request_body_size
                            .on_request(request)
                            .into_iter()
                            .collect::<Vec<_>>();
                        histogram.record(body_size, &attrs);
                    }
                    Err(err) => {
                        tracing::error!(
                            "cannot create gauge for 'http.server.request.body.size': {err:?}"
                        );
                    }
                }
            }
        }
    }

    fn on_response(&self, response: &Self::Response) {
        let meter = metrics::meter_provider().meter("apollo/router");
        if self.http_server_request_duration.is_enabled() {
            let attrs = self
                .http_server_request_duration
                .on_response(response)
                .into_iter()
                .collect::<Vec<_>>();
            let request_duration = response.context.busy_time();
            match meter
                .f64_histogram("http.server.request.duration")
                .with_unit(Unit::new("s"))
                .try_init()
            {
                Ok(histogram) => histogram.record(request_duration.as_secs_f64(), &attrs),
                Err(_) => todo!(),
            }
        }

        if self.http_server_response_body_size.is_enabled() {
            let body_size = response
                .response
                .headers()
                .get(&CONTENT_LENGTH)
                .and_then(|val| val.to_str().ok()?.parse::<u64>().ok());
            if let Some(body_size) = body_size {
                match meter
                    .u64_histogram("http.server.response.body.size")
                    .try_init()
                {
                    Ok(histogram) => {
                        let attrs = self
                            .http_server_response_body_size
                            .on_response(response)
                            .into_iter()
                            .collect::<Vec<_>>();
                        histogram.record(body_size, &attrs);
                    }
                    Err(err) => {
                        tracing::error!(
                            "cannot create gauge for 'http.server.response.body.size': {err:?}"
                        );
                    }
                }
            }
        }
    }

    fn on_error(&self, error: &BoxError) {
        let meter = metrics::meter_provider().meter("apollo/router");
        // FIXME: Can't use the context here
        // if self.http_server_request_duration.is_enabled() {
        //     let attrs = self
        //         .http_server_request_duration
        //         .on_response(response)
        //         .into_iter()
        //         .collect::<Vec<_>>();
        //     let request_duration = response.context.busy_time();
        //     match meter
        //         .f64_histogram("http.server.request.duration")
        //         .with_unit(Unit::new("s"))
        //         .try_init()
        //     {
        //         Ok(histogram) => histogram.record(request_duration.as_secs_f64(), &attrs),
        //         Err(_) => todo!(),
        //     }
        // }
    }
}

impl<A, B, E, Request, Response> Instrumented for Extendable<A, Instrument<B, E>>
where
    A: Default + Instrumented<Request = Request, Response = Response>,
    B: Default + Debug + Selectors<Request = Request, Response = Response>,
    E: Debug + Selector<Request = Request, Response = Response>,
{
    type Request = Request;
    type Response = Response;

    fn on_request(&self, request: &Self::Request) {
        self.attributes.on_request(request);
        // TODO custom
        // for (key, instr) in &self.custom {
        //     let attrs = instr.on_request(request);

        // }
    }

    fn on_response(&self, response: &Self::Response) {
        self.attributes.on_response(response);
        // TODO custom
    }

    fn on_error(&self, error: &BoxError) {
        self.attributes.on_error(error);
        // TODO custom
    }
}

impl Selectors for RouterInstrumentsConfig {
    type Request = router::Request;
    type Response = router::Response;

    fn on_request(&self, request: &Self::Request) -> LinkedList<opentelemetry_api::KeyValue> {
        let mut attrs = self.http_server_active_requests.on_request(request);
        attrs.extend(self.http_server_request_body_size.on_request(request));
        attrs.extend(self.http_server_request_duration.on_request(request));
        attrs.extend(self.http_server_response_body_size.on_request(request));

        attrs
    }

    fn on_response(&self, response: &Self::Response) -> LinkedList<opentelemetry_api::KeyValue> {
        let mut attrs = self.http_server_active_requests.on_response(response);
        attrs.extend(self.http_server_request_body_size.on_response(response));
        attrs.extend(self.http_server_request_duration.on_response(response));
        attrs.extend(self.http_server_response_body_size.on_response(response));

        attrs
    }

    fn on_error(&self, error: &BoxError) -> LinkedList<opentelemetry_api::KeyValue> {
        let mut attrs = self.http_server_active_requests.on_error(error);
        attrs.extend(self.http_server_request_body_size.on_error(error));
        attrs.extend(self.http_server_request_duration.on_error(error));
        attrs.extend(self.http_server_response_body_size.on_error(error));

        attrs
    }
}

#[derive(Debug, Clone)]
struct RouterInstruments {
    /// Histogram of server request duration
    http_server_request_duration: Histogram<f64>,
    /// Gauge of active requests
    http_server_active_requests: UpDownCounter<i64>,
    /// Histogram of server request body size
    http_server_request_body_size: Histogram<u64>,
    /// Histogram of server response body size
    http_server_response_body_size: Histogram<u64>,
    /// Config
    config: RouterInstrumentsConfig,
}

struct CustomInstruments {
    counters: Vec<Counter<u64>>,
    histograms: Vec<Histogram<f64>>,
}
