# Phase 2: Full Spec Compliance - Research

**Researched:** 2026-04-03
**Domain:** SOAP 1.1 envelope/fault, RPC/encoded dispatch, multi-service WSDL routing
**Confidence:** HIGH

---

<user_constraints>
## User Constraints (from CONTEXT.md)

### Locked Decisions
- Same porting approach as Phase 1: match the spec and follow the best available prior art
- DESIGN.md remains the authoritative architecture specification
- Extend existing modules (envelope.rs, fault.rs, dispatch.rs) rather than creating new ones

### Claude's Discretion
- **RPC/encoded depth:** How far to implement RPC/encoded binding — minimal dispatch-only, or full SOAP Section 5 encoding rules (multi-ref, array types). Research should check what python-zeep, node-soap, and Apache CXF actually support and what real-world RPC/encoded WSDLs look like in practice.
- **Multi-service routing model:** Whether multiple services share one path and dispatch by QName/SOAPAction, or each service gets its own URL path prefix. Research should check how node-soap and Spring-WS handle multi-service WSDLs and what the WSDL 1.1 spec says about port addresses.
- **SOAP 1.1 fault mapping:** How to implement the bidirectional fault code mapping (Sender↔Client, Receiver↔Server) cleanly in the existing FaultCode enum. Research should determine if this belongs in serialization or in the type system.

### Deferred Ideas (OUT OF SCOPE)
None — discussion stayed within phase scope
</user_constraints>

---

<phase_requirements>
## Phase Requirements

| ID | Description | Research Support |
|----|-------------|-----------------|
| ENV-05 | Parse SOAP 1.1 envelope | SOAP 1.1 uses namespace `http://schemas.xmlsoap.org/soap/envelope/` — envelope.rs already checks this namespace in parse_envelope(); only the serialization path needs a 1.1-aware test |
| ENV-06 | Serialize SOAP 1.1 response envelope | serialize_envelope() already switches on SoapVersion::Soap11 — confirms the code path exists; needs integration test coverage |
| FLT-04 | Generate spec-correct SOAP 1.1 faults with faultcode/faultstring/faultactor/detail | SOAP 1.1 fault structure is entirely different from 1.2; needs new serializer on SoapFault |
| FLT-05 | Map fault codes between versions (Sender↔Client, Receiver↔Server) | Mapping belongs in fault serialization, not the type system — FaultCode enum stays canonical; serializer branches on version |
| DSP-05 | RPC/encoded binding style dispatch | RPC wrapper element carries operation name + namespace from soap:body; dispatch by wrapper element local name plus SOAPAction fallback |
| DSP-06 | Multiple services per WSDL — each with its own operation table | Per-service DispatchTable keyed by service name; ServerBuilder maps service→path or shares single path with per-service table lookup |
</phase_requirements>

---

## Summary

Phase 2 adds three distinct capabilities on top of Phase 1's SOAP 1.2 pipeline: SOAP 1.1 envelope parsing/serialization, SOAP 1.1 fault format, and RPC/encoded dispatch with multi-service routing.

The good news is that the envelope.rs parsing code already handles SOAP 1.1 namespaces — `parse_envelope()` checks both `http://schemas.xmlsoap.org/soap/envelope/` and `http://www.w3.org/2003/05/soap-envelope` and sets `SoapVersion::Soap11` correctly. `serialize_envelope()` already branches on `SoapVersion::Soap11` and emits the right namespace. The two envelope requirements (ENV-05, ENV-06) are structurally already implemented; they need dedicated test coverage to confirm correctness end-to-end, and `server.rs`'s `fault_response` helper still hard-codes `application/soap+xml` content-type rather than reading from the detected version.

The fault requirements (FLT-04, FLT-05) require the most new code. SOAP 1.1 faults use a completely different XML structure — flat elements `<faultcode>`, `<faultstring>`, `<faultactor>`, `<detail>` directly under `<Fault>`, versus SOAP 1.2's nested `<Code>/<Value>` and `<Reason>/<Text>`. The fault code names also differ: SOAP 1.1 uses `SOAP-ENV:Client` and `SOAP-ENV:Server` while SOAP 1.2 uses `env:Sender` and `env:Receiver`. The clean implementation places this mapping in the serialization layer — `SoapFault::to_xml_bytes()` takes a `SoapVersion` parameter, and the same `FaultCode::Sender` enum variant serializes as `env:Sender` for 1.2 and `SOAP-ENV:Client` for 1.1.

