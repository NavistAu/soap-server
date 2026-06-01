//! Builds the soap-server System-Under-Test from the controlled fixture (spec §5.8)
//! and replays requests against it in-process via axum_test.

use axum_test::TestServer;
use bytes::Bytes;
use soap_server::{FnHandler, ServerBuilder};

pub const CONTROLLED_WSDL: &[u8] = include_bytes!("../fixtures/controlled.wsdl");
pub const MULTI_SERVICE_WSDL: &[u8] = include_bytes!("../fixtures/multi_service.wsdl");

pub struct Response {
    pub status: u16,
    pub content_type: String,
    pub body: Vec<u8>,
}

impl Response {
    pub fn body_utf8(&self) -> String {
        String::from_utf8_lossy(&self.body).into_owned()
    }
}

pub struct Sut {
    server: TestServer,
}

impl Sut {
    pub async fn replay(&self, path: &str, body: &[u8], content_type: &str) -> Response {
        let r = self
            .server
            .post(path)
            .content_type(content_type)
            .bytes(Bytes::copy_from_slice(body))
            .await;
        let content_type = r
            .headers()
            .get("content-type")
            .and_then(|h| h.to_str().ok())
            .unwrap_or("")
            .to_string();
        Response {
            status: r.status_code().as_u16(),
            content_type,
            body: r.as_bytes().to_vec(),
        }
    }

    /// Issue a GET request to `path?wsdl` and return the response.
    /// Used for wsdl_rewrite_* scenarios (Group E).
    pub async fn replay_get_wsdl(&self, path: &str) -> Response {
        let r = self.server.get(path).add_query_param("wsdl", "").await;
        let content_type = r
            .headers()
            .get("content-type")
            .and_then(|h| h.to_str().ok())
            .unwrap_or("")
            .to_string();
        Response {
            status: r.status_code().as_u16(),
            content_type,
            body: r.as_bytes().to_vec(),
        }
    }
}

/// Extract the text content of the first element whose local name ends with "Text".
fn extract_text(body: &[u8]) -> Option<String> {
    use quick_xml::events::Event;
    use quick_xml::Reader;

    let mut reader = Reader::from_reader(body);
    reader.config_mut().trim_text(true);
    let mut in_text = false;
    loop {
        match reader.read_event() {
            Ok(Event::Start(e)) => {
                let local = e.local_name();
                let local_str = std::str::from_utf8(local.as_ref()).unwrap_or("");
                if local_str.ends_with("Text") {
                    in_text = true;
                }
            }
            Ok(Event::Text(t)) if in_text => {
                return Some(t.decode().unwrap_or_default().into_owned());
            }
            Ok(Event::End(_)) if in_text => return None,
            Ok(Event::Eof) => return None,
            Err(_) => return None,
            _ => {}
        }
    }
}

fn echo_handler() -> impl soap_server::SoapHandler {
    FnHandler::new(|body: Bytes| async move {
        let text = extract_text(&body).unwrap_or_default();
        let escaped = soap_server::escape_text(&text);
        let resp = format!(
            r#"<c:EchoResponse xmlns:c="http://crossref.example/controlled"><c:Text>{escaped}</c:Text></c:EchoResponse>"#
        );
        Ok::<Bytes, soap_server::SoapFault>(Bytes::from(resp))
    })
}

/// Extract the text content of the first element whose local name ends with "Value".
fn extract_value(body: &[u8]) -> Option<String> {
    use quick_xml::events::Event;
    use quick_xml::Reader;

    let mut reader = Reader::from_reader(body);
    reader.config_mut().trim_text(true);
    let mut in_value = false;
    loop {
        match reader.read_event() {
            Ok(Event::Start(e)) => {
                let local = e.local_name();
                let local_str = std::str::from_utf8(local.as_ref()).unwrap_or("");
                if local_str.ends_with("Value") {
                    in_value = true;
                }
            }
            Ok(Event::Text(t)) if in_value => {
                return Some(t.decode().unwrap_or_default().into_owned());
            }
            Ok(Event::End(_)) if in_value => return None,
            Ok(Event::Eof) => return None,
            Err(_) => return None,
            _ => {}
        }
    }
}

fn echo_named_handler() -> impl soap_server::SoapHandler {
    FnHandler::new(|body: Bytes| async move {
        let value = extract_value(&body).unwrap_or_default();
        let escaped = soap_server::escape_text(&value);
        let resp = format!(
            r#"<c:EchoNamedResponse xmlns:c="http://crossref.example/controlled"><c:Value>{escaped}</c:Value></c:EchoNamedResponse>"#
        );
        Ok::<Bytes, soap_server::SoapFault>(Bytes::from(resp))
    })
}

pub fn build_controlled_sut() -> Sut {
    let svc = ServerBuilder::from_wsdl_bytes(CONTROLLED_WSDL.to_vec())
        .path("/soap")
        .handler("Echo", echo_handler())
        .handler("EchoNamed", echo_named_handler())
        .build()
        .expect("controlled WSDL should build without error");
    let server = TestServer::new(svc.into_router());
    Sut { server }
}

