# Phase 1: ONVIF-Level Support - Context

**Gathered:** 2026-04-04
**Status:** Ready for planning

<domain>
## Phase Boundary

Full working SOAP 1.2 server that can load a real ONVIF WSDL, parse its XSD schemas, dispatch requests by body element QName to registered handlers, enforce WS-Security UsernameToken authentication, serve the WSDL on GET ?wsdl, and expose an axum Router for HTTP integration. This is a porting exercise from python-zeep (WSDL/XSD), node-soap (dispatch), and rpos (WS-Security), targeting spec compliance.

</domain>

<decisions>
## Implementation Decisions

### Porting approach
- This is a porting exercise — match the spec and follow the best available prior art
- python-zeep is the primary source for WSDL/XSD parsing logic (two-pass pattern)
- node-soap for dispatch pattern (body element QName → handler)
- rpos for WS-Security UsernameToken digest (cleanest 50-line implementation)
- globusdigital/soap (Go) for minimal dispatch pattern reference
- DESIGN.md is the authoritative specification for this crate's architecture

### Claude's Discretion
All four identified gray areas are delegated to research:

- **Handler boundary / namespace context:** How to handle namespace inheritance loss when body bytes are extracted for the raw handler. Options include re-emitting namespace declarations on the fragment root or passing a (bytes, namespace_map) tuple. Research should investigate how node-soap and gSOAP handle this.

- **Auth model:** Whether the auth layer takes a single static credential or a lookup function for multi-user support. Research should check how ONVIF devices actually present credentials and what onvif-server will need.

- **WSDL loading API:** Whether to support from_file() only or also from_bytes/from_str for embedded WSDLs. Research should check what onvif-server's actual loading pattern will be.

- **Test fixtures:** Which ONVIF WSDLs to use, whether to bundle in repo or download at test time. Research should identify the minimal set of WSDLs that exercise all XSD/WSDL features.

</decisions>

<code_context>
## Existing Code Insights

### Reusable Assets
- None — greenfield project, no existing code

### Established Patterns
- None yet — patterns will be established in this phase

### Integration Points
- DESIGN.md (`docs/DESIGN.md`) is the authoritative architecture specification
- Research outputs in `.planning/research/` (STACK.md, FEATURES.md, ARCHITECTURE.md, PITFALLS.md, SUMMARY.md)
- First consumer will be onvif-server crate (separate repo)

</code_context>

<specifics>
## Specific Ideas

- Crate structure follows DESIGN.md: lib.rs, server.rs, dispatch.rs, envelope.rs, fault.rs, security.rs, wsdl/, xsd/
- Dependencies locked: roxmltree (startup DOM), quick-xml (per-request streaming), axum + tokio, sha1 + base64 + chrono
- License: MIT OR Apache-2.0

</specifics>

<deferred>
## Deferred Ideas

None — discussion stayed within phase scope

</deferred>

---

*Phase: 01-onvif-level-support*
*Context gathered: 2026-04-04*
