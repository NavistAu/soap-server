//! # soap-server
//!
//! A WSDL-driven SOAP 1.1/1.2 server library for Rust, built on top of
//! [axum](https://docs.rs/axum). Provide a WSDL file, register async handler
//! closures for each operation, and get a fully spec-compliant SOAP endpoint.
//!
//! ## Features
//!
//! - **SOAP 1.1 and 1.2** — auto-detects version from `Content-Type` and envelope
//!   namespace; responds in the same version as the request.
//! - **WSDL-driven dispatch** — operations are discovered from the WSDL at build time.
//!   Registering a handler for an unknown operation is a build-time error.
//! - **WS-Security (UsernameToken)** — supports PasswordDigest and PasswordText
//!   authentication with nonce replay detection and timestamp freshness checks.
//! - **XSD structural validation** — required elements are validated against the
//!   WSDL/XSD schema before the handler is called.
//!
//! ## Quick start
//!
//! ```rust,no_run
//! use soap_server::{FnHandler, ServerBuilder, SoapFault};
//! use bytes::Bytes;
//!
//! #[tokio::main]
//! async fn main() {
//!     let svc = ServerBuilder::from_wsdl_file("path/to/service.wsdl")
//!         .handler(
//!             "MyOperation",
//!             FnHandler::new(|_body: Bytes| async move {
//!                 // Parse body, call business logic, return response XML bytes.
//!                 Ok::<Bytes, SoapFault>(Bytes::from(
//!                     r#"<MyOperationResponse xmlns="urn:example"/>"#,
//!                 ))
//!             }),
//!         )
//!         .build()
//!         .expect("WSDL build failed");
//!
//!     let router = svc.into_router();
//!     let listener = tokio::net::TcpListener::bind("0.0.0.0:8080").await.unwrap();
//!     axum::serve(listener, router).await.unwrap();
//! }
//! ```
//!
//! ## WS-Security
//!
//! Call `.auth(|username| { /* return password or None */ })` on the builder.
//! Operations that require authentication return a `Sender` fault if the
//! `<wsse:Security>` header is missing or invalid. Use `.auth_bypass([...])` to
//! exempt specific operations (e.g. clock-sync operations).
//!
//! ## Multi-WSDL / multi-service
//!
//! Call `SoapService::into_router()` on each service and merge the resulting
//! `axum::Router` instances — each service mounts at its own path derived from
//! the WSDL `<service><port address>` element.

pub mod dispatch;
pub(crate) mod envelope;
pub(crate) mod qname;
pub(crate) mod server;
pub(crate) mod wsdl;
pub(crate) mod wssec;
pub(crate) mod xsd;

pub mod fault;
pub mod handler;
pub mod xml_escape;

pub use crate::dispatch::{build_dispatch_table, DispatchTable};
pub use crate::fault::{FaultCode, SoapFault};
pub use crate::handler::{FnHandler, SoapHandler};
pub use crate::server::{BuildError, FileWsdlLoader, ServerBuilder, SoapService};
pub use crate::wsdl::parser::WsdlError;
pub use crate::wsdl::resolver::WsdlLoader;
pub use crate::wssec::nonce_cache::RotatingNonceCache;
pub use crate::wssec::username_token::compute_digest;
pub use crate::wssec::username_token::validate_username_token;
pub use crate::xml_escape::{escape_attr, escape_text};
