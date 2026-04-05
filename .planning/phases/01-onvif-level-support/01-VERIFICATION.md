---
phase: 01-onvif-level-support
verified: 2026-04-03T00:00:00Z
status: passed
score: 40/40 requirements verified
re_verification: false
gaps: []
human_verification:
  - test: "Load a real ONVIF camera or ONVIF simulator and send GetSystemDateAndTime over the wire"
    expected: "200 response with valid SOAP 1.2 envelope; WS-Security accepts correct credentials"
    why_human: "All automated tests use fixture-based mocks; real-device wire compatibility requires a live target"
---

# Phase 1: ONVIF-Level Support Verification Report

**Phase Goal:** A consumer can point the server at a real ONVIF WSDL, register raw handlers, and serve authenticated SOAP 1.2 requests over axum — everything onvif-server needs to function
**Verified:** 2026-04-03
**Status:** passed
**Re-verification:** No — initial verification

## Goal Achievement

### Observable Truths

| #  | Truth | Status | Evidence |
|----|-------|--------|----------|
| 1  | `cargo check` passes on the crate skeleton | ✓ VERIFIED | Full test suite compiles; `cargo test` exits 0 with 175 tests passing |
| 2  | All required dependencies present in Cargo.toml with correct versions | ✓ VERIFIED | `Cargo.toml` has roxmltree 0.21, quick-xml 0.39, axum 0.8, sha1 0.11, base64 0.22, chrono 0.4, thiserror 2, bytes 1, async-trait 0.1, http-body-util 0.1 |
| 3  | ONVIF test fixture files exist and are valid XML | ✓ VERIFIED | `tests/fixtures/devicemgmt.wsdl`, `onvif.xsd`, `common.xsd` present; `test_onvif_wsdl_loads` parses all three without error |
| 4  | SoapFault serializes to spec-correct SOAP 1.2 fault XML envelope | ✓ VERIFIED | 11 fault unit tests pass; `to_xml_bytes()` produces `<env:Envelope xmlns:env="http://www.w3.org/2003/05/soap-envelope">` with correct Code/Reason/Detail structure |
| 5  | SoapHandler trait is implementable; FnHandler closure wrapper works | ✓ VERIFIED | `src/handler.rs` — 4 async trait-object tests pass |
| 6  | XSD parser handles all required XSD constructs (XSD-01 through XSD-10) | ✓ VERIFIED | 21 parser unit tests pass covering complexType/sequence/all/choice/extension/restriction/element/attribute/attributeGroup/group/any/anyAttribute/simpleType/list/union/import/include |
| 7  | XSD resolver flattens extension chains correctly (three-level test) | ✓ VERIFIED | `three_level_extension_chain_resolves_in_order` passes; `cycle_detection_returns_err` passes; `diamond_import_loads_schema_once` passes |
| 8  | SOAP 1.2 envelope parser extracts Header children and Body first-child with namespace re-emission | ✓ VERIFIED | `parse_envelope_body_bytes_contain_ancestor_ns_declarations` and 5 other envelope tests pass |
| 9  | WS-Security PasswordDigest computed correctly against known vector | ✓ VERIFIED | `known_vector_digest_matches_expected`: nonce `AAECAwQFBgcICQoLDA0ODw==` + created `2010-09-09T14:18:30.000Z` + password `userpassword` → `QPgtSBfcw764Vty2h0+LsasXgxo=` |
| 10 | Timestamp freshness check accepts/rejects correctly within ±300s | ✓ VERIFIED | 5 timestamp unit tests pass |
| 11 | Nonce replay cache rejects replays, detects across bucket rotation | ✓ VERIFIED | 5 nonce_cache tests pass including `nonce_in_previous_bucket_still_detected_as_replay` |
| 12 | WSDL parser reads all WSDL 1.1 constructs; SOAP 1.2 binding detected | ✓ VERIFIED | 16 wsdl::parser tests pass including `soap12_binding_detected` |
| 13 | WSDL resolver wires cross-references; diamond import deduplication works | ✓ VERIFIED | `two_file_wsdl_merges_operations`, `diamond_import_loads_d_once`, `diamond_import_types_deduplicated_in_registry`, `cycle_import_returns_err_without_stack_overflow` all pass |
| 14 | `rewrite_wsdl_address()` replaces soap:address location attribute | ✓ VERIFIED | 3 rewrite tests pass for both SOAP 1.1 and 1.2 address elements |
| 15 | Dispatch table routes by body element QName (O(1)) with SOAPAction fallback | ✓ VERIFIED | 10 dispatch tests pass including `route_falls_back_to_soap_action_on_unknown_qname` |
| 16 | XSD-11 structural validation rejects missing required elements | ✓ VERIFIED | `validate_request_missing_required_element_returns_sender_fault` passes |
| 17 | ServerBuilder::from_wsdl().handler().auth().build() produces SoapService | ✓ VERIFIED | `server_builder_builds_without_panic` and `server_builder_into_router_returns_router` pass |
| 18 | POST with valid SOAP 1.2 envelope dispatches to handler; returns wrapped response | ✓ VERIFIED | `test_soap12_dispatch` and `post_soap12_valid_envelope_dispatches_to_handler` pass |
| 19 | POST with correct WS-Security UsernameToken PasswordDigest succeeds | ✓ VERIFIED | `test_wssec_valid_credentials_accepted` passes using live `Utc::now()` timestamp |
| 20 | POST with wrong password returns HTTP 500 SOAP fault; handler not called | ✓ VERIFIED | `test_wssec_wrong_password_rejected` passes; `AtomicBool` handler-called flag confirmed false |
| 21 | Auth-bypassed operation dispatches without Security header | ✓ VERIFIED | `test_auth_bypass_get_system_date` and `auth_bypassed_operation_without_security_calls_handler` pass |
| 22 | GET ?wsdl returns WSDL XML with soap:address rewritten | ✓ VERIFIED | `test_wsdl_serving` and `get_wsdl_returns_wsdl_xml` pass |
| 23 | `into_router()` returns an axum::Router composable via `Router::merge` | ✓ VERIFIED | `test_router_composition` merges with health-check route; both routes respond correctly |
| 24 | Real ONVIF devicemgmt.wsdl + onvif.xsd + common.xsd loads without panic | ✓ VERIFIED | `test_onvif_wsdl_loads` loads all three files via FixtureLoader; builds and produces a router |

