//! Layer-2 controlled soap-server: serves the controlled Echo/EchoNamed WSDL with the
//! shared deterministic handlers (crossref::handlers). Listens on 0.0.0.0:8080 inside
//! the compose network.
//!
//! Four mounts on a single listener:
//!   /soap           — unauthenticated (controlled WSDL)
//!   /soapsec        — WS-Security, lenient timestamp (~100yr tolerance)
//!   /soapsec-strict — WS-Security, strict 300s tolerance (rejects stale)
//!   /soap/a + /soap/b — multi-service WSDL (ServiceA + ServiceB from multi_service.wsdl)

use bytes::Bytes;
use crossref::handlers::{echo_handler, echo_named_handler, faulty_handler};
use soap_server::{FnHandler, ServerBuilder};

const CONTROLLED_WSDL: &[u8] = include_bytes!("../../fixtures/controlled.wsdl");
const MULTI_SERVICE_WSDL: &[u8] = include_bytes!("../../fixtures/multi_service.wsdl");

#[tokio::main]
async fn main() {
    // Unauthed echo at /soap.
    let svc_unauth = ServerBuilder::from_wsdl_bytes(CONTROLLED_WSDL.to_vec())
        .path("/soap")
        .handler("Echo", echo_handler())
        .handler("EchoNamed", echo_named_handler())
        .handler("Faulty", faulty_handler())
        .build()
        .expect("controlled service must build");

    // Lenient authed at /soapsec: ~100 years tolerance so fixed Created=2020 is accepted.
    let svc_sec = ServerBuilder::from_wsdl_bytes(CONTROLLED_WSDL.to_vec())
        .path("/soapsec")
        .handler("Echo", echo_handler())
        .handler("EchoNamed", echo_named_handler())
        .handler("Faulty", faulty_handler())
        .auth(|user| (user == "alice").then(|| "secret".to_string()))
        .timestamp_tolerance_secs(3_153_600_000)
        .build()
        .expect("authed-lenient service must build");

    // Strict authed at /soapsec-strict: 300s tolerance so Created=2000 is rejected.
    let svc_sec_strict = ServerBuilder::from_wsdl_bytes(CONTROLLED_WSDL.to_vec())
        .path("/soapsec-strict")
        .handler("Echo", echo_handler())
        .handler("EchoNamed", echo_named_handler())
        .handler("Faulty", faulty_handler())
        .auth(|user| (user == "alice").then(|| "secret".to_string()))
        .timestamp_tolerance_secs(300)
        .build()
        .expect("authed-strict service must build");

    // Multi-service WSDL: ServiceA at /soap/a, ServiceB at /soap/b.
    // build_multi_service_sut mirrors this: one ServerBuilder, two handlers, router auto-mounts
    // at /soap/a and /soap/b derived from the soap:address location in the WSDL.
    let svc_multi = ServerBuilder::from_wsdl_bytes(MULTI_SERVICE_WSDL.to_vec())
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
        .expect("multi-service WSDL must build");

    // Merge all four routers into a single app.
    let app = svc_unauth
        .into_router()
        .merge(svc_sec.into_router())
        .merge(svc_sec_strict.into_router())
        .merge(svc_multi.into_router());

    let listener = tokio::net::TcpListener::bind("0.0.0.0:8080")
        .await
        .expect("bind 0.0.0.0:8080");
    eprintln!("controlled-server listening on 0.0.0.0:8080");
    eprintln!("  /soap           — unauthenticated");
    eprintln!("  /soapsec        — WS-Security lenient (100yr tolerance)");
    eprintln!("  /soapsec-strict — WS-Security strict (300s tolerance)");
    eprintln!("  /soap/a         — multi-service ServiceA");
    eprintln!("  /soap/b         — multi-service ServiceB");
    axum::serve(listener, app.into_make_service())
        .await
        .expect("serve");
}
