# Conformance

`soap-server`'s SOAP output is validated against independent authorities, not just
self-checked. This page summarises what that means and how to reproduce it.

## What was validated

A differential conformance and interop harness (`crossref/`, a non-published
workspace member) exercises **25 seed scenarios** — 23 conformance + 2 interop — to
`verified` status. It runs in two layers:

- **Layer 1 — in-process replay.** Scenarios run against a controlled in-process
  service; each response is normalised (parse → path-scoped mask → deterministic
  serialize) and diffed against a golden snapshot, so behaviour changes show up as
  reviewable snapshot diffs.
- **Layer 2 — external authority (Docker).** Responses are validated by independent,
  fully containerised authorities: a Java XML oracle (JAXP / Xerces schema
  validation + Apache Santuario exclusive C14N), an **Apache CXF** reference server,
  and real third-party SOAP client containers for interop. The host needs only
  Docker and the Rust toolchain — no Java, Python, or CXF installed locally.

## What a pass means — and doesn't

A pass means the enveloping, faults, WS-Security handling, and dispatch produce
SOAP that is schema-valid and interoperable with mainstream SOAP stacks (CXF, zeep)
for the exercised scenarios. It does **not** certify every WSDL construct or the
features listed as unsupported in [Capabilities & Limitations](./capabilities.md).

## Reproducing it

```sh
# Layer 1 (no Docker): replay against frozen snapshots
cargo test -p crossref --test layer1_replay

# Layer 2 (Docker): conformance + interop against external authorities
cargo run -p crossref --bin layer2 -- --promote --interop
```

Full details, the scenario contract, and the authority setup are in the harness
README: <https://github.com/NavistAu/soap-server/blob/main/crossref/README.md>.