**Score:** 24/24 truths verified

### Required Artifacts

| Artifact | Provides | Status | Details |
|----------|----------|--------|---------|
| `Cargo.toml` | Crate metadata and all dependency declarations | ✓ VERIFIED | Contains all required deps; async-trait added as required |
| `src/lib.rs` | Public re-exports: SoapService, SoapHandler, SoapFault, FaultCode, FnHandler | ✓ VERIFIED | Exports SoapService, ServerBuilder, SoapHandler, FnHandler, SoapFault, FaultCode, compute_digest, WsdlLoader, WsdlError |
| `src/fault.rs` | SoapFault, FaultCode, to_xml_bytes() | ✓ VERIFIED | Substantive; all fault codes and serialization implemented with 11 unit tests |
| `src/handler.rs` | SoapHandler trait, FnHandler wrapper | ✓ VERIFIED | Async trait + closure wrapper with 4 tests |
| `src/envelope.rs` | parse_envelope(), serialize_envelope(), detect_soap_version() | ✓ VERIFIED | Full NsReader streaming implementation; namespace re-emission on body fragment; 8 tests |
| `src/dispatch.rs` | DispatchTable, build_dispatch_table(), route(), validate_request() | ✓ VERIFIED | 680 lines; O(1) by_element + by_action HashMaps; auth_required field; XSD-11 validation; 13 tests |
| `src/server.rs` | ServerBuilder, SoapService, axum route handlers, security interceptor | ✓ VERIFIED | 637 lines; complete 8-step request pipeline; FileWsdlLoader; 6 unit tests |
| `src/wsdl/definitions.rs` | WsdlDefinition and all WSDL structs | ✓ VERIFIED | Contains WsdlDefinition, Message, PortType, Binding, Operation, Service, Port, SoapBinding, all enums |
| `src/wsdl/parser.rs` | Pass 1 WSDL parser | ✓ VERIFIED | 340+ lines; 16 unit tests; all WSDL 1.1 constructs; SOAP 1.1 and 1.2 binding detection |
| `src/wsdl/resolver.rs` | resolve_wsdl(), WsdlLoader trait, rewrite_wsdl_address() | ✓ VERIFIED | 280+ lines; full cross-reference wiring; diamond import guard; cycle detection; 7 tests |
| `src/xsd/types.rs` | XsdType, ComplexType, SimpleType, TypeRegistry | ✓ VERIFIED | Contains ComplexContent with all variants including ComplexExtension/ComplexRestriction |
| `src/xsd/elements.rs` | XsdElement, XsdAttribute, AttributeGroup, Group, MaxOccurs, Any | ✓ VERIFIED | All required types present with correct field definitions |
| `src/xsd/parser.rs` | Pass 1 XSD parser with all visit_* functions | ✓ VERIFIED | 550+ lines; roxmltree traversal; parse_schema() entry point; 21 unit tests |
| `src/xsd/resolver.rs` | resolve_schema(), SchemaLoader trait, extension chain flattening | ✓ VERIFIED | 380+ lines; three-level chain; cycle detection; diamond dedup; 8 tests |
| `src/wssec/username_token.rs` | parse_username_token(), compute_digest(), validate_username_token() | ✓ VERIFIED | Known vector verified; 12 tests |
| `src/wssec/nonce_cache.rs` | RotatingNonceCache, check_and_insert() | ✓ VERIFIED | Two-bucket rotating design; 5 tests |
| `src/wssec/timestamp.rs` | parse_created(), check_freshness() | ✓ VERIFIED | RFC 3339 parse; ±tolerance check; 5 tests |
| `src/qname.rs` | QName type | ✓ VERIFIED | Hash+Eq for use as HashMap key; 5 tests |
| `tests/fixtures/devicemgmt.wsdl` | ONVIF Device Management WSDL | ✓ VERIFIED | Loads via FixtureLoader in 7 ONVIF integration tests |
| `tests/fixtures/onvif.xsd` | ONVIF core type schema | ✓ VERIFIED | Loaded transitively; schema parses without error |
| `tests/fixtures/common.xsd` | ONVIF common types schema | ✓ VERIFIED | Loaded transitively; schema parses without error |
| `tests/onvif_integration_test.rs` | End-to-end ONVIF integration tests | ✓ VERIFIED | 526 lines; 7 tests covering all 5 roadmap success criteria |
| `tests/integration_test.rs` | Unit-level integration tests | ✓ VERIFIED | 6 tests; generic WSDL-based pipeline tests |

