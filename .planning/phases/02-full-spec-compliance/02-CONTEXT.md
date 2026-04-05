# Phase 2: Full Spec Compliance - Context

**Gathered:** 2026-04-05
**Status:** Ready for planning

<domain>
## Phase Boundary

Extend the existing SOAP 1.2 server to handle SOAP 1.1 envelopes with correct fault format, dispatch RPC/encoded bindings, and route multi-service WSDLs. This is additive — Phase 1's SOAP 1.2 pipeline remains unchanged, Phase 2 adds the SOAP 1.1 code path and broader dispatch capabilities.

</domain>

<decisions>
## Implementation Decisions

### Porting approach
- Same as Phase 1: match the spec and follow the best available prior art
- DESIGN.md remains the authoritative architecture specification
- Extend existing modules (envelope.rs, fault.rs, dispatch.rs) rather than creating new ones

### Claude's Discretion
All gray areas delegated to research:

- **RPC/encoded depth:** How far to implement RPC/encoded binding — minimal dispatch-only, or full SOAP Section 5 encoding rules (multi-ref, array types). Research should check what python-zeep, node-soap, and Apache CXF actually support and what real-world RPC/encoded WSDLs look like in practice.

- **Multi-service routing model:** Whether multiple services share one path and dispatch by QName/SOAPAction, or each service gets its own URL path prefix. Research should check how node-soap and Spring-WS handle multi-service WSDLs and what the WSDL 1.1 spec says about port addresses.

- **SOAP 1.1 fault mapping:** How to implement the bidirectional fault code mapping (Sender↔Client, Receiver↔Server) cleanly in the existing FaultCode enum. Research should determine if this belongs in serialization or in the type system.

</decisions>

<code_context>
## Existing Code Insights

### Reusable Assets
- `src/envelope.rs`: Already has `detect_soap_version()` returning Soap11/Soap12 from Content-Type. SOAP 1.1 parsing/serialization adds a parallel code path.
- `src/fault.rs`: SoapFault struct and FaultCode enum exist. SOAP 1.1 fault serialization adds a second format alongside the existing 1.2 serializer.
- `src/dispatch.rs`: DispatchTable with by_element and by_action HashMaps. Multi-service adds per-service tables. RPC/encoded adds a new dispatch path.
- `src/wsdl/definitions.rs`: SoapBinding already tracks BindingStyle (Document/RPC) and SoapVersion (Soap11/Soap12).
- `src/server.rs`: SoapService request pipeline already branches on SoapVersion for Content-Type.

### Established Patterns
- Version detection at envelope level drives all downstream behavior (response format, fault structure, Content-Type)
- Two-pass parse+resolve for WSDL/XSD
- TDD with cargo test per module
- Deviation auto-fix when plan assumptions don't match runtime reality

### Integration Points
- `SoapVersion` enum already in envelope.rs — 1.1 paths branch on this
- `FaultCode` enum already has Sender/Receiver — needs Client/Server aliases for 1.1
- `build_dispatch_table()` currently iterates all services — multi-service needs per-service tables

</code_context>

<specifics>
## Specific Ideas

No specific requirements — open to standard approaches. The spec and prior art guide implementation.

</specifics>

<deferred>
## Deferred Ideas

None — discussion stayed within phase scope

</deferred>

---

*Phase: 02-full-spec-compliance*
*Context gathered: 2026-04-05*
