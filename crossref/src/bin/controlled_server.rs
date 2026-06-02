//! Layer-2 controlled soap-server: serves the controlled Echo/EchoNamed WSDL with the
//! shared deterministic handlers (crossref::handlers). Listens on 0.0.0.0:8080 inside
//! the compose network.

use crossref::handlers::{echo_handler, echo_named_handler};
use soap_server::ServerBuilder;

const CONTROLLED_WSDL: &[u8] = include_bytes!("../../fixtures/controlled.wsdl");

#[tokio::main]
async fn main() {
    let svc = ServerBuilder::from_wsdl_bytes(CONTROLLED_WSDL.to_vec())
        .path("/soap")
        .handler("Echo", echo_handler())
        .handler("EchoNamed", echo_named_handler())
        .build()
        .expect("controlled service must build");
    let router = svc.into_router();
    let listener = tokio::net::TcpListener::bind("0.0.0.0:8080")
        .await
        .expect("bind 0.0.0.0:8080");
    eprintln!("controlled-server listening on 0.0.0.0:8080/soap");
    axum::serve(listener, router.into_make_service())
        .await
        .expect("serve");
}