### Key Link Verification

| From | To | Via | Status | Details |
|------|----|-----|--------|---------|
| `src/lib.rs` | `src/server.rs` | `pub use server::{SoapService, ServerBuilder}` | ✓ WIRED | Line 12: `pub use crate::server::{ServerBuilder, SoapService, BuildError, FileWsdlLoader}` |
| `src/lib.rs` | `src/fault.rs` | `pub mod fault` | ✓ WIRED | Line 9: `pub mod fault` |
| `src/lib.rs` | `src/xsd/mod.rs` | `pub(crate) mod xsd` | ✓ WIRED | Line 7: `pub(crate) mod xsd` |
| `src/server.rs` | `src/dispatch.rs` | `dispatch::route()` per request | ✓ WIRED | Line 404: `dispatch::route(&svc.dispatch_table, &body_qname, soap_action)` |
| `src/server.rs` | `src/wssec/username_token.rs` | `validate_username_token()` before dispatch | ✓ WIRED | Line 428: `validate_username_token(security_bytes, auth_fn.as_ref(), ...)` |
| `src/server.rs` | `src/envelope.rs` | `parse_envelope()` + `serialize_envelope()` | ✓ WIRED | Lines 386, 457: both called in request pipeline |
| `src/dispatch.rs` | `src/handler.rs` | `Arc<dyn SoapHandler>` per entry | ✓ WIRED | `DispatchEntry.handler: Arc<dyn SoapHandler>` |
| `src/dispatch.rs` | `src/wsdl/resolver.rs` | Consumes `ResolvedWsdl` | ✓ WIRED | `build_dispatch_table(resolved: &ResolvedWsdl, ...)` |
| `src/envelope.rs` | `quick_xml::NsReader` | Streaming parse | ✓ WIRED | `NsReader::from_reader(input)` at line 36 |
| `src/wssec/nonce_cache.rs` | `tokio::sync::Mutex` | Wrapped in Arc<Mutex> in SoapService | ✓ WIRED | `Arc<Mutex<RotatingNonceCache>>` in server.rs |
| `src/wssec/username_token.rs` | `src/wssec/nonce_cache.rs` | `nonce_cache.check_and_insert()` | ✓ WIRED | Line 233: `nonce_cache.check_and_insert(nonce)?` |
| `src/wssec/username_token.rs` | `src/wssec/timestamp.rs` | `check_freshness()` call | ✓ WIRED | Line 228: `check_freshness(now, created_dt, tolerance_secs)?` |
| `src/wsdl/parser.rs` | `src/wsdl/definitions.rs` | Returns `WsdlDefinition` | ✓ WIRED | `parse_wsdl() -> Result<WsdlDefinition, WsdlError>` |
| `src/wsdl/resolver.rs` | `src/xsd/parser.rs` | `xsd::parse_schema()` on inline schemas | ✓ WIRED | `parse_schema` imported and called |
| `src/wsdl/resolver.rs` | `src/xsd/resolver.rs` | `xsd::resolve_schema()` after schema collection | ✓ WIRED | `resolve_schema` imported and called |
| `src/xsd/parser.rs` | `roxmltree` | `roxmltree::Node` traversal | ✓ WIRED | `use roxmltree::Node` at line 3 |
| `src/xsd/resolver.rs` | `src/xsd/parser.rs` | Consumes `RawSchema` | ✓ WIRED | `resolve_schema(raw: RawSchema, ...)` |
| `tests/onvif_integration_test.rs` | `tests/fixtures/devicemgmt.wsdl` | `include_bytes!` | ✓ WIRED | `include_bytes!("fixtures/devicemgmt.wsdl")` at line 129 |
| `tests/onvif_integration_test.rs` | `src/lib.rs` | Uses `ServerBuilder, SoapHandler, SoapFault` | ✓ WIRED | Line 14: `use soap_server::{compute_digest, FnHandler, SoapFault, ServerBuilder, WsdlLoader}` |

