// Raw handler trait — receives XML bytes, returns XML bytes or SoapFault
use crate::fault::SoapFault;
use async_trait::async_trait;
use bytes::Bytes;
use std::future::Future;

/// A SOAP operation handler that receives the Body first child element as
/// self-contained XML bytes (all ancestor namespace declarations re-emitted
/// on the root) and returns response body XML bytes or a SoapFault.
#[async_trait]
pub trait SoapHandler: Send + Sync + 'static {
    async fn handle(&self, body: Bytes) -> Result<Bytes, SoapFault>;

    /// Handle a request with access to the SOAP header element fragments (each is the
    /// raw bytes of one direct child of `<Header>`). Defaults to ignoring headers and
    /// calling `handle`. Handlers needing WS-Addressing/WS-Security header data override this.
    async fn handle_with_headers(
        &self,
        body: Bytes,
        _headers: &[Bytes],
    ) -> Result<Bytes, SoapFault> {
        self.handle(body).await
    }
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
    use std::sync::{Arc, Mutex};

    #[tokio::test]
    async fn fn_handler_ok_passthrough() {
        let handler = FnHandler::new(|body: Bytes| async move { Ok::<Bytes, SoapFault>(body) });
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
        let result = handler
            .handle(Bytes::from_static(b"<request/>"))
            .await
            .unwrap();
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

    // ── handle_with_headers tests (round-2 #5) ────────────────────────────────

    /// A handler that overrides handle_with_headers, records received header fragments,
    /// and returns a fixed response.
    struct HeaderCapturingHandler {
        captured_headers: Arc<Mutex<Vec<Vec<u8>>>>,
    }

    #[async_trait::async_trait]
    impl SoapHandler for HeaderCapturingHandler {
        async fn handle(&self, _body: Bytes) -> Result<Bytes, SoapFault> {
            Ok(Bytes::from_static(b"<resp/>"))
        }

        async fn handle_with_headers(
            &self,
            _body: Bytes,
            headers: &[Bytes],
        ) -> Result<Bytes, SoapFault> {
            let mut captured = self.captured_headers.lock().unwrap();
            for h in headers {
                captured.push(h.to_vec());
            }
            Ok(Bytes::from_static(b"<resp-with-headers/>"))
        }
    }

    #[tokio::test]
    async fn handle_with_headers_override_receives_header_fragments() {
        let captured = Arc::new(Mutex::new(Vec::<Vec<u8>>::new()));
        let handler = HeaderCapturingHandler {
            captured_headers: captured.clone(),
        };

        let header1 = Bytes::from_static(b"<wsa:To>http://example.com/service</wsa:To>");
        let header2 = Bytes::from_static(b"<tev:SubscriptionId>sub-123</tev:SubscriptionId>");
        let headers = [header1.clone(), header2.clone()];

        let result = handler
            .handle_with_headers(Bytes::from_static(b"<req/>"), &headers)
            .await
            .unwrap();

        // Overriding handler produces its own response.
        assert_eq!(result, Bytes::from_static(b"<resp-with-headers/>"));

        // Both header fragments were delivered.
        let seen = captured.lock().unwrap();
        assert_eq!(seen.len(), 2);
        assert_eq!(seen[0], header1.as_ref());
        assert_eq!(seen[1], header2.as_ref());
    }

    #[tokio::test]
    async fn handle_with_headers_default_falls_through_to_handle() {
        // FnHandler does NOT override handle_with_headers — the default impl must
        // delegate to handle() and the handler response must come through unchanged.
        let handler = FnHandler::new(|body: Bytes| async move {
            // Echo back the body to prove handle() was actually called.
            Ok::<Bytes, SoapFault>(body)
        });

        let body = Bytes::from_static(b"<echo>data</echo>");
        let header = Bytes::from_static(b"<some:Header>ignored</some:Header>");

        let result = handler
            .handle_with_headers(body.clone(), &[header])
            .await
            .unwrap();

        // Default impl calls handle(body) — should receive the body back.
        assert_eq!(result, body);
    }
}
