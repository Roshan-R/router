#![allow(dead_code, unreachable_pub)]
#![allow(missing_docs)] // FIXME

use std::panic;
use std::sync::Arc;
use std::thread;

use futures::channel::mpsc;
use futures::channel::oneshot;
use futures::future::BoxFuture;
use futures::future::Future;
use futures::lock::Mutex;
use futures::SinkExt;
use futures::StreamExt;

use hyper::Body;
use hyper::Request as HyperRequest;
use hyper::Response as HyperResponse;
use tokio::runtime::Handle;
use tokio::runtime::RuntimeFlavor;
use tokio::task::JoinError;
use tower::Service;

use crate::services::ExecutionRequest;
use crate::services::ExecutionResponse;
#[cfg(test)]
use crate::services::HasSchema;
use crate::services::RouterRequest;
use crate::services::RouterResponse;
use crate::services::SubgraphRequest;
use crate::services::SubgraphResponse;
use crate::services::SupergraphRequest;
use crate::services::SupergraphResponse;
#[cfg(test)]
use crate::spec::Schema;

/// Build a mock service handler for the router pipeline.
macro_rules! mock_service {
    ($name:ident, $request_type:ty, $response_type:ty) => {
        paste::item! {
            mockall::mock! {
                #[derive(Debug)]
                #[allow(dead_code)]
                pub [<$name Service>] {
                    pub fn call(&mut self, req: $request_type) -> Result<$response_type, tower::BoxError>;
                }

                #[allow(dead_code)]
                impl Clone for [<$name Service>] {
                    fn clone(&self) -> [<Mock $name Service>];
                }
            }

            // mockall does not handle well the lifetime on Context
            impl tower::Service<$request_type> for [<Mock $name Service>] {
                type Response = $response_type;
                type Error = tower::BoxError;
                type Future = futures::future::BoxFuture<'static, Result<Self::Response, Self::Error>>;

                fn poll_ready(&mut self, _cx: &mut std::task::Context<'_>) -> std::task::Poll<Result<(), tower::BoxError>> {
                    std::task::Poll::Ready(Ok(()))
                }
                #[track_caller]
                fn call(&mut self, req: $request_type) -> Self::Future {
                    let r  = self.call(req);
                    Box::pin(async move { r })
                }
            }
        }
    };
}

macro_rules! mock_async_service {
    ($name:ident, $request_type:tt < $req_generic:tt > , $response_type:tt < $res_generic:tt >) => {
        paste::item! {
            mockall::mock! {
                #[derive(Debug)]
                #[allow(dead_code)]
                pub [<$name Service>] {
                    pub fn call(&mut self, req: $request_type<$req_generic>) -> impl Future<Output = Result<$response_type<$res_generic>, tower::BoxError>> + Send + 'static;
                }

                #[allow(dead_code)]
                impl Clone for [<$name Service>] {
                    fn clone(&self) -> [<Mock $name Service>];
                }
            }


            // mockall does not handle well the lifetime on Context
            impl tower::Service<$request_type<$req_generic>> for [<Mock $name Service>] {
                type Response = $response_type<$res_generic>;
                type Error = tower::BoxError;
                type Future = futures::future::BoxFuture<'static, Result<Self::Response, Self::Error>>;

                fn poll_ready(&mut self, _cx: &mut std::task::Context<'_>) -> std::task::Poll<Result<(), tower::BoxError>> {
                    std::task::Poll::Ready(Ok(()))
                }
                fn call(&mut self, req: $request_type<$req_generic>) -> Self::Future {
                    Box::pin(self.call(req))
                }
            }
        }
    };
}

#[cfg(test)]
impl HasSchema for MockSupergraphService {
    fn schema(&self) -> Arc<crate::spec::Schema> {
        Arc::new(
            Schema::parse_test(
                include_str!("../../testdata/supergraph.graphql"),
                &Default::default(),
            )
            .unwrap(),
        )
    }
}

mock_service!(Router, RouterRequest, RouterResponse);
mock_service!(Supergraph, SupergraphRequest, SupergraphResponse);
mock_service!(Execution, ExecutionRequest, ExecutionResponse);
mock_service!(Subgraph, SubgraphRequest, SubgraphResponse);
mock_async_service!(HttpClient, HyperRequest<Body>, HyperResponse<Body>);

type MockServiceMessage<Request, Response, Error> = (
    Request,
    oneshot::Sender<thread::Result<Result<Response, Error>>>,
);

pub struct MockService<Request, Response, Error>
where
    Request: Send,
    Response: Send,
    Error: Send,
{
    inner: Arc<InnerMockService<Request, Response, Error>>,
}