### Requirements Coverage

| Requirement | Source Plan | Description | Status | Evidence |
|-------------|-------------|-------------|--------|----------|
| XSD-01 | 01-01, 01-03 | Parser reads XSD schemas and constructs in-memory type graph | ✓ SATISFIED | `parse_schema()` in parser.rs; 21 parser tests pass |
| XSD-02 | 01-03 | Supports xs:sequence, xs:all, xs:choice | ✓ SATISFIED | `visit_sequence`, `visit_all`, `visit_choice` in parser.rs; `parse_all_compositor`, `parse_choice_compositor` tests pass |
| XSD-03 | 01-05 | Supports xs:extension and xs:restriction with recursive chain resolution | ✓ SATISFIED | `three_level_extension_chain_resolves_in_order` test passes |
| XSD-04 | 01-05 | Supports xs:import and xs:include with cycle detection and caching | ✓ SATISFIED | `diamond_import_loads_schema_once`, `cycle_detection_returns_err` tests pass |
| XSD-05 | 01-03 | Supports xs:element with ref, minOccurs, maxOccurs, nillable, default, fixed | ✓ SATISFIED | 5 element-specific tests pass in xsd::parser |
| XSD-06 | 01-03 | Supports xs:attribute and xs:attributeGroup | ✓ SATISFIED | `parse_attribute_use_required`, `parse_attribute_group_definition` tests pass |
| XSD-07 | 01-03 | Supports xs:group for reusable content groups | ✓ SATISFIED | `parse_group_definition_sequence` test passes |
| XSD-08 | 01-03 | Supports xs:any and xs:anyAttribute | ✓ SATISFIED | `parse_any_element` test passes |
| XSD-09 | 01-03 | Supports xs:simpleType restrictions | ✓ SATISFIED | `parse_restriction_facets`, `parse_simple_type_enumeration` tests pass |
| XSD-10 | 01-03 | Supports xs:list and xs:union | ✓ SATISFIED | `parse_simple_type_list`, `parse_simple_type_union` tests pass |
| XSD-11 | 01-08 | Payload validation before handler invocation | ✓ SATISFIED | `validate_request()` in dispatch.rs; `validate_request_missing_required_element_returns_sender_fault` passes |
| WSDL-01 | 01-06, 01-10 | Parser reads WSDL 1.1 and constructs in-memory representation | ✓ SATISFIED | `parse_wsdl()` in parser.rs; 16 parser tests pass |
| WSDL-02 | 01-06, 01-07 | Two-pass resolution: parse + wire cross-references | ✓ SATISFIED | `parse_wsdl()` + `resolve_wsdl()` dual-pass; `two_file_wsdl_merges_operations` passes |
| WSDL-03 | 01-07 | Import resolution with diamond import and cycle prevention | ✓ SATISFIED | `diamond_import_loads_d_once`, `cycle_import_returns_err_without_stack_overflow` pass |
| WSDL-04 | 01-07 | GET ?wsdl with soap:address rewriting | ✓ SATISFIED | `rewrite_wsdl_address()` in resolver.rs; 3 rewrite tests; `test_wsdl_serving` integration test passes |
| WSDL-05 | 01-10 | Imported XSD schemas inlined or served | ✓ SATISFIED | Inline xs:schema nodes in wsdl:types are parsed via `standalone_wsdl_resolves_inline_schema`; external XSD loaded via SchemaLoader; `test_onvif_wsdl_loads` resolves multi-file WSDL |
| ENV-01 | 01-04 | Parse SOAP 1.2 envelope — extract Header children and Body first child | ✓ SATISFIED | `parse_envelope()` with NsReader; `parse_envelope_with_header_child`, `parse_envelope_minimal_soap12_empty_body_child` pass |
| ENV-02 | 01-04 | Serialize SOAP 1.2 response envelope | ✓ SATISFIED | `serialize_envelope()` in envelope.rs; `serialize_envelope_wraps_body_in_soap12` passes |
| ENV-03 | 01-04 | Detect SOAP version from Content-Type | ✓ SATISFIED | `detect_soap_version()` handles application/soap+xml and text/xml; 4 tests pass |
| ENV-04 | 01-09 | Set correct response Content-Type matching request SOAP version | ✓ SATISFIED | `response_content_type()` in envelope.rs; server.rs line 458 uses it for response |
| FLT-01 | 01-02 | Generate spec-correct SOAP 1.2 faults with Code/Value, Reason/Text, Detail | ✓ SATISFIED | `to_xml_bytes()` in fault.rs; `serialize_fault_wraps_in_soap12_envelope` and detail tests pass |
| FLT-02 | 01-02 | Support all 5 standard fault codes | ✓ SATISFIED | FaultCode enum with VersionMismatch, MustUnderstand, DataEncodingUnknown, Sender, Receiver; 5 as_str tests pass |
| FLT-03 | 01-02 | Return HTTP 500 for SOAP 1.2 faults | ✓ SATISFIED | `fault_response()` in server.rs returns `StatusCode::INTERNAL_SERVER_ERROR`; `post_with_wrong_password_returns_fault_handler_not_called` verifies 500 |
| DSP-01 | 01-08 | Document/literal dispatch by body element QName | ✓ SATISFIED | `by_element: HashMap<QName, DispatchEntry>` in dispatch.rs; `dispatch_table_routes_by_element_qname` passes |
| DSP-02 | 01-08 | SOAPAction header as secondary dispatch hint | ✓ SATISFIED | `by_action: HashMap<String, DispatchEntry>`; `route_falls_back_to_soap_action_on_unknown_qname` passes |
| DSP-03 | 01-08 | Dispatch table built at startup, not per-request | ✓ SATISFIED | `build_dispatch_table()` called once in `ServerBuilder::build()`; table stored in `Arc<DispatchTable>` |
| DSP-04 | 01-08 | Unmatched operations produce SOAP Fault | ✓ SATISFIED | `route()` returns `Err(SoapFault::action_not_supported(...))` on miss; `route_unknown_qname_no_soap_action_returns_sender_fault` passes |
| HDL-01 | 01-02 | Raw handler trait: XML bytes in, XML bytes out or SoapFault | ✓ SATISFIED | `SoapHandler::handle(&self, body: Bytes) -> Result<Bytes, SoapFault>` |
| HDL-02 | 01-02 | Async handler support | ✓ SATISFIED | `#[async_trait]` on SoapHandler; all handlers are async |
| HDL-03 | 01-02 | Closure-based handler registration | ✓ SATISFIED | `FnHandler<F>` wrapper; `.handler("Op", FnHandler::new(|body| async { ... }))` API |
| SEC-01 | 01-06 | Extract wsse:Security header from SOAP Header | ✓ SATISFIED | `find_security_header()` in server.rs searches header_children for "Security"; `parse_username_token()` extracts from bytes |
| SEC-02 | 01-06 | WS-Security PasswordDigest validation | ✓ SATISFIED | `compute_digest()`: `Base64(SHA-1(Base64Decode(Nonce) + Created + Password))`; known vector test passes |
| SEC-03 | 01-06 | PasswordText direct comparison | ✓ SATISFIED | `PasswordType::Text` branch in `validate_username_token()`; `validate_text_password_correct` and wrong tests pass |
| SEC-04 | 01-04 | Timestamp validation with configurable tolerance | ✓ SATISFIED | `check_freshness()` in timestamp.rs; default 300s; `validate_expired_timestamp_returns_err` passes |
| SEC-05 | 01-04 | Nonce replay prevention with rotating bucket cache | ✓ SATISFIED | `RotatingNonceCache` with two-bucket design; `nonce_in_previous_bucket_still_detected_as_replay` passes |
| SEC-06 | 01-08, 01-09 | Per-operation auth bypass list | ✓ SATISFIED | `auth_bypass: HashSet<String>` in ServerBuilder; `auth_required = !auth_bypass.contains(&op_name)` in dispatch; `test_auth_bypass_get_system_date` passes |
| SEC-07 | 01-09 | Reject unauthenticated requests with SOAP Fault | ✓ SATISFIED | server.rs checks `entry.auth_required`, returns `fault_response(SoapFault::sender("WS-Security header required..."))` if missing; `test_wssec_wrong_password_rejected` passes |
| HTTP-01 | 01-09 | axum Router integration | ✓ SATISFIED | `SoapService::into_router()` returns `axum::Router`; `test_router_composition` uses `Router::merge()` |
| HTTP-02 | 01-09 | POST handler for SOAP requests | ✓ SATISFIED | `routing::post(soap_post_handler)` in `into_router()`; all SOAP dispatch tests pass |
| HTTP-03 | 01-09 | GET handler for WSDL serving with ?wsdl | ✓ SATISFIED | `.get(wsdl_get_handler)` in `into_router()`; `get_without_wsdl_param_returns_404` and `test_wsdl_serving` pass |
| HTTP-04 | 01-09 | Server builder API | ✓ SATISFIED | `ServerBuilder::from_wsdl_bytes().handler().auth().auth_bypass().build()?.into_router()` — full API present and tested |