For RPC/encoded dispatch (DSP-05), the research finding is that full SOAP Section 5 encoding (multi-ref, array serialization) is not needed at the dispatch layer. Dispatch only requires recognizing the wrapper element's local name as the operation name. The `soap:body namespace` attribute from the WSDL binding provides the namespace for that wrapper. This is a small addition to `build_dispatch_table()` — when `BindingStyle::Rpc`, derive the dispatch QName from operation name + `soap:body.namespace`. For multi-service (DSP-06), node-soap's proven model is one path per port address from the WSDL, with per-service dispatch tables — the `ServerBuilder` should expose a multi-service builder API and `SoapService` stores `HashMap<String, Arc<DispatchTable>>` keyed by service path.

**Primary recommendation:** Implement in three discrete units: (1) envelope tests + fault_response content-type fix, (2) versioned fault serializer with Client/Server mapping, (3) RPC dispatch key derivation + per-service table map.

---

## Standard Stack

### Core (unchanged from Phase 1)
| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| quick-xml | 0.39 | SOAP envelope streaming parse/serialize | Already in use; streaming handles per-request XML |
| axum | 0.8 | HTTP routing | Already in use |
| bytes | 1 | Zero-copy byte slices | Already in use |
| thiserror | 2 | Error derivation | Already in use |

No new dependencies required for Phase 2. All work is additive changes to existing modules.

---

## Architecture Patterns

### Pattern 1: Versioned Fault Serialization (FLT-04, FLT-05)

**What:** `SoapFault::to_xml_bytes()` gains a `version: SoapVersion` parameter. A second free function `to_xml_bytes_v11()` is also acceptable if the method signature change is undesirable (avoids touching all existing callers).

**When to use:** Whenever a fault is returned from the pipeline. The `SoapVersion` detected at envelope parse time flows through to fault generation.

**SOAP 1.2 structure (existing):**
```xml
<env:Envelope xmlns:env="http://www.w3.org/2003/05/soap-envelope">
  <env:Body>
    <env:Fault>
      <env:Code><env:Value>env:Sender</env:Value></env:Code>
      <env:Reason><env:Text xml:lang="en">reason</env:Text></env:Reason>
      <!-- optional: <env:Detail>...</env:Detail> -->
    </env:Fault>
  </env:Body>
</env:Envelope>
```

**SOAP 1.1 structure (new):**
```xml
<SOAP-ENV:Envelope xmlns:SOAP-ENV="http://schemas.xmlsoap.org/soap/envelope/">
  <SOAP-ENV:Body>
    <SOAP-ENV:Fault>
      <faultcode>SOAP-ENV:Client</faultcode>
      <faultstring>reason</faultstring>
      <!-- optional: <faultactor>URI</faultactor> -->
      <!-- optional: <detail>...</detail> -->
    </SOAP-ENV:Fault>
  </SOAP-ENV:Body>
</SOAP-ENV:Envelope>
```

Source: W3C SOAP 1.1 spec Section 4.4, confirmed by tutorialspoint example and W3C spec at https://www.w3.org/TR/2000/NOTE-SOAP-20000508/

**FaultCode mapping table:**

| FaultCode variant | SOAP 1.2 value | SOAP 1.1 value |
|-------------------|----------------|----------------|
| `Sender` | `env:Sender` | `SOAP-ENV:Client` |
| `Receiver` | `env:Receiver` | `SOAP-ENV:Server` |
| `VersionMismatch` | `env:VersionMismatch` | `SOAP-ENV:VersionMismatch` |
| `MustUnderstand` | `env:MustUnderstand` | `SOAP-ENV:MustUnderstand` |
| `DataEncodingUnknown` | `env:DataEncodingUnknown` | not a standard 1.1 code — emit as `SOAP-ENV:Server` |

