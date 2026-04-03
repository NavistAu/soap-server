// Integration test for the ServerBuilder + SoapService pipeline.
// Tests the full request pipeline: envelope parsing → dispatch → handler → response.

use axum_test::TestServer;
use bytes::Bytes;
use soap_server::{FaultCode, FnHandler, ServerBuilder, SoapFault};

// Minimal WSDL bytes with one operation (GetSystemDateAndTime) for testing.
// Uses a simple inline schema so resolve_wsdl completes without external imports.
const TEST_WSDL: &[u8] = br#"<?xml version="1.0" encoding="utf-8"?>
<wsdl:definitions
    xmlns:wsdl="http://schemas.xmlsoap.org/wsdl/"
    xmlns:soap="http://schemas.xmlsoap.org/wsdl/soap12/"
    xmlns:xs="http://www.w3.org/2001/XMLSchema"
    xmlns:tds="http://example.com/test"
    targetNamespace="http://example.com/test">

    <wsdl:types>
        <xs:schema targetNamespace="http://example.com/test"
                   xmlns:tds="http://example.com/test"
                   elementFormDefault="qualified">
            <xs:element name="GetSystemDateAndTime">
                <xs:complexType>
                    <xs:sequence/>
                </xs:complexType>
            </xs:element>
            <xs:element name="GetSystemDateAndTimeResponse">
                <xs:complexType>
                    <xs:sequence>
                        <xs:element name="SystemDateAndTime" type="xs:string"/>
                    </xs:sequence>
                </xs:complexType>
            </xs:element>
            <xs:element name="GetProfiles">
                <xs:complexType>
                    <xs:sequence/>
                </xs:complexType>
            </xs:element>
            <xs:element name="GetProfilesResponse">
                <xs:complexType>
                    <xs:sequence>
                        <xs:element name="Profiles" type="xs:string"/>
                    </xs:sequence>
                </xs:complexType>
            </xs:element>
        </xs:schema>
    </wsdl:types>

    <wsdl:message name="GetSystemDateAndTimeRequest">
        <wsdl:part name="parameters" element="tds:GetSystemDateAndTime"/>
    </wsdl:message>
    <wsdl:message name="GetSystemDateAndTimeResponse">
        <wsdl:part name="parameters" element="tds:GetSystemDateAndTimeResponse"/>
    </wsdl:message>
    <wsdl:message name="GetProfilesRequest">
        <wsdl:part name="parameters" element="tds:GetProfiles"/>
    </wsdl:message>
    <wsdl:message name="GetProfilesResponse">
        <wsdl:part name="parameters" element="tds:GetProfilesResponse"/>
    </wsdl:message>

    <wsdl:portType name="TestPortType">
        <wsdl:operation name="GetSystemDateAndTime">
            <wsdl:input message="tds:GetSystemDateAndTimeRequest"/>
            <wsdl:output message="tds:GetSystemDateAndTimeResponse"/>
        </wsdl:operation>
        <wsdl:operation name="GetProfiles">
            <wsdl:input message="tds:GetProfilesRequest"/>
            <wsdl:output message="tds:GetProfilesResponse"/>
        </wsdl:operation>
    </wsdl:portType>

    <wsdl:binding name="TestBinding" type="tds:TestPortType">
        <soap:binding style="document" transport="http://schemas.xmlsoap.org/soap/http"/>
        <wsdl:operation name="GetSystemDateAndTime">
            <soap:operation soapAction="http://example.com/test/GetSystemDateAndTime"/>
            <wsdl:input><soap:body use="literal"/></wsdl:input>
            <wsdl:output><soap:body use="literal"/></wsdl:output>
        </wsdl:operation>
        <wsdl:operation name="GetProfiles">
            <soap:operation soapAction="http://example.com/test/GetProfiles"/>
            <wsdl:input><soap:body use="literal"/></wsdl:input>
            <wsdl:output><soap:body use="literal"/></wsdl:output>
        </wsdl:operation>
    </wsdl:binding>

    <wsdl:service name="TestService">
        <wsdl:port name="TestPort" binding="tds:TestBinding">
            <soap:address location="http://localhost/soap"/>
        </wsdl:port>
    </wsdl:service>
</wsdl:definitions>"#;

/// Build a SOAP 1.2 envelope with the given body content.
fn make_soap12_envelope(body: &str) -> String {
    format!(
        r#"<env:Envelope xmlns:env="http://www.w3.org/2003/05/soap-envelope" xmlns:tds="http://example.com/test"><env:Body>{body}</env:Body></env:Envelope>"#
    )
}

