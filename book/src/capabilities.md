# Capabilities & Limitations

What `soap-server` does, what it deliberately does not, and where the edges are.
Read this before assuming a WSDL will "just work" — the crate is a faithful SOAP
*transport and dispatch* layer, not a code generator or a full XSD validator.

## What it does

- **SOAP 1.1 and 1.2.** The version is auto-detected from the `Content-Type`
  header (`text/xml` → 1.1, `application/soap+xml` → 1.2) and the envelope
  namespace; the response is emitted in the same version as the request. Faults
  are mapped to the correct per-version codes (see [Architecture](./architecture.md)).
- **WSDL-driven dispatch.** Operations are read from the WSDL at `.build()` time.
  Requests are routed by the body's first-child element `QName`, with the
  `SOAPAction` header as a fallback key. Registering a handler for an operation
  name not present in the WSDL is a **build-time error**, not a runtime panic.
- **Document and RPC bindings.** Both `style="document"` and `style="rpc"` SOAP
  bindings are parsed and dispatched. For RPC, the wrapper element namespace from
  the binding is used for routing/validation.
- **WS-Security UsernameToken.** `PasswordDigest` and `PasswordText`, with nonce
  replay detection and timestamp freshness checks. Per-operation bypass via
  `.auth_bypass([...])`. See [WS-Security](./ws-security.md).
- **WSDL/XSD import & include resolution.** `.build()` resolves imported and
  included schema documents through a `WsdlLoader` (file-based by default, or your
  own loader for embedded/remote schemas).
- **Multi-service composition.** Each `SoapService` becomes an `axum::Router`
  mounted at the path from its WSDL `<service><port address>`; merge several into
  one app.
- **Raw-bytes handlers.** Your handler receives the body element as self-contained
  XML bytes (ancestor namespaces re-emitted on the fragment root) and returns
  response XML bytes. You own parsing and serialisation.

## What "XSD structural validation" means here

Before your handler runs, the request body is checked against the operation's
input type — but only shallowly:

- It verifies that **every top-level child element with `minOccurs > 0` is
  present** (by local name, one level deep), across `sequence`, `all`, and through
  `extension`/`restriction`.
- For `xs:choice`, individual branches are **not** independently required (a valid
  request supplies one branch), so a choice contributes no required names.

It explicitly does **not**:

- validate datatypes, formats, or value facets (`pattern`, `enumeration`,
  `minInclusive`, `length`, …);
- validate nested/recursive structure below the first level;
- enforce `maxOccurs`, element ordering, or attribute presence;
- reject unknown/extra elements.

In short: it catches "you forgot a required field," not "this field is malformed."
Treat handler-side parsing as the real validation boundary.

## Limitations / not supported

- **No typed handlers or code generation.** There is no proc-macro and no
  WSDL→Rust struct generation. You work with XML bytes, not generated types.
- **No SOAP encoding (`use="encoded"`, section 5).** RPC/encoded bindings dispatch,
  but encoded value graphs are not decoded — the body bytes are handed to you as-is.
- **No MTOM / XOP attachments**, no SwA. Request/response are single XML parts.
- **No WS-Addressing dispatch.** Header fragments are exposed to handlers
  (`handle_with_headers`) so you can implement addressing yourself, but routing is
  by body element / SOAPAction only.
- **No automatic response validation.** Bytes you return are wrapped in an envelope
  and sent verbatim; the crate does not check them against the WSDL.
- **WS-Security is UsernameToken only** — no X.509, SAML, signing, or encryption.

## Operational notes

- Built on [axum](https://docs.rs/axum)/[Tokio](https://tokio.rs); you provide the
  runtime and bind the listener.
- Nonce replay state lives in a `RotatingNonceCache`. In a multi-process / load-
  balanced deployment the cache is per-process, so replay protection is per-node —
  pin a client to one node or front with sticky sessions if strict replay rejection
  across the fleet matters.
- All five XML special characters are escaped via `escape_text`/`escape_attr`; use
  them when composing response XML from untrusted data.