impl<Request, Response, Error> Clone for MockService<Request, Response, Error>
where
    Request: Send,
    Response: Send,
    Error: Send,
{
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
        }
    }
}

impl<Request, Response, Error> MockService<Request, Response, Error>
where
    Request: Send + 'static,
    Response: Send + 'static,
    Error: Send + 'static,
{
    pub fn create<F>(mut closure: F) -> Self
    where
        F: FnMut(Request) -> Result<Response, Error> + Send + 'static,
    {
        if Handle::current().runtime_flavor() != RuntimeFlavor::MultiThread {
            panic!("this MockService can only work under the 'multi_thread' runtime");
        }

        let (tx, mut rx) = mpsc::channel::<MockServiceMessage<Request, Response, Error>>(100);
        let (drop_tx, drop_rx) = oneshot::channel();

        let store_sender = Arc::new(Mutex::new(None));

        tokio::task::spawn(async move {
            let store = store_sender.clone();
            if let Err(e) = tokio::task::spawn(async move {
                while let Some((request, sender)) = rx.next().await {
                    *store.lock().await = Some(sender);
                    let res = closure(request);
                    let sender = store.lock().await.take().unwrap();
                    let _ = sender.send(Ok(res));
                }
            })
            .await
            {
                println!("got join error");
                drop_tx.send(e).unwrap();
            }
        });

        Self {
            inner: Arc::new(InnerMockService {
                tx: Some(tx),
                drop_rx: Some(drop_rx),
            }),
        }
    }
}

impl<Request, Response, Error> Service<Request> for MockService<Request, Response, Error>
where
    Request: Send + 'static,
    Response: Send + 'static,
    Error: Send + 'static,
{
    type Response = Response;

    type Error = Error;

    type Future = BoxFuture<'static, Result<Response, Error>>;

    fn poll_ready(
        &mut self,
        _cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<(), Self::Error>> {
        std::task::Poll::Ready(Ok(()))
    }

    #[track_caller]
    fn call(&mut self, request: Request) -> Self::Future {
        let (sender, receiver) = oneshot::channel();

        let mut tx = self.inner.tx.clone().unwrap();

        Box::pin(async move {
            tx.send((request, sender))
                .await
                .expect("mock service task closed");

            receiver
                .await
                .expect("oneshot sender dropped")
                .expect("mock service panicked")
        })
    }
}

struct InnerMockService<Request, Response, Error>
where
    Request: Send,
    Response: Send,
    Error: Send,
{
    tx: Option<mpsc::Sender<MockServiceMessage<Request, Response, Error>>>,
    drop_rx: Option<oneshot::Receiver<JoinError>>,
}

impl<Request, Response, Error> Drop for InnerMockService<Request, Response, Error>
where
    Request: Send,
    Response: Send,
    Error: Send,
{
    fn drop(&mut self) {
        println!("will drop tx");
        drop(self.tx.take());
        //std::thread::sleep(std::time::Duration::from_millis(100));
        println!("will try to receive");
        match self.drop_rx.take().unwrap().try_recv() {
            Ok(e) => panic!("MockService panicked: {e:?}"),
            res => {
                println!("did not receive")
            }
        }
    }
}

#[tokio::test(flavor = "multi_thread")]
#[should_panic]
async fn mockservice_catches_panic_on_drop() {
    use tower::ServiceExt;

    let expected_label = "from_mock_service";

    let mut exec = MockExecutionService::new();

    exec.expect_call()
        .times(2)
        .returning(move |req: ExecutionRequest| {
            Ok(ExecutionResponse::fake_builder()
                .label(expected_label.to_string())
                .context(req.context)
                .build()
                .unwrap())
        });

    let execution_service: MockService<ExecutionRequest, ExecutionResponse, tower::BoxError> =
        MockService::create(move |req| exec.call(req));

    let request = ExecutionRequest::fake_builder().build();

    execution_service
        .oneshot(request)
        .await
        .unwrap()
        .next_response()
        .await
        .unwrap()
        .label
        .unwrap();
}

#[tokio::test]
#[should_panic]
async fn mock_service_requires_multithreaded_runtime() {
    let expected_label = "from_mock_service";

    let mut exec = MockExecutionService::new();

    exec.expect_call()
        .times(1)
        .returning(move |req: ExecutionRequest| {
            Ok(ExecutionResponse::fake_builder()
                .label(expected_label.to_string())
                .context(req.context)
                .build()
                .unwrap())
        });

    MockService::create(move |req| exec.call(req));
}