**Key design decision:** Keep `FaultCode` enum with `Sender`/`Receiver` as the canonical SOAP 1.2 names. The mapping to 1.1 names (`Client`/`Server`) lives entirely in the serialization function. No `Client` or `Server` variants needed in the enum — that would force callers to know which version to use when creating a fault.

**Recommended implementation:**
```rust
impl SoapFault {
    /// Serialize to a complete SOAP envelope. Version determines fault structure and code names.
    pub fn to_xml_bytes_versioned(&self, version: &SoapVersion) -> Vec<u8> {
        match version {
            SoapVersion::Soap12 => self.to_xml_bytes(),   // existing
            SoapVersion::Soap11 => self.to_xml_bytes_v11(),  // new
        }
    }

    fn to_xml_bytes_v11(&self) -> Vec<u8> {
        let ns = "http://schemas.xmlsoap.org/soap/envelope/";
        let faultcode = match &self.code {
            FaultCode::Sender => "SOAP-ENV:Client",
            FaultCode::Receiver => "SOAP-ENV:Server",
            FaultCode::VersionMismatch => "SOAP-ENV:VersionMismatch",
            FaultCode::MustUnderstand => "SOAP-ENV:MustUnderstand",
            FaultCode::DataEncodingUnknown => "SOAP-ENV:Server",
        };
        // ... serialize faultstring, optional detail
    }
}
```

### Pattern 2: RPC Dispatch Key Derivation (DSP-05)

**What:** For bindings with `BindingStyle::Rpc`, derive the dispatch QName from `(operation_name, soap_body_namespace)` rather than from the message part's `element` attribute.

**Background:** In RPC style, the SOAP body contains a wrapper element whose local name is the operation name and whose namespace comes from the `soap:body namespace="..."` attribute in the WSDL binding. Each parameter is a child element of that wrapper. There is no `element` reference on the message part for RPC — message parts use `type` references instead.

**Current code path in dispatch.rs** calls `resolve_input_element()` which looks for `first_part.element` — this returns `None` for RPC bindings. The fix is to detect `BindingStyle::Rpc` and synthesize the dispatch QName differently.

```rust
// In build_dispatch_table(), where input_type is resolved:
let input_type = if binding.soap_binding.style == BindingStyle::Rpc {
    // RPC: wrapper element QName = (soap_body_namespace, op_name)
    let ns = binding_op.input.body.namespace.as_deref().unwrap_or("");
    Some(QName::new(ns, &binding_op.name))
} else {
    // Document: existing resolve_input_element() logic
    resolve_input_element(resolved, &op_name)
};
```

The `by_element` HashMap then holds this synthesized QName, and the routing logic (`route()`) needs no changes — it still dispatches by body first-child QName.

**Section 5 encoding depth decision:** Implement dispatch-only. Do not implement multi-ref serialization, array encoding, or `xsi:type` injection. python-zeep's `RpcMessage` class confirms this is sufficient for server-side routing — the server receives parameters as children of the wrapper element and passes the raw wrapper bytes to the handler, same as document/literal. Full Section 5 encoding is a client-side serialization concern; servers only need to decode.

### Pattern 3: Multi-Service Dispatch Table (DSP-06)

**What:** `DispatchTable` is currently a single flat table. Multi-service support requires per-service tables so each service's operations are isolated.

**node-soap model (confirmed by source review):** A single HTTP path handles all services; routing to the correct per-service table is done by matching the WSDL port's `soap:address location` against the incoming request URL path. Each port has its own `topElements` map.

**WSDL 1.1 spec constraint:** A port MUST NOT specify more than one address. Multiple services each define their own ports with distinct addresses. This means each service naturally gets its own URL path.

**Recommended approach — per-service path routing:**

```rust
// ServerBuilder accumulates per-service handler maps
// SoapService stores a map from path -> DispatchTable
pub struct SoapService {
    // existing single-service fields remain for backward compat
    dispatch_table: Arc<DispatchTable>,
    // NEW: per-service tables; key is the mount path (e.g. "/soap/DeviceService")
    service_tables: HashMap<String, Arc<DispatchTable>>,
    // ... rest unchanged
}
```

