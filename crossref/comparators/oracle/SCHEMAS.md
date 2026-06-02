# Vendored Schemas

All XSDs are static, public W3C/OASIS artifacts vendored here for offline use.
None are fetched at container runtime.

| File | Source URL | Notes |
|------|-----------|-------|
| `controlled.xsd` | Extracted from `crossref/fixtures/controlled.wsdl` `<wsdl:types>` | targetNamespace `http://crossref.example/controlled`; contains Echo, EchoResponse, EchoNamed (named type EchoNamedInputType), EchoNamedResponse |
| `soap12-envelope.xsd` | http://www.w3.org/2003/05/soap-envelope/ | SOAP 1.2 envelope schema; `schemaLocation` for xml namespace import changed to local `xml.xsd` |
| `xml.xsd` | http://www.w3.org/2001/xml.xsd | W3C XML namespace attributes schema; imported by soap12-envelope.xsd |
| `soap11-envelope.xsd` | http://schemas.xmlsoap.org/soap/envelope/ | SOAP 1.1 envelope schema; used by deferred Phase 1c scenarios; registered but in-scope scenarios are all SOAP 1.2 |