/// Build the authed controlled SUT with a lenient timestamp tolerance (~100 years),
/// so that static request fixtures with a fixed Created timestamp never expire.
/// Used for wssec_digest_success, wssec_bad_password, and wssec_missing_auth scenarios.
pub fn build_controlled_sut_authed() -> Sut {
    let svc = ServerBuilder::from_wsdl_bytes(CONTROLLED_WSDL.to_vec())
        .path("/soap")
        .handler("Echo", echo_handler())
        .handler("EchoNamed", echo_named_handler())
        .auth(|user| (user == "alice").then(|| "secret".to_string()))
        // ~100 years in seconds so fixed Created never goes stale
        .timestamp_tolerance_secs(3_153_600_000)
        .build()
        .expect("authed controlled WSDL should build without error");
    let server = TestServer::new(svc.into_router());
    Sut { server }
}

/// Build the authed controlled SUT with a tight (300 s) timestamp tolerance.
/// Used for wssec_stale_timestamp — the fixed Created "2000-01-01T00:00:00.000Z"
/// is decades in the past and must be rejected.
pub fn build_controlled_sut_authed_strict() -> Sut {
    let svc = ServerBuilder::from_wsdl_bytes(CONTROLLED_WSDL.to_vec())
        .path("/soap")
        .handler("Echo", echo_handler())
        .handler("EchoNamed", echo_named_handler())
        .auth(|user| (user == "alice").then(|| "secret".to_string()))
        .timestamp_tolerance_secs(300)
        .build()
        .expect("authed-strict controlled WSDL should build without error");
    let server = TestServer::new(svc.into_router());
    Sut { server }
}

/// Build the multi-service SUT from multi_service.wsdl (ServiceA at /soap/a, ServiceB at /soap/b).
/// Used for wsdl_rewrite_multi scenarios (Group E).
pub fn build_multi_service_sut() -> Sut {
    let svc = ServerBuilder::from_wsdl_bytes(MULTI_SERVICE_WSDL.to_vec())
        .handler(
            "OpA",
            FnHandler::new(|_body: Bytes| async move {
                Ok::<Bytes, soap_server::SoapFault>(Bytes::from_static(
                    b"<tns:OpAResponse xmlns:tns=\"http://example.com/multi\"/>",
                ))
            }),
        )
        .handler(
            "OpB",
            FnHandler::new(|_body: Bytes| async move {
                Ok::<Bytes, soap_server::SoapFault>(Bytes::from_static(
                    b"<tns:OpBResponse xmlns:tns=\"http://example.com/multi\"/>",
                ))
            }),
        )
        .build()
        .expect("multi-service WSDL should build without error");
    let server = TestServer::new(svc.into_router());
    Sut { server }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn echo_success_returns_echoresponse() {
        let sut = build_controlled_sut();
        let body = br#"<env:Envelope xmlns:env="http://www.w3.org/2003/05/soap-envelope"><env:Body><c:Echo xmlns:c="http://crossref.example/controlled"><c:Text>hi</c:Text></c:Echo></env:Body></env:Envelope>"#;
        let resp = sut
            .replay("/soap", body, "application/soap+xml; charset=utf-8")
            .await;
        assert_eq!(resp.status, 200);
        assert!(resp.body_utf8().contains("EchoResponse"));
        assert!(resp.body_utf8().contains("hi"));
    }

    #[tokio::test]
    async fn echo_named_success_returns_echonamedresponse() {
        let sut = build_controlled_sut();
        let body = br#"<env:Envelope xmlns:env="http://www.w3.org/2003/05/soap-envelope"><env:Body><c:EchoNamed xmlns:c="http://crossref.example/controlled"><c:Value>named_value</c:Value></c:EchoNamed></env:Body></env:Envelope>"#;
        let resp = sut
            .replay("/soap", body, "application/soap+xml; charset=utf-8")
            .await;
        assert_eq!(resp.status, 200);
        assert!(resp.body_utf8().contains("EchoNamedResponse"));
        assert!(resp.body_utf8().contains("named_value"));
    }

    #[tokio::test]
    async fn echo_named_missing_required_returns_fault() {
        let sut = build_controlled_sut();
        // EchoNamed without the required Value element
        let body = br#"<env:Envelope xmlns:env="http://www.w3.org/2003/05/soap-envelope"><env:Body><c:EchoNamed xmlns:c="http://crossref.example/controlled"/></env:Body></env:Envelope>"#;
        let resp = sut
            .replay("/soap", body, "application/soap+xml; charset=utf-8")
            .await;
        assert_eq!(resp.status, 500);
        assert!(resp.body_utf8().contains("Fault"));
        assert!(resp.body_utf8().contains("Value"));
    }
}