The axum router registers one POST route per service path. `ServerBuilder::build()` iterates WSDL services, extracts each port's address path, and builds a `DispatchTable` per service.

**Backward compatibility:** If the WSDL has exactly one service, the existing `dispatch_table` field continues to work. The `service_tables` map is only consulted when non-empty.

**Simplest viable implementation for DSP-06:** Build one `DispatchTable` per service (not per port/binding), collect all binding operations for that service's ports, and mount each service at a configurable path. The `ServerBuilder` gains a `service_path(service_name, path)` method; if not configured, derive from the WSDL port address URL path.

### Anti-Patterns to Avoid

- **Putting `Client`/`Server` in FaultCode enum:** Forces callers to choose version when creating a fault; better handled in serialization.
- **Single dispatch table for multi-service:** Operations with the same name in different services collide. Must be per-service.
- **Implementing full Section 5 encoding for server-side dispatch:** Multi-ref and `href` resolution is client-side serialization. The server receives the wire bytes; dispatch only needs the wrapper element local name.
- **Hard-coding `fault_response()` to SOAP 1.2 content-type:** `server.rs`'s `fault_response()` currently always returns `application/soap+xml`. It must accept the detected `SoapVersion` and use `response_content_type()` to match the request version.

---

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| SOAP 1.1 fault XML template | A separate XML builder | Format string with fault fields | Fault XML is tiny and fixed-structure; quick-xml overkill |
| RPC namespace resolution | A full WSDL namespace resolver | Read `soap:body.namespace` already parsed in `SoapBody.namespace` | `SoapBody.namespace` field already exists in `wsdl/definitions.rs` |
| Multi-service path routing | A custom router layer on top of axum | Multiple axum routes, one per service path | axum's router already handles this; no custom middleware needed |
| Version detection in fault_response | A new version-detection pass | Pass `SoapVersion` already detected in step 1 of the pipeline | The version is already in scope when faults are generated |

---

## Common Pitfalls

### Pitfall 1: fault_response() Ignores SoapVersion

**What goes wrong:** `server.rs`'s `fault_response()` is called in multiple places in the pipeline before the SOAP version is detected (e.g., Content-Type parsing failure). If `fault_response()` always returns a SOAP 1.2 envelope, a SOAP 1.1 client receives a 1.2-structured fault.

**Why it happens:** The SOAP version is detected in step 1 of `soap_post_handler`. Faults that occur before step 1 (bad Content-Type) have no version context. Faults after step 1 have version context but `fault_response()` ignores it.

**How to avoid:**
- For pre-version-detection faults (Content-Type failure): return a plain 400 with no SOAP body, or return a SOAP 1.2 fault (acceptable per spec since we cannot determine version).
- For post-version-detection faults: pass `SoapVersion` to `fault_response()` and call `fault.to_xml_bytes_versioned(&version)`.
- Signature change: `fn fault_response(fault: SoapFault, version: SoapVersion) -> Response`.

**Warning signs:** SOAP 1.1 client receives `<env:Fault>` instead of `<SOAP-ENV:Fault>`.

### Pitfall 2: SOAP 1.1 faultcode Must Use Envelope Namespace Prefix

**What goes wrong:** Serializing `<faultcode>Client</faultcode>` instead of `<faultcode>SOAP-ENV:Client</faultcode>`. The SOAP 1.1 spec states the faultcode value MUST be a qualified name — just `Client` is invalid.

**Why it happens:** The W3C spec says "the faultcode value MUST be a qualified name as defined in XML Namespaces spec." The prefix used must resolve to the SOAP 1.1 envelope namespace. Implementations that omit the prefix produce technically invalid SOAP.

**How to avoid:** Always emit `SOAP-ENV:Client` (not bare `Client`) and declare `xmlns:SOAP-ENV="http://schemas.xmlsoap.org/soap/envelope/"` on the Envelope element.

**Warning signs:** Strict clients (SoapUI validator mode) reject the fault; WCF clients log "invalid fault code format."

### Pitfall 3: RPC dispatch_table Missing Entry When soap:body Has No namespace Attribute

