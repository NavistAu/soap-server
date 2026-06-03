// examples/simple_service.rs
//
// A complete, runnable SOAP service from a WSDL fixture.
//
// The WSDL (examples/hello.wsdl) declares one document/literal operation,
// SayHello, taking a `Name` string and returning a `Greeting` string. This
// example loads it, registers a handler for SayHello, and serves it.
//
// Usage:
//   cargo run --example simple_service
//
// Then, from another terminal (SOAP 1.2):
//   curl -s http://localhost:8080/hello \
//     -H 'Content-Type: application/soap+xml' \
//     -d '<s:Envelope xmlns:s="http://www.w3.org/2003/05/soap-envelope">
//           <s:Body>
//             <SayHello xmlns="urn:example:hello"><Name>Ada</Name></SayHello>
//           </s:Body>
//         </s:Envelope>'
//
//   => ...<SayHelloResponse xmlns="urn:example:hello"><Greeting>Hello, Ada!</Greeting>...
//
// Fetch the WSDL itself with a GET:
//   curl -s 'http://localhost:8080/hello?wsdl'

use bytes::Bytes;
use quick_xml::events::Event;
use quick_xml::NsReader;
use soap_server::{escape_text, FnHandler, ServerBuilder, SoapFault};

/// Extract the text content of the first element with the given local name.
/// `read_text` returns the raw inner text (`Ada &amp; Co`); `unescape` turns the
/// XML entities back into characters (`Ada & Co`).
fn text_of(body: &[u8], local: &str) -> Option<String> {
    let mut reader = NsReader::from_reader(body);
    reader.config_mut().trim_text(true);
    loop {
        match reader.read_event() {
            Ok(Event::Start(e)) if e.local_name().as_ref() == local.as_bytes() => {
                let end = e.to_end().into_owned();
                let raw = reader.read_text(end.name()).ok()?;
                return quick_xml::escape::unescape(&raw)
                    .ok()
                    .map(|c| c.into_owned());
            }
            Ok(Event::Eof) => return None,
            Err(_) => return None,
            _ => {}
        }
    }
}

#[tokio::main]
async fn main() {
    let wsdl = concat!(env!("CARGO_MANIFEST_DIR"), "/examples/hello.wsdl");

    let svc = ServerBuilder::from_wsdl_file(wsdl)
        // The operation name MUST match a <wsdl:operation> in the WSDL, or
        // .build() returns Err — misnamed handlers fail at startup, not at runtime.
        .handler(
            "SayHello",
            FnHandler::new(|body: Bytes| async move {
                // `body` is the <SayHello> element. Pull out <Name>...
                let name = text_of(&body, "Name").unwrap_or_else(|| "world".to_string());
                // ...and build the <SayHelloResponse>. You own the response XML;
                // escape any untrusted text you interpolate into it.
                let xml = format!(
                    r#"<SayHelloResponse xmlns="urn:example:hello"><Greeting>Hello, {}!</Greeting></SayHelloResponse>"#,
                    escape_text(&name)
                );
                Ok::<Bytes, SoapFault>(Bytes::from(xml))
            }),
        )
        .path("/hello")
        .build()
        .expect("WSDL build failed");

    let app = svc.into_router();
    let listener = tokio::net::TcpListener::bind("0.0.0.0:8080")
        .await
        .expect("bind failed");

    println!("simple_service on http://0.0.0.0:8080/hello  (GET ?wsdl for the contract)");
    axum::serve(listener, app).await.expect("server error");
}
