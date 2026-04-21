# soap-server Roadmap

## Known Limitations and Deferred Items for v0.2+

These are intentional deviations or gaps identified during the v1.0 internal milestone assessment.

### Not Implemented

- [ ] **typed_handler support** — Spec described closures receiving typed structs via de/serialization (`server.typed_handler::<Req, Res>(...)`). Handlers currently take and return raw `Bytes`. Not blocking for onvif-server (which builds XML by hand), but would improve ergonomics for general-purpose SOAP server use.
- [ ] **SoapFault node/role fields** — Spec included `node: Option<String>` and `role: Option<String>` on `SoapFault`. Current implementation omits these. Low priority — rarely used in practice.
- [ ] **MTOM/XOP support** — Listed as Phase 1b in spec. Not implemented. Not needed for ONVIF.
- [ ] **examples/ directory** — Spec described a `simple_service.rs` example. Not created. Low priority given onvif-server's `virtual_ptz.rs` serves as the primary usage example.

### Implementation Notes

- WSDL address rewriting uses string replacement on raw WSDL bytes rather than re-serializing the parsed model. Pragmatic but could be fragile with unusual WSDL formatting.
- `extract_local_name` helper is duplicated in onvif-server's service handlers — could be promoted to a public utility in soap-server if other consumers need it.
