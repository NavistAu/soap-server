---
phase: 02-full-spec-compliance
verified: 2026-04-05T00:00:00Z
status: passed
score: 13/13 must-haves verified
re_verification: false
---

# Phase 2: Full Spec Compliance Verification Report

**Phase Goal:** Full SOAP 1.1 spec compliance — envelope handling, fault serialization, RPC dispatch, and multi-service routing
**Verified:** 2026-04-05
**Status:** PASSED
**Re-verification:** No — initial verification

## Goal Achievement

### Observable Truths

| # | Truth | Status | Evidence |
|---|-------|--------|----------|
| 1 | A POST with Content-Type: text/xml and a SOAP 1.1 envelope is parsed and dispatched without error | VERIFIED | `parse_envelope` checks `SOAP11_NS` bytes; `soap11_end_to_end` integration test passes |
| 2 | A SOAP 1.1 successful response is wrapped in a SOAP 1.1 envelope with the correct namespace | VERIFIED | `serialize_envelope(body, SoapVersion::Soap11)` emits `http://schemas.xmlsoap.org/soap/envelope/`; `serialize_envelope_soap11` unit test + integration test pass |
| 3 | A SOAP 1.1 response carries Content-Type: text/xml, not application/soap+xml | VERIFIED | `response_content_type(&SoapVersion::Soap11)` returns `"text/xml; charset=utf-8"`; `response_content_type_soap11` unit test + `soap11_end_to_end` integration test confirm |
| 4 | fault_response() accepts a SoapVersion and emits the matching Content-Type | VERIFIED | Signature is `fn fault_response(fault: SoapFault, version: SoapVersion)` using `response_content_type(&version)`; `fault_response_soap11_content_type` and `fault_response_soap12_content_type` unit tests pass |
| 5 | SoapFault serialized for SOAP 1.1 produces faultcode/faultstring elements under SOAP-ENV:Fault | VERIFIED | `to_xml_bytes_v11()` produces `<faultcode>`, `<faultstring>`, `SOAP-ENV:Fault`; 8 unit tests confirm |
| 6 | FaultCode::Sender serializes as SOAP-ENV:Client in SOAP 1.1 (not env:Sender) | VERIFIED | `fault_code_sender_maps_to_client` unit test passes; `soap11_fault_has_correct_structure` integration test confirms |
| 7 | FaultCode::Receiver serializes as SOAP-ENV:Server in SOAP 1.1 | VERIFIED | `fault_code_receiver_maps_to_server` unit test passes |
| 8 | SOAP 1.1 fault body is wrapped in a SOAP-ENV:Envelope with the correct 1.1 namespace | VERIFIED | `fault_soap11_namespace` and `fault_soap11_wraps_in_envelope` unit tests pass |
| 9 | A full SOAP 1.1 request that triggers a fault receives a SOAP 1.1 fault envelope response | VERIFIED | `soap11_fault_has_correct_structure` and `soap11_fault_content_type_is_text_xml` integration tests pass |
| 10 | An RPC/encoded binding builds a dispatch table entry keyed by (soap:body namespace, operation name) | VERIFIED | `BindingStyle::Rpc` branch in `collect_ops_for_service` synthesizes `QName::new(ns, &op_name)`; `build_dispatch_table_rpc_binding` unit test passes |
| 11 | A multi-service WSDL builds separate DispatchTables per service, one per axum route | VERIFIED | `service_tables: HashMap<String, Arc<DispatchTable>>` built per-service in `ServerBuilder::build()`; `into_router()` mounts one route per entry |
| 12 | A request to service A's path routes to service A's handlers, not service B's | VERIFIED | `multi_service_routing` integration test: POST to `/soap/b` with service A's op returns 500; POST to correct paths returns 200 |
| 13 | The single-service ServerBuilder API is backward-compatible — existing code builds unchanged | VERIFIED | `service_tables` is empty in single-service mode; existing handler path is unchanged; all 7 ONVIF integration tests pass |

**Score:** 13/13 truths verified

---

### Required Artifacts

| Artifact | Expected | Status | Details |
|----------|----------|--------|---------|
| `src/envelope.rs` | `parse_envelope_soap11` and `serialize_envelope_soap11` unit tests, `response_content_type` correct | VERIFIED | 6 SOAP 1.1 unit tests present in `#[cfg(test)]` block: `parse_envelope_soap11`, `parse_envelope_soap11_with_header`, `parse_envelope_soap11_body_first_child`, `serialize_envelope_soap11`, `response_content_type_soap11`, `response_content_type_soap12` |
| `src/server.rs` | `fault_response(fault, version)` signature accepting `SoapVersion` | VERIFIED | Line 435: `fn fault_response(fault: SoapFault, version: crate::wsdl::definitions::SoapVersion) -> Response` |
| `src/fault.rs` | `to_xml_bytes_v11()` and `to_xml_bytes_versioned(version)` | VERIFIED | Both methods present in `impl SoapFault`; 12 SOAP 1.1 fault unit tests present |
| `src/server.rs` | `fault_response` calls `to_xml_bytes_versioned` | VERIFIED | Line 436: `let bytes = fault.to_xml_bytes_versioned(&version);` |
| `tests/integration_test.rs` | `soap11_end_to_end`, `soap11_fault_has_correct_structure`, `soap11_fault_content_type_is_text_xml` | VERIFIED | All three tests present and passing |
| `src/dispatch.rs` | `build_dispatch_table_for_service()` + RPC QName derivation | VERIFIED | `pub fn build_dispatch_table_for_service` at line 87; `BindingStyle::Rpc` branch at line 117 and 197 |
| `src/server.rs` | `SoapService` with `service_tables: HashMap<String, Arc<DispatchTable>>` and multi-route axum router | VERIFIED | Field at line 375; multi-route mounting in `into_router()` |
| `tests/integration_test.rs` | `rpc_dispatch_by_wrapper_element`, `multi_service_routing` | VERIFIED | `multi_service_routing` and `rpc_dispatch_integration` present and passing |