**Requirements satisfied:** 40/40

### Anti-Patterns Found

| File | Pattern | Severity | Impact |
|------|---------|----------|--------|
| `src/fault.rs` line 1 | `// TODO: SOAP 1.2 fault generation` | ℹ Info | Comment is stale — the full implementation is present below it. No functional impact. |
| `src/envelope.rs` line 1 | `// TODO: SOAP 1.2 envelope parsing and serialization` | ℹ Info | Comment is stale — full NsReader implementation is present. No functional impact. |

No blockers. No stubs. Both TODO comments are leftover artifacts from plan task headers, not indicators of missing code. The implementations below them are complete.

### Human Verification Required

#### 1. Real-device ONVIF wire compatibility

**Test:** Connect to a real ONVIF-capable IP camera or run an ONVIF device simulator (e.g., ONVIF Device Manager emulator). Point it at a running instance of this server. Send GetSystemDateAndTime and GetDeviceInformation requests.
**Expected:** Server returns valid SOAP 1.2 responses; ONVIF client tools accept the responses as conformant; WS-Security negotiation completes successfully.
**Why human:** All automated tests use mock loaders and test fixtures. Real ONVIF clients may send namespace prefixes, attribute orderings, or UsernameToken structures that differ slightly from what the fixtures exercise.

### Gaps Summary

No gaps. All 40 phase requirements are satisfied. The full test suite passes:

- **162 unit tests** (162 passed, 0 failed) covering all modules
- **6 integration tests** covering the generic SOAP pipeline
- **7 ONVIF integration tests** covering all 5 roadmap success criteria with real ONVIF fixtures

The phase goal is fully achieved: a consumer can load the real ONVIF `devicemgmt.wsdl`, register handlers, configure WS-Security, and get an `axum::Router` that serves authenticated SOAP 1.2 requests.

---
_Verified: 2026-04-03_
_Verifier: Claude (gsd-verifier)_