**What goes wrong:** Some RPC WSDLs omit `namespace` on `soap:body`. The synthesized dispatch QName gets an empty namespace string. The body wrapper element arrives with a real namespace but the empty-namespace QName misses.

**Why it happens:** `SoapBody.namespace` is `Option<String>`. When `None`, using an empty string for dispatch QName creates a local-name-only entry that won't match namespace-qualified wrappers.

**How to avoid:** When `soap:body.namespace` is absent for RPC, fall back to the WSDL `targetNamespace`. Log a warning at startup if namespace is missing from an RPC body binding.

**Warning signs:** RPC operations never dispatch; fallback to `by_action` always triggered.

### Pitfall 4: Multi-Service Build Fails When Service Names Collide

**What goes wrong:** Two services in the same WSDL define operations with the same name. Building a merged dispatch table causes the second to overwrite the first silently.

**Why it happens:** Current `build_dispatch_table()` deduplicates by operation name across all services. With multiple services this is wrong — each service needs its own isolated table.

**How to avoid:** Build one `DispatchTable` per service. `build_dispatch_table_for_service(service_name, ...)` scopes the operation lookup to that service's ports/bindings only.

**Warning signs:** Operation "GetCapabilities" from ServiceA is routed to ServiceB's handler.

### Pitfall 5: server.rs fault_response Content-Type Hard-Coded to SOAP 1.2

**What goes wrong:** `fault_response()` in `server.rs` currently has `("Content-Type", "application/soap+xml; charset=utf-8")` hard-coded. SOAP 1.1 clients must receive `text/xml; charset=utf-8` on fault responses.

**Why it happens:** Phase 1 only needed SOAP 1.2 fault responses. The content-type was not parameterized.

**How to avoid:** Change `fault_response(fault: SoapFault)` to `fault_response(fault: SoapFault, version: SoapVersion)` and call `response_content_type(&version)`. For pre-detection faults, pass `SoapVersion::Soap12` as default.

---

## Code Examples

### SOAP 1.1 Fault Wire Format
```xml
<!-- Source: W3C SOAP 1.1 spec https://www.w3.org/TR/2000/NOTE-SOAP-20000508/ Section 4.4 -->
<SOAP-ENV:Envelope xmlns:SOAP-ENV="http://schemas.xmlsoap.org/soap/envelope/">
  <SOAP-ENV:Body>
    <SOAP-ENV:Fault>
      <faultcode>SOAP-ENV:Client</faultcode>
      <faultstring>Failed to locate method</faultstring>
      <!-- faultactor is optional; omit when this node is ultimate destination -->
      <!-- detail is optional; include only for Body processing failures -->
    </SOAP-ENV:Fault>
  </SOAP-ENV:Body>
</SOAP-ENV:Envelope>
```

Note: `faultcode`, `faultstring`, `faultactor`, `detail` are **not** namespace-qualified — they are unqualified element names under the `SOAP-ENV:Fault` element. Only the `faultcode` *value* is a qualified name like `SOAP-ENV:Client`.

### RPC Dispatch Wrapper Element
```xml
<!-- Source: SOAP 1.1 spec Section 7 RPC representation -->
<!-- soap:body namespace="http://example.com/svc" in WSDL -->
<m:GetLastTradePrice xmlns:m="http://example.com/svc">
  <symbol>DIS</symbol>
</m:GetLastTradePrice>
```
Dispatch key is QName `{http://example.com/svc}GetLastTradePrice` — derived from (operation name, soap:body namespace).

### WSDL Multi-Service Structure
```xml
<!-- Each service has a separate port address — natural per-path routing -->
<wsdl:service name="DeviceService">
  <wsdl:port name="DeviceServicePort" binding="tns:DeviceServiceBinding">
    <soap:address location="http://host/onvif/device_service"/>
  </wsdl:port>
</wsdl:service>
<wsdl:service name="MediaService">
  <wsdl:port name="MediaServicePort" binding="tns:MediaServiceBinding">
    <soap:address location="http://host/onvif/media_service"/>
  </wsdl:port>
</wsdl:service>
```
Each service mounts at its port address path. `ServerBuilder` extracts the path component from each `soap:address location` and registers one axum route per service.