---

### Key Link Verification

| From | To | Via | Status | Details |
|------|----|-----|--------|---------|
| `src/server.rs soap_post_handler` | `fault_response` | passes `soap_version` after detection | WIRED | 8 call sites verified: pre-detection passes `SoapVersion::Soap12` as default; post-detection passes `envelope.soap_version.clone()` |
| `src/server.rs fault_response` | `src/fault.rs SoapFault::to_xml_bytes_versioned` | passes `SoapVersion` parameter | WIRED | `fault.to_xml_bytes_versioned(&version)` at line 436 |
| `src/dispatch.rs build_dispatch_table` | `BindingStyle::Rpc` branch | checks binding style | WIRED | `if style == BindingStyle::Rpc` at lines 117 and 197 synthesizes QName from `(rpc_ns, op_name)` |
| `src/server.rs ServerBuilder::build` | per-service `DispatchTable` | iterates services, calls `build_dispatch_table_for_service` per service | WIRED | `service_tables` HashMap built in multi-service branch; `into_router()` registers one POST route per entry |

---

### Requirements Coverage

| Requirement | Source Plan | Description | Status | Evidence |
|-------------|------------|-------------|--------|----------|
| ENV-05 | 02-01 | Parse SOAP 1.1 envelope (backfill) | SATISFIED | `parse_envelope()` already handles `SOAP11_NS`; 3 SOAP 1.1 parse unit tests pass; `soap11_end_to_end` integration test passes. Note: REQUIREMENTS.md body checkbox and traceability table both show this as incomplete — documentation not updated after implementation |
| ENV-06 | 02-01 | Serialize SOAP 1.1 response envelope (backfill) | SATISFIED | `serialize_envelope(body, SoapVersion::Soap11)` emits correct 1.1 namespace; `serialize_envelope_soap11` unit test + integration test pass. Same documentation gap as ENV-05 |
| FLT-04 | 02-02 | Generate spec-correct SOAP 1.1 faults with faultcode, faultstring (backfill) | SATISFIED | `to_xml_bytes_v11()` produces spec-correct structure; 8 unit tests verify all aspects |
| FLT-05 | 02-02 | Map fault codes between versions (Sender/Client, Receiver/Server) (backfill) | SATISFIED | `FaultCode::Sender` → `"SOAP-ENV:Client"`, `FaultCode::Receiver` → `"SOAP-ENV:Server"`; unit tests `fault_code_sender_maps_to_client` and `fault_code_receiver_maps_to_server` pass |
| DSP-05 | 02-03 | RPC/encoded binding style dispatch (backfill) | SATISFIED | QName synthesized from `(soap:body namespace or targetNamespace, operation name)` for `BindingStyle::Rpc`; `rpc_dispatch_integration` integration test passes |
| DSP-06 | 02-03 | Multiple services per WSDL — dispatch across services, each with its own operation table (backfill) | SATISFIED | `service_tables` HashMap per service; separate axum routes; `multi_service_routing` integration test verifies isolation |

**Documentation Gap (non-blocking):** REQUIREMENTS.md traceability table shows ENV-05 and ENV-06 as "Pending" and the checkbox list shows them as `[ ]` (unchecked). The implementation is complete and tested. This is a stale documentation issue only — the traceability table was not updated to "Complete" after implementation.

---

### Anti-Patterns Found

| File | Line | Pattern | Severity | Impact |
|------|------|---------|----------|--------|
| `src/envelope.rs` | 1 | `// TODO: SOAP 1.2 envelope parsing and serialization` | Info | Stale file-header comment from phase 1 scaffolding; implementation is complete |
| `src/fault.rs` | 1 | `// TODO: SOAP 1.2 fault generation` | Info | Stale file-header comment from phase 1 scaffolding; implementation is complete |

No blockers. No functional stubs. All TODOs are stale header comments that predate the implementations, not markers for missing work.

---

### Human Verification Required

None. All observable behaviors are verified programmatically via unit tests and integration tests.

---

### Gaps Summary

No gaps. All 13 must-haves are verified. The full test suite (186 unit tests + 11 integration tests + 7 ONVIF integration tests = 204 total) passes with zero failures.

The only non-blocking finding is that REQUIREMENTS.md was not updated after phase 2 implementation: ENV-05 and ENV-06 remain marked `[ ]` in the requirements list and "Pending" in the traceability table. This does not affect functionality.

---

_Verified: 2026-04-05_
_Verifier: Claude (gsd-verifier)_
