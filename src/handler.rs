// Raw handler trait — receives XML bytes, returns XML bytes or SoapFault
use async_trait::async_trait;
use bytes::Bytes;
use crate::fault::SoapFault;
use std::future::Future;

/// A SOAP operation handler that receives the Body first child element as
/// self-contained XML bytes (all ancestor namespace declarations re-emitted
/// on the root) and returns response body XML bytes or a SoapFault.
#[async_trait]
pub trait SoapHandler: Send + Sync + 'static {
    async fn handle(&self, body: Bytes) -> Result<Bytes, SoapFault>;
}

/// Wraps a closure as a SoapHandler for ergonomic registration.
pub struct FnHandler<F> {
    f: F,
}

impl<F, Fut> FnHandler<F>
where
    F: Fn(Bytes) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = Result<Bytes, SoapFault>> + Send + 'static,
{
    pub fn new(f: F) -> Self {
        Self { f }
    }
}

#[async_trait]
impl<F, Fut> SoapHandler for FnHandler<F>
where
    F: Fn(Bytes) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = Result<Bytes, SoapFault>> + Send + 'static,
{
    async fn handle(&self, body: Bytes) -> Result<Bytes, SoapFault> {
        (self.f)(body).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fault::{FaultCode, SoapFault};

    #[tokio::test]
    async fn fn_handler_ok_passthrough() {
        let handler = FnHandler::new(|body: Bytes| async move {
            Ok::<Bytes, SoapFault>(body)
        });
        let input = Bytes::from_static(b"<hello/>");
        let result = handler.handle(input.clone()).await.unwrap();
        assert_eq!(result, input);
    }

    #[tokio::test]
    async fn fn_handler_returns_fault() {
        let handler = FnHandler::new(|_body: Bytes| async move {
            Err::<Bytes, SoapFault>(SoapFault::sender("not allowed"))
        });
        let result = handler.handle(Bytes::from_static(b"<test/>")).await;
        assert!(result.is_err());
        let fault = result.unwrap_err();
        assert_eq!(fault.code, FaultCode::Sender);
        assert!(fault.reason.contains("not allowed"));
    }

    #[tokio::test]
    async fn fn_handler_closure_captures_context() {
        let expected_response = Bytes::from_static(b"<response>ok</response>");
        let resp_clone = expected_response.clone();
        let handler = FnHandler::new(move |_body: Bytes| {
            let resp = resp_clone.clone();
            async move { Ok::<Bytes, SoapFault>(resp) }
        });
        let result = handler.handle(Bytes::from_static(b"<request/>")).await.unwrap();
        assert_eq!(result, expected_response);
    }

    #[tokio::test]
    async fn soap_handler_trait_object() {
        // Verify FnHandler can be used as a trait object
        let handler: Box<dyn SoapHandler> = Box::new(FnHandler::new(|_body: Bytes| async move {
            Ok::<Bytes, SoapFault>(Bytes::from_static(b"<resp/>"))
        }));
        let result = handler.handle(Bytes::from_static(b"<req/>")).await.unwrap();
        assert_eq!(result, Bytes::from_static(b"<resp/>"));
    }
}