---

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| RPC/encoded (SOAP Section 5) | Document/literal (WS-I Basic Profile) | ~2003-2004 | RPC/encoded is legacy; most new SOAP services use document/literal; implement RPC dispatch-only |
| Separate fault types per SOAP version | Shared FaultCode enum + versioned serializer | Phase 2 design | Callers create faults without knowing target version; serializer handles mapping |

**Deprecated/outdated:**
- Full SOAP Section 5 encoding (multi-ref, array types): Still encountered in legacy Java/.NET services from pre-2005 era. Not needed for server-side dispatch; only matters if implementing a typed handler API (v2 scope).
- RPC/encoded is not WS-I Basic Profile 1.1 compliant — new services do not use it, but legacy services in the wild still do.

---

## Open Questions

1. **`DataEncodingUnknown` in SOAP 1.1 context**
   - What we know: `DataEncodingUnknown` is a SOAP 1.2 fault code that has no direct equivalent in SOAP 1.1.
   - What's unclear: The W3C SOAP 1.1 spec does not define this code. Apache CXF maps it to `SOAP-ENV:Server`.
   - Recommendation: Map `FaultCode::DataEncodingUnknown` to `SOAP-ENV:Server` in the 1.1 serializer. Document this in code comments.

2. **ServerBuilder API for multi-service**
   - What we know: Each service should map to a path. The `ServerBuilder` currently takes one handler map.
   - What's unclear: Whether the API should use `.service("ServiceName", handlers)` chaining or a map-based approach.
   - Recommendation: Add `.service(service_name, handlers)` method to `ServerBuilder`. If never called, behavior falls back to existing single-service mode for backward compatibility.

3. **Fault HTTP status for SOAP 1.1**
   - What we know: SOAP 1.1 spec does not mandate HTTP 500 for faults. SOAP 1.2 spec (Section 7.4.2) mandates HTTP 500. Common implementations return 500 for both.
   - What's unclear: Whether to differentiate per-version or always use 500.
   - Recommendation: Return HTTP 500 for both versions. This is what python-zeep, node-soap, and Apache CXF do. Document the choice in FLT-04 test comments.

---

## Validation Architecture

### Test Framework
| Property | Value |
|----------|-------|
| Framework | cargo test (built-in) |
| Config file | none — standard `cargo test` |
| Quick run command | `cargo test` |
| Full suite command | `cargo test` |

### Phase Requirements → Test Map

| Req ID | Behavior | Test Type | Automated Command | File Exists? |
|--------|----------|-----------|-------------------|-------------|
| ENV-05 | Parse SOAP 1.1 envelope with correct namespace and SoapVersion::Soap11 result | unit | `cargo test parse_envelope_soap11` | ❌ Wave 0 — add to `src/envelope.rs` tests |
| ENV-06 | Serialize SOAP 1.1 response envelope with correct namespace | unit | `cargo test serialize_envelope_soap11` | ❌ Wave 0 — add to `src/envelope.rs` tests |
| ENV-06 | Full pipeline: SOAP 1.1 request returns SOAP 1.1 response with text/xml Content-Type | integration | `cargo test soap11_end_to_end` | ❌ Wave 0 — add to `tests/integration_test.rs` |
| FLT-04 | SOAP 1.1 fault XML contains faultcode/faultstring/faultactor/detail structure | unit | `cargo test fault_soap11_structure` | ❌ Wave 0 — add to `src/fault.rs` tests |
| FLT-04 | SOAP 1.1 fault uses SOAP-ENV namespace prefix | unit | `cargo test fault_soap11_namespace` | ❌ Wave 0 — add to `src/fault.rs` tests |
| FLT-05 | FaultCode::Sender serializes as SOAP-ENV:Client in 1.1 | unit | `cargo test fault_code_sender_maps_to_client` | ❌ Wave 0 — add to `src/fault.rs` tests |
| FLT-05 | FaultCode::Receiver serializes as SOAP-ENV:Server in 1.1 | unit | `cargo test fault_code_receiver_maps_to_server` | ❌ Wave 0 — add to `src/fault.rs` tests |
| DSP-05 | RPC/encoded operation dispatches by wrapper element QName | unit | `cargo test rpc_dispatch_by_wrapper_element` | ❌ Wave 0 — add to `src/dispatch.rs` tests |
| DSP-05 | RPC binding dispatch table built from soap:body namespace + op name | unit | `cargo test build_dispatch_table_rpc_binding` | ❌ Wave 0 — add to `src/dispatch.rs` tests |
| DSP-06 | Multiple services build separate dispatch tables | unit | `cargo test multi_service_dispatch_tables` | ❌ Wave 0 — add to `src/dispatch.rs` tests |
| DSP-06 | Request to service A path routes to service A handler, not service B | integration | `cargo test multi_service_routing` | ❌ Wave 0 — add to `tests/integration_test.rs` |

