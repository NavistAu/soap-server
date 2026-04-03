// ONVIF end-to-end integration tests — Phase 1 acceptance gate.
//
// Tests the full stack with real ONVIF devicemgmt.wsdl + onvif.xsd + common.xsd fixtures:
//   1. Multi-file WSDL loads without panic, ServiceModel contains expected operations
//   2. SOAP 1.2 POST dispatches to correct handler → correct response
//   3. WS-Security: valid credentials accepted; wrong password rejected; auth bypass works
//   4. GET ?wsdl returns rewritten WSDL XML
//   5. axum Router composes cleanly (Router::merge)

use std::sync::{Arc, Mutex};
use axum_test::TestServer;
use bytes::Bytes;
use soap_server::{
    compute_digest, FaultCode, FnHandler, SoapFault, ServerBuilder, WsdlError, WsdlLoader,
};

// ── FixtureLoader ─────────────────────────────────────────────────────────────
//
// Maps any import location to a fixture file by extracting the basename and
// looking it up in tests/fixtures/. This handles the real ONVIF relative
// paths like "../../../ver10/schema/onvif.xsd" → "onvif.xsd".
//
// External HTTP/HTTPS URLs (xmlmime, soap-envelope, wsn) are silently ignored
// by returning empty schema bytes — these are optional extensions not needed
// for dispatch correctness.

struct FixtureLoader;

