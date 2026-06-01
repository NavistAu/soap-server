//! Builds the soap-server System-Under-Test from the controlled fixture (spec §5.8)
//! and replays requests against it in-process via axum_test.

use axum_test::TestServer;
use bytes::Bytes;
use soap_server::{FnHandler, ServerBuilder};

pub const CONTROLLED_WSDL: &[u8] = include_bytes!("../fixtures/controlled.wsdl");

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
}

/// Extract the text content of the first element whose local name ends with "Text".
fn extract_text(body: &[u8]) -> String {
    use quick_xml::events::Event;
    use quick_xml::Reader;

    let mut reader = Reader::from_reader(body);
    reader.config_mut().trim_text(true);
    let mut in_text = false;
    let mut result = String::new();
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
                result = t.decode().unwrap_or_default().into_owned();
                break;
            }
            Ok(Event::End(_)) if in_text => break,
            Ok(Event::Eof) => break,
            Err(_) => break,
            _ => {}
        }
    }
    result
}

fn echo_handler() -> impl soap_server::SoapHandler {
    FnHandler::new(|body: Bytes| async move {
        let text = extract_text(&body);
        let escaped = soap_server::escape_text(&text);
        let resp = format!(
            r#"<c:EchoResponse xmlns:c="http://crossref.example/controlled"><c:Text>{escaped}</c:Text></c:EchoResponse>"#
        );
        Ok::<Bytes, soap_server::SoapFault>(Bytes::from(resp))
    })
}

pub fn build_controlled_sut() -> Sut {
    let svc = ServerBuilder::from_wsdl_bytes(CONTROLLED_WSDL.to_vec())
        .path("/soap")
        .handler("Echo", echo_handler())
        .build()
        .expect("controlled WSDL should build without error");
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
}
