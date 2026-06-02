//! Layer-2 controlled soap-server: serves the controlled Echo/EchoNamed WSDL with the
//! shared deterministic handlers (crossref::handlers). Listens on 0.0.0.0:8080 inside
//! the compose network.
//!
//! Three mounts on a single listener:
//!   /soap          — unauthenticated (existing)
//!   /soapsec       — WS-Security, lenient timestamp (~100yr tolerance)
//!   /soapsec-strict — WS-Security, strict 300s tolerance (rejects stale)

use crossref::handlers::{echo_handler, echo_named_handler};
use soap_server::ServerBuilder;

const CONTROLLED_WSDL: &[u8] = include_bytes!("../../fixtures/controlled.wsdl");

#[tokio::main]
async fn main() {
    // Unauthed echo at /soap.
    let svc_unauth = ServerBuilder::from_wsdl_bytes(CONTROLLED_WSDL.to_vec())
        .path("/soap")
        .handler("Echo", echo_handler())
        .handler("EchoNamed", echo_named_handler())
        .build()
        .expect("controlled service must build");

    // Lenient authed at /soapsec: ~100 years tolerance so fixed Created=2020 is accepted.
    let svc_sec = ServerBuilder::from_wsdl_bytes(CONTROLLED_WSDL.to_vec())
        .path("/soapsec")
        .handler("Echo", echo_handler())
        .handler("EchoNamed", echo_named_handler())
        .auth(|user| (user == "alice").then(|| "secret".to_string()))
        .timestamp_tolerance_secs(3_153_600_000)
        .build()
        .expect("authed-lenient service must build");

    // Strict authed at /soapsec-strict: 300s tolerance so Created=2000 is rejected.
    let svc_sec_strict = ServerBuilder::from_wsdl_bytes(CONTROLLED_WSDL.to_vec())
        .path("/soapsec-strict")
        .handler("Echo", echo_handler())
        .handler("EchoNamed", echo_named_handler())
        .auth(|user| (user == "alice").then(|| "secret".to_string()))
        .timestamp_tolerance_secs(300)
        .build()
        .expect("authed-strict service must build");

    // Merge all three routers into a single app.
    let app = svc_unauth
        .into_router()
        .merge(svc_sec.into_router())
        .merge(svc_sec_strict.into_router());

    let listener = tokio::net::TcpListener::bind("0.0.0.0:8080")
        .await
        .expect("bind 0.0.0.0:8080");
    eprintln!("controlled-server listening on 0.0.0.0:8080");
    eprintln!("  /soap           — unauthenticated");
    eprintln!("  /soapsec        — WS-Security lenient (100yr tolerance)");
    eprintln!("  /soapsec-strict — WS-Security strict (300s tolerance)");
    axum::serve(listener, app.into_make_service())
        .await
        .expect("serve");
}
