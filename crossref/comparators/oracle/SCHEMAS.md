# Vendored Schemas

All XSDs are static, public W3C/OASIS artifacts vendored here for offline use.
None are fetched at container runtime.

| File | Source URL | Notes |
|------|-----------|-------|
| `controlled.xsd` | Extracted from `crossref/fixtures/controlled.wsdl` `<wsdl:types>` | targetNamespace `http://crossref.example/controlled`; contains Echo, EchoResponse, EchoNamed (named type EchoNamedInputType), EchoNamedResponse |
| `soap12-envelope.xsd` | http://www.w3.org/2003/05/soap-envelope/ | SOAP 1.2 envelope schema; `schemaLocation` for xml namespace import changed to local `xml.xsd` |
| `xml.xsd` | http://www.w3.org/2001/xml.xsd | W3C XML namespace attributes schema; imported by soap12-envelope.xsd; DOCTYPE removed (Xerces parameter-entity incompatibility) |
| `soap11-envelope.xsd` | http://schemas.xmlsoap.org/soap/envelope/ | Real SOAP 1.1 envelope schema (replaces Phase 1b placeholder); no DOCTYPE present in source; registered as schema id `soap11-envelope` |
| `wsdl11.xsd` | http://schemas.xmlsoap.org/wsdl/ | WSDL 1.1 schema; extensibility elements use `processContents="lax"` so SOAP binding sub-schemas are not required for validation; registered as schema id `wsdl11` |