/// Build a SOAP 1.2 envelope with a WS-Security header containing a plaintext password.
fn make_soap12_envelope_with_auth(body: &str, username: &str, password: &str) -> String {
    format!(
        r#"<env:Envelope xmlns:env="http://www.w3.org/2003/05/soap-envelope" xmlns:tds="http://example.com/test" xmlns:wsse="http://docs.oasis-open.org/wss/2004/01/oasis-200401-wss-wssecurity-secext-1.0.xsd"><env:Header><wsse:Security><wsse:UsernameToken><wsse:Username>{username}</wsse:Username><wsse:Password Type="http://docs.oasis-open.org/wss/2004/01/oasis-200401-wss-username-token-profile-1.0#PasswordText">{password}</wsse:Password></wsse:UsernameToken></wsse:Security></env:Header><env:Body>{body}</env:Body></env:Envelope>"#
    )
}

// ── Test 1: POST with valid SOAP 1.2 → handler called, 200 response ──────────

#[tokio::test]
async fn post_soap12_valid_envelope_dispatches_to_handler() {
    let svc = ServerBuilder::from_wsdl_bytes(TEST_WSDL)
        .handler(
            "GetSystemDateAndTime",
            FnHandler::new(|_body: Bytes| async move {
                Ok::<Bytes, SoapFault>(Bytes::from_static(
                    b"<tds:GetSystemDateAndTimeResponse xmlns:tds=\"http://example.com/test\"><tds:SystemDateAndTime>2024-01-01T00:00:00Z</tds:SystemDateAndTime></tds:GetSystemDateAndTimeResponse>",
                ))
            }),
        )
        .handler(
            "GetProfiles",
            FnHandler::new(|_body: Bytes| async move {
                Ok::<Bytes, SoapFault>(Bytes::from_static(
                    b"<tds:GetProfilesResponse xmlns:tds=\"http://example.com/test\"><tds:Profiles>profile1</tds:Profiles></tds:GetProfilesResponse>",
                ))
            }),
        )
        .auth_bypass(["GetSystemDateAndTime", "GetProfiles"])
        .build()
        .expect("ServerBuilder::build() should succeed");

    let router = svc.into_router();
    let server = TestServer::new(router);

    let body = make_soap12_envelope("<tds:GetSystemDateAndTime/>");
    let resp = server
        .post("/soap")
        .bytes(axum::body::Bytes::from(body.into_bytes()))
        .content_type("application/soap+xml")
        .await;

    resp.assert_status_ok();
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

// ── Test 2: POST with wrong password → SOAP fault, handler NOT called ─────────

#[tokio::test]
async fn post_with_wrong_password_returns_fault_handler_not_called() {
    use std::sync::{Arc, Mutex};

    let handler_called = Arc::new(Mutex::new(false));
    let handler_called_clone = handler_called.clone();

    let svc = ServerBuilder::from_wsdl_bytes(TEST_WSDL)
        .handler(
            "GetSystemDateAndTime",
            FnHandler::new(|_body: Bytes| async move {
                Ok::<Bytes, SoapFault>(Bytes::from_static(b"<resp/>"))
            }),
        )
        .handler(
            "GetProfiles",
            FnHandler::new(move |_body: Bytes| {
                let called = handler_called_clone.clone();
                async move {
                    *called.lock().unwrap() = true;
                    Ok::<Bytes, SoapFault>(Bytes::from_static(b"<resp/>"))
                }
            }),
        )
        .auth(|username: &str| {
            if username == "admin" {
                Some("correct_password".to_string())
            } else {
                None
            }
        })
        .auth_bypass(["GetSystemDateAndTime"])
        .build()
        .expect("build should succeed");

    let router = svc.into_router();
    let server = TestServer::new(router);

    let body = make_soap12_envelope_with_auth(
        "<tds:GetProfiles/>",
        "admin",
        "wrong_password",
    );
    let resp = server
        .post("/soap")
        .bytes(axum::body::Bytes::from(body.into_bytes()))
        .content_type("application/soap+xml")
        .await;

    // Should return 500 with SOAP fault
    resp.assert_status(axum::http::StatusCode::INTERNAL_SERVER_ERROR);
    let resp_text = resp.text();
    assert!(
        resp_text.contains("env:Fault"),
        "Response should be a SOAP Fault, got: {resp_text}"
    );
    assert!(
        !*handler_called.lock().unwrap(),
        "Handler should NOT have been called"
    );
}

// ── Test 3: Auth-bypassed operation, no security header → handler called ──────

#[tokio::test]
async fn auth_bypassed_operation_without_security_calls_handler() {
    let svc = ServerBuilder::from_wsdl_bytes(TEST_WSDL)
        .handler(
            "GetSystemDateAndTime",
            FnHandler::new(|_body: Bytes| async move {
                Ok::<Bytes, SoapFault>(Bytes::from_static(
                    b"<tds:GetSystemDateAndTimeResponse xmlns:tds=\"http://example.com/test\"><tds:SystemDateAndTime>now</tds:SystemDateAndTime></tds:GetSystemDateAndTimeResponse>",
                ))
            }),
        )
        .handler(
            "GetProfiles",
            FnHandler::new(|_body: Bytes| async move {
                Ok::<Bytes, SoapFault>(Bytes::from_static(b"<resp/>"))
            }),
        )
        .auth(|_username: &str| Some("password".to_string()))
        .auth_bypass(["GetSystemDateAndTime"]) // GetProfiles requires auth
        .build()
        .expect("build should succeed");

    let router = svc.into_router();
    let server = TestServer::new(router);

    // No security header — but GetSystemDateAndTime is bypassed
    let body = make_soap12_envelope("<tds:GetSystemDateAndTime/>");
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

// ── Test 4: POST to unregistered operation → SOAP fault FaultCode::Sender ─────

#[tokio::test]
async fn post_unknown_operation_returns_sender_fault() {
    let svc = ServerBuilder::from_wsdl_bytes(TEST_WSDL)
        .handler(
            "GetSystemDateAndTime",
            FnHandler::new(|_body: Bytes| async move {
                Ok::<Bytes, SoapFault>(Bytes::from_static(b"<resp/>"))
            }),
        )
        .handler(
            "GetProfiles",
            FnHandler::new(|_body: Bytes| async move {
                Ok::<Bytes, SoapFault>(Bytes::from_static(b"<resp/>"))
            }),
        )
        .auth_bypass(["GetSystemDateAndTime", "GetProfiles"])
        .build()
        .expect("build should succeed");

    let router = svc.into_router();
    let server = TestServer::new(router);

    // UnknownOperation is not in the dispatch table
    let body = make_soap12_envelope(
        "<tds:UnknownOperation xmlns:tds=\"http://example.com/test\"/>",
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
        "Expected SOAP Fault, got: {resp_text}"
    );
    assert!(
        resp_text.contains("env:Sender"),
        "Expected Sender fault code, got: {resp_text}"
    );
}

// ── Test 5: GET ?wsdl returns WSDL XML ────────────────────────────────────────

#[tokio::test]
async fn get_wsdl_returns_wsdl_xml() {
    let svc = ServerBuilder::from_wsdl_bytes(TEST_WSDL)
        .handler(
            "GetSystemDateAndTime",
            FnHandler::new(|_body: Bytes| async move {
                Ok::<Bytes, SoapFault>(Bytes::from_static(b"<resp/>"))
            }),
        )
        .handler(
            "GetProfiles",
            FnHandler::new(|_body: Bytes| async move {
                Ok::<Bytes, SoapFault>(Bytes::from_static(b"<resp/>"))
            }),
        )
        .auth_bypass(["GetSystemDateAndTime", "GetProfiles"])
        .build()
        .expect("build should succeed");

    let router = svc.into_router();
    let server = TestServer::new(router);

    let resp = server.get("/soap?wsdl").await;
    resp.assert_status_ok();
    let body = resp.text();
    assert!(
        body.contains("wsdl:definitions") || body.contains("definitions"),
        "Response should be WSDL XML, got: {body}"
    );
}

// ── Test 6: GET without ?wsdl returns 404 ─────────────────────────────────────

#[tokio::test]
async fn get_without_wsdl_param_returns_404() {
    let svc = ServerBuilder::from_wsdl_bytes(TEST_WSDL)
        .handler(
            "GetSystemDateAndTime",
            FnHandler::new(|_body: Bytes| async move {
                Ok::<Bytes, SoapFault>(Bytes::from_static(b"<resp/>"))
            }),
        )
        .handler(
            "GetProfiles",
            FnHandler::new(|_body: Bytes| async move {
                Ok::<Bytes, SoapFault>(Bytes::from_static(b"<resp/>"))
            }),
        )
        .auth_bypass(["GetSystemDateAndTime", "GetProfiles"])
        .build()
        .expect("build should succeed");

    let router = svc.into_router();
    let server = TestServer::new(router);

    let resp = server.get("/soap").await;
    resp.assert_status(axum::http::StatusCode::NOT_FOUND);
}