### Sampling Rate
- **Per task commit:** `cargo test`
- **Per wave merge:** `cargo test`
- **Phase gate:** Full suite green before `/gsd:verify-work`

### Wave 0 Gaps
- [ ] `src/envelope.rs` — add `parse_envelope_soap11_*` and `serialize_envelope_soap11_*` test cases
- [ ] `src/fault.rs` — add `fault_soap11_*` and `fault_code_*_maps_to_*` test cases
- [ ] `src/dispatch.rs` — add `rpc_dispatch_*` and `multi_service_*` test cases
- [ ] `tests/integration_test.rs` — add `soap11_end_to_end` and `multi_service_routing` integration tests

---

## Sources

### Primary (HIGH confidence)
- W3C SOAP 1.1 spec https://www.w3.org/TR/2000/NOTE-SOAP-20000508/ — envelope namespace, fault structure, RPC body representation
- Existing `src/envelope.rs` (read directly) — confirms parse_envelope() already handles SOAP 1.1 namespace
- Existing `src/fault.rs` (read directly) — confirms FaultCode enum uses Sender/Receiver as canonical names
- Existing `src/dispatch.rs` (read directly) — confirms DispatchTable is currently single flat table; `SoapBody.namespace` field exists
- Existing `src/wsdl/definitions.rs` (read directly) — confirms `BindingStyle::Rpc`, `UseStyle::Encoded`, `SoapBody.namespace: Option<String>` all present
- https://www.herongyang.com/WSDL/WSDL-11-Extension-SOAP-12-body-Binding-SOAP-Body.html — soap:body namespace attribute for RPC wrapper element

### Secondary (MEDIUM confidence)
- node-soap server.ts GitHub source review — confirmed single-path multi-service routing model where port address is used to identify service
- python-zeep soap.py GitHub source review — confirmed Soap11Binding and Soap12Binding as separate classes, SOAP 1.1 fault extraction pattern
- tutorialspoint SOAP fault example — confirmed SOAP-ENV prefix usage on faultcode value with unqualified faultcode/faultstring element names
- johnderinger.wordpress.com WSDL binding styles — confirmed RPC wrapper element structure and dispatch key pattern

### Tertiary (LOW confidence)
- WebSearch WSDL multi-service routing — community forum posts (coderanch) confirming per-port-address approach

---

## Metadata

**Confidence breakdown:**
- SOAP 1.1 envelope (ENV-05, ENV-06): HIGH — spec read directly, existing code confirmed to already handle it
- SOAP 1.1 fault format (FLT-04): HIGH — W3C spec plus tutorialspoint example, cross-confirmed
- Fault code mapping (FLT-05): HIGH — spec names confirmed, mapping placement in serializer is clear
- RPC dispatch (DSP-05): MEDIUM-HIGH — RPC body wrapper element pattern confirmed from SOAP 1.1 spec + WSDL binding docs; Section 5 encoding scope decision (dispatch-only) is recommendation
- Multi-service routing (DSP-06): MEDIUM — node-soap model confirmed from source, but the exact ServerBuilder API shape is discretionary

**Research date:** 2026-04-03
**Valid until:** 2026-10-01 (SOAP 1.1 spec is stable; WSDL 1.1 spec is stable; no expiry concern)