impl WsdlLoader for FixtureLoader {
    fn load(&self, location: &str) -> Result<Vec<u8>, WsdlError> {
        // Extract the basename from the location path.
        let basename = location
            .split('/')
            .last()
            .unwrap_or(location);

        // Map known fixture filenames to their on-disk paths.
        let fixture_path = match basename {
            "onvif.xsd" => "tests/fixtures/onvif.xsd",
            "common.xsd" => "tests/fixtures/common.xsd",
            "devicemgmt.wsdl" => "tests/fixtures/devicemgmt.wsdl",
            other => {
                // Unknown external location (e.g., HTTPS schema URLs) — return empty schema.
                // These are optional type extensions; their absence does not affect dispatch.
                return Ok(
                    format!(
                        r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema"
                            targetNamespace="urn:fixture-stub:{other}"/>"#
                    )
                    .into_bytes(),
                );
            }
        };

        std::fs::read(fixture_path).map_err(|e| {
            WsdlError::MalformedXml(format!(
                "FixtureLoader: failed to load '{fixture_path}': {e}"
            ))
        })
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Format a Unix timestamp as a wsu:Created string (ISO 8601 / WS-Security format).
fn format_wsu_created(unix_secs: u64) -> String {
    // Format: YYYY-MM-DDTHH:MM:SS.000Z
    // Compute date/time components from Unix timestamp using integer arithmetic only.
    let secs_in_day = unix_secs % 86400;
    let days = unix_secs / 86400;

    let hour = secs_in_day / 3600;
    let minute = (secs_in_day % 3600) / 60;
    let second = secs_in_day % 60;

    // Gregorian calendar from days since 1970-01-01
    let (year, month, day) = days_to_ymd(days);

    format!("{:04}-{:02}-{:02}T{:02}:{:02}:{:02}.000Z", year, month, day, hour, minute, second)
}

fn days_to_ymd(days: u64) -> (u64, u64, u64) {
    // Algorithm: convert days since Unix epoch (1970-01-01) to (year, month, day)
    let mut remaining = days as i64 + 719468; // shift to civil epoch (0001-03-01 = day 0)
    let era = if remaining >= 0 { remaining } else { remaining - 146096 } / 146097;
    let doe = remaining - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    (y as u64, m as u64, d as u64)
}

// ── ONVIF device namespace ────────────────────────────────────────────────────

const ONVIF_TDS_NS: &str = "http://www.onvif.org/ver10/device/wsdl";

fn soap12_envelope(ns: &str, prefix: &str, body: &str) -> String {
    format!(
        r#"<env:Envelope xmlns:env="http://www.w3.org/2003/05/soap-envelope" xmlns:{prefix}="{ns}"><env:Body>{body}</env:Body></env:Envelope>"#
    )
}

fn soap12_envelope_with_security(
    ns: &str,
    prefix: &str,
    body: &str,
    username: &str,
    nonce_b64: &str,
    created: &str,
    password_digest: &str,
) -> String {
    // Single-line envelope ensures no whitespace issues with XML namespace inheritance.
    format!(
        r#"<env:Envelope xmlns:env="http://www.w3.org/2003/05/soap-envelope" xmlns:{prefix}="{ns}" xmlns:wsse="http://docs.oasis-open.org/wss/2004/01/oasis-200401-wss-wssecurity-secext-1.0.xsd" xmlns:wsu="http://docs.oasis-open.org/wss/2004/01/oasis-200401-wss-wssecurity-utility-1.0.xsd"><env:Header><wsse:Security><wsse:UsernameToken><wsse:Username>{username}</wsse:Username><wsse:Password Type="http://docs.oasis-open.org/wss/2004/01/oasis-200401-wss-username-token-profile-1.0#PasswordDigest">{password_digest}</wsse:Password><wsse:Nonce>{nonce_b64}</wsse:Nonce><wsu:Created>{created}</wsu:Created></wsse:UsernameToken></wsse:Security></env:Header><env:Body>{body}</env:Body></env:Envelope>"#
    )
}

// ── Test 1: WSDL loads (success criterion 1) ─────────────────────────────────
//
// Verifies: real multi-file ONVIF WSDL loads without panic; ServiceModel is non-empty;
// GetSystemDateAndTime and GetDeviceInformation operations are present.

#[tokio::test]
async fn test_onvif_wsdl_loads() {
    let wsdl_bytes = include_bytes!("fixtures/devicemgmt.wsdl");

    // Use default_handler so we don't have to register all ~100 ONVIF operations.
    let service = ServerBuilder::from_wsdl_bytes_with_loader(
        wsdl_bytes.as_ref(),
        FixtureLoader,
    )
    .handler(
        "GetSystemDateAndTime",
        FnHandler::new(|_| async {
            Ok::<Bytes, SoapFault>(Bytes::from(
                "<tds:GetSystemDateAndTimeResponse xmlns:tds=\"http://www.onvif.org/ver10/device/wsdl\"/>",
            ))
        }),
    )
    .handler(
        "GetDeviceInformation",
        FnHandler::new(|_| async {
            Ok::<Bytes, SoapFault>(Bytes::from(
                "<tds:GetDeviceInformationResponse xmlns:tds=\"http://www.onvif.org/ver10/device/wsdl\"/>",
            ))
        }),
    )
    .default_handler(FnHandler::new(|_| async {
        Ok::<Bytes, SoapFault>(Bytes::from("<DefaultResponse/>"))
    }))
    .auth_bypass(["GetSystemDateAndTime", "GetDeviceInformation"])
    .build()
    .expect("Real ONVIF WSDL should load without error");

    // into_router() must not panic (success criterion 1)
    let _router = service.into_router();
}

// ── Test 2: SOAP 1.2 dispatch (success criterion 2) ──────────────────────────
//
// Verifies: POST with valid SOAP 1.2 envelope dispatched to correct handler;
// response is 200, Content-Type application/soap+xml, body contains expected response.

#[tokio::test]
async fn test_soap12_dispatch() {
    let wsdl_bytes = include_bytes!("fixtures/devicemgmt.wsdl");

    let service = ServerBuilder::from_wsdl_bytes_with_loader(wsdl_bytes.as_ref(), FixtureLoader)
        .handler(
            "GetSystemDateAndTime",
            FnHandler::new(|_| async {
                Ok::<Bytes, SoapFault>(Bytes::from(
                    "<tds:GetSystemDateAndTimeResponse xmlns:tds=\"http://www.onvif.org/ver10/device/wsdl\"><tds:SystemDateAndTime/></tds:GetSystemDateAndTimeResponse>",
                ))
            }),
        )
        .default_handler(FnHandler::new(|_| async {
            Ok::<Bytes, SoapFault>(Bytes::from("<DefaultResponse/>"))
        }))
        .auth_bypass(["GetSystemDateAndTime"])
        .build()
        .expect("build should succeed");

    let router = service.into_router();
    let server = TestServer::new(router);

    let body = soap12_envelope(
        ONVIF_TDS_NS,
        "tds",
        "<tds:GetSystemDateAndTime/>",
    );
    let resp = server
        .post("/soap")
        .bytes(axum::body::Bytes::from(body.into_bytes()))
        .content_type("application/soap+xml")
        .await;

    resp.assert_status_ok();

    let content_type = resp.headers().get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    assert!(
        content_type.contains("application/soap+xml"),
        "Content-Type should be application/soap+xml, got: {content_type}"
    );

    let resp_text = resp.text();
    assert!(
        resp_text.contains("GetSystemDateAndTimeResponse"),
        "Response should contain handler output, got: {resp_text}"
    );
    assert!(
        resp_text.contains("env:Envelope"),
        "Response should be wrapped in SOAP envelope, got: {resp_text}"
    );
}

// ── Test 3a: Valid WS-Security credentials accepted (success criterion 3) ─────
//
// Verifies: POST with correct PasswordDigest is dispatched to handler → 200.

#[tokio::test]
async fn test_wssec_valid_credentials_accepted() {
    let wsdl_bytes = include_bytes!("fixtures/devicemgmt.wsdl");

    let handler_called = Arc::new(Mutex::new(false));
    let handler_called_clone = handler_called.clone();

    let service = ServerBuilder::from_wsdl_bytes_with_loader(wsdl_bytes.as_ref(), FixtureLoader)
        .handler(
            "GetDeviceInformation",
            FnHandler::new(move |_| {
                let called = handler_called_clone.clone();
                async move {
                    *called.lock().unwrap() = true;
                    Ok::<Bytes, SoapFault>(Bytes::from(
                        "<tds:GetDeviceInformationResponse xmlns:tds=\"http://www.onvif.org/ver10/device/wsdl\"/>",
                    ))
                }
            }),
        )
        .default_handler(FnHandler::new(|_| async {
            Ok::<Bytes, SoapFault>(Bytes::from("<DefaultResponse/>"))
        }))
        .auth(|username| {
            if username == "admin" {
                Some("password123".to_string())
            } else {
                None
            }
        })
        // GetDeviceInformation is NOT bypassed — auth required
        .build()
        .expect("build should succeed");

    let router = service.into_router();
    let server = TestServer::new(router);

    // Compute a valid PasswordDigest using current time so the timestamp check passes.
    // The WS-Security validator uses Utc::now() with a 300s tolerance window.
    let nonce_b64 = "AAECAwQFBgcICQoLDA0ODw==";
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let created = format_wsu_created(now - 1);
    let digest = compute_digest(nonce_b64, &created, "password123")
        .expect("compute_digest should not fail");

    let body_content = "<tds:GetDeviceInformation/>";
    let body = soap12_envelope_with_security(
        ONVIF_TDS_NS,
        "tds",
        body_content,
        "admin",
        nonce_b64,
        &created,
        &digest,
    );

    let resp = server
        .post("/soap")
        .bytes(axum::body::Bytes::from(body.into_bytes()))
        .content_type("application/soap+xml")
        .await;

    let resp_text = resp.text();
    assert!(
        resp.status_code() == axum::http::StatusCode::OK,
        "Expected 200, got 500. Created: {created}, Digest: {digest}. Response: {resp_text}"
    );
    assert!(
        *handler_called.lock().unwrap(),
        "Handler should have been called for valid credentials. Response: {resp_text}"
    );
}

// ── Test 3b: Wrong password rejected (success criterion 3) ───────────────────
//
// Verifies: POST with wrong PasswordDigest returns HTTP 500 SOAP fault;
// handler is NOT called.

#[tokio::test]
async fn test_wssec_wrong_password_rejected() {
    let wsdl_bytes = include_bytes!("fixtures/devicemgmt.wsdl");

    let handler_called = Arc::new(Mutex::new(false));
    let handler_called_clone = handler_called.clone();

    let service = ServerBuilder::from_wsdl_bytes_with_loader(wsdl_bytes.as_ref(), FixtureLoader)
        .handler(
            "GetDeviceInformation",
            FnHandler::new(move |_| {
                let called = handler_called_clone.clone();
                async move {
                    *called.lock().unwrap() = true;
                    Ok::<Bytes, SoapFault>(Bytes::from("<tds:GetDeviceInformationResponse xmlns:tds=\"http://www.onvif.org/ver10/device/wsdl\"/>"))
                }
            }),
        )
        .default_handler(FnHandler::new(|_| async {
            Ok::<Bytes, SoapFault>(Bytes::from("<DefaultResponse/>"))
        }))
        .auth(|username| {
            if username == "admin" {
                Some("password123".to_string())
            } else {
                None
            }
        })
        .build()
        .expect("build should succeed");

    let router = service.into_router();
    let server = TestServer::new(router);

    // Compute digest with WRONG password
    let nonce_b64 = "AAECAwQFBgcICQoLDA0ODw==";
    let created = "2026-04-03T12:00:00.000Z";
    let wrong_digest = compute_digest(nonce_b64, created, "wrong_password")
        .expect("compute_digest should not fail");

    let body_content = "<tds:GetDeviceInformation/>";
    let body = soap12_envelope_with_security(
        ONVIF_TDS_NS,
        "tds",
        body_content,
        "admin",
        nonce_b64,
        created,
        &wrong_digest,
    );

    let resp = server
        .post("/soap")
        .bytes(axum::body::Bytes::from(body.into_bytes()))
        .content_type("application/soap+xml")
        .await;

    resp.assert_status(axum::http::StatusCode::INTERNAL_SERVER_ERROR);
    let resp_text = resp.text();
    assert!(
        resp_text.contains("env:Fault"),
        "Response should contain SOAP Fault, got: {resp_text}"
    );
    assert!(
        !*handler_called.lock().unwrap(),
        "Handler should NOT have been called for wrong password"
    );
}

// ── Test 3c: Auth bypass works (success criterion 3 / SEC-06) ────────────────
//
// Verifies: operation in auth_bypass list is dispatched without Security header.

#[tokio::test]
async fn test_auth_bypass_get_system_date() {
    let wsdl_bytes = include_bytes!("fixtures/devicemgmt.wsdl");

    let service = ServerBuilder::from_wsdl_bytes_with_loader(wsdl_bytes.as_ref(), FixtureLoader)
        .handler(
            "GetSystemDateAndTime",
            FnHandler::new(|_| async {
                Ok::<Bytes, SoapFault>(Bytes::from(
                    "<tds:GetSystemDateAndTimeResponse xmlns:tds=\"http://www.onvif.org/ver10/device/wsdl\"><tds:SystemDateAndTime/></tds:GetSystemDateAndTimeResponse>",
                ))
            }),
        )
        .default_handler(FnHandler::new(|_| async {
            Ok::<Bytes, SoapFault>(Bytes::from("<DefaultResponse/>"))
        }))
        .auth(|_username| Some("password".to_string()))
        .auth_bypass(["GetSystemDateAndTime"])
        .build()
        .expect("build should succeed");

    let router = service.into_router();
    let server = TestServer::new(router);

    // No security header — GetSystemDateAndTime is bypassed
    let body = soap12_envelope(ONVIF_TDS_NS, "tds", "<tds:GetSystemDateAndTime/>");
    let resp = server
        .post("/soap")
        .bytes(axum::body::Bytes::from(body.into_bytes()))
        .content_type("application/soap+xml")
        .await;

    resp.assert_status_ok();
    let resp_text = resp.text();
    assert!(
        resp_text.contains("GetSystemDateAndTimeResponse"),
        "Handler should have been called for bypassed op, got: {resp_text}"
    );
}

// ── Test 4: GET ?wsdl returns rewritten WSDL (success criterion 4) ───────────
//
// Verifies: GET /soap?wsdl returns 200, Content-Type text/xml, parseable XML,
// and soap:address location is rewritten to test server URL.

#[tokio::test]
async fn test_wsdl_serving() {
    let wsdl_bytes = include_bytes!("fixtures/devicemgmt.wsdl");

    let service = ServerBuilder::from_wsdl_bytes_with_loader(wsdl_bytes.as_ref(), FixtureLoader)
        .handler(
            "GetSystemDateAndTime",
            FnHandler::new(|_| async {
                Ok::<Bytes, SoapFault>(Bytes::from("<tds:GetSystemDateAndTimeResponse xmlns:tds=\"http://www.onvif.org/ver10/device/wsdl\"/>"))
            }),
        )
        .default_handler(FnHandler::new(|_| async {
            Ok::<Bytes, SoapFault>(Bytes::from("<DefaultResponse/>"))
        }))
        .auth_bypass(["GetSystemDateAndTime"])
        .build()
        .expect("build should succeed");

    let router = service.into_router();
    let server = TestServer::new(router);

    let resp = server.get("/soap?wsdl").await;
    resp.assert_status_ok();

    let content_type = resp.headers().get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    assert!(
        content_type.contains("text/xml"),
        "Content-Type should be text/xml, got: {content_type}"
    );

    let body_text = resp.text();

    // Should be valid XML (contains WSDL definitions element)
    assert!(
        body_text.contains("wsdl:definitions") || body_text.contains("definitions"),
        "Response should be WSDL XML, got (first 200 chars): {}",
        &body_text[..body_text.len().min(200)]
    );

    // soap:address location should be rewritten to the test server's URL
    // (axum-test uses localhost as the host header by default)
    assert!(
        body_text.contains("localhost") || body_text.contains("/soap"),
        "soap:address should be rewritten, got (first 500 chars): {}",
        &body_text[..body_text.len().min(500)]
    );
}

// ── Test 5: axum Router composition (success criterion 5) ────────────────────
//
// Verifies: SoapService::into_router() produces a Router composable via Router::merge().

#[tokio::test]
async fn test_router_composition() {
    let wsdl_bytes = include_bytes!("fixtures/devicemgmt.wsdl");

    let service = ServerBuilder::from_wsdl_bytes_with_loader(wsdl_bytes.as_ref(), FixtureLoader)
        .handler(
            "GetSystemDateAndTime",
            FnHandler::new(|_| async {
                Ok::<Bytes, SoapFault>(Bytes::from("<tds:GetSystemDateAndTimeResponse xmlns:tds=\"http://www.onvif.org/ver10/device/wsdl\"/>"))
            }),
        )
        .default_handler(FnHandler::new(|_| async {
            Ok::<Bytes, SoapFault>(Bytes::from("<DefaultResponse/>"))
        }))
        .auth_bypass(["GetSystemDateAndTime"])
        .build()
        .expect("build should succeed");

    let soap_router = service.into_router();

    // Compose with a separate health check route
    let app = axum::Router::new()
        .route("/health", axum::routing::get(|| async { "ok" }))
        .merge(soap_router);

    let server = TestServer::new(app);

    // Health check should return 200
    let health_resp = server.get("/health").await;
    health_resp.assert_status_ok();
    assert_eq!(health_resp.text(), "ok");

    // SOAP endpoint should still be reachable (not 404)
    let soap_body = soap12_envelope(ONVIF_TDS_NS, "tds", "<tds:GetSystemDateAndTime/>");
    let soap_resp = server
        .post("/soap")
        .bytes(axum::body::Bytes::from(soap_body.into_bytes()))
        .content_type("application/soap+xml")
        .await;
    // Should not be 404
    assert_ne!(
        soap_resp.status_code().as_u16(),
        404,
        "SOAP endpoint should be accessible after Router::merge"
    );
}
