# crossref (soap-server)

Differential conformance & interop harness. See the design spec:
`../docs/superpowers/specs/2026-06-02-crossref-harness-design.md`.

## Phase 1a — Rust Layer-1 (regression baseline)

- Replays `scenarios/*.toml` against a controlled in-process SUT.
- Normalizes (parse → path-scoped mask → deterministic serialize) and diffs against
  golden `snapshots/*.xml`.
- Snapshots begin as **`unverified`** (self-captured regression baselines). Conformance
  correctness arrives in Phase 1b when the Java XML oracle + CXF promote them to
  `verified`.

### Run Layer 1

- All tests (replay + wssec): `cargo test -p crossref`
- Diff against frozen snapshots: `cargo test -p crossref --test layer1_replay`
- (Re)capture snapshots: `CROSSREF_REGEN=1 cargo test -p crossref --test layer1_replay`

Snapshot changes are reviewed like any golden file.

---

## Phase 1b — Layer 2 (Docker conformance)

Layer 2 validates our SOAP responses with independent external authority: a containerised
Java XML oracle (JAXP/Xerces schema validation + Apache Santuario exclusive C14N) and an
Apache CXF reference server. Both authorities are fully containerised — **the host needs
only Docker and the Rust toolchain**. No Java, no CXF, no Xerces on the host.

### Prerequisites

- Docker (with `docker compose`)
- Rust toolchain (matching `rust-toolchain.toml` or `1.88`)

### Run Layer 2 locally

```sh
cargo run -p crossref --bin layer2 -- --promote
```

This command:
1. Brings the compose topology up (`controlled-server`, `oracle`, `cxf`) and builds images.
2. Replays all 12 in-scope conformance scenarios against both our server and the CXF
   reference server, validating each response via the oracle.
3. On a `Pass` verdict, promotes the snapshot from `unverified` to `verified` and writes
   the oracle-canonical bytes to `snapshots/canonical/<name>.c14n`.
4. Tears the topology down when done (unless `--keep-up` is passed).
5. Prints a per-scenario verdict table and the count of still-`unverified` scenarios.

Use `--keep-up` to leave the compose topology running for manual inspection.

### What "verified" means

A snapshot is `verified` when:
- Our server's SOAP response is **schema-valid** against the SOAP 1.2 envelope schema
  (checked by the Java XML oracle using JAXP/Xerces).
- The **body payload** is schema-valid against the controlled XSD.
- The oracle-canonical form (exclusive C14N, Santuario) of our response **agrees** with
  the oracle-canonical form of the CXF reference server's response for the same request.

Oracle-canonical evidence is stored in `snapshots/canonical/` as immutable conformance
artifacts. Status is recorded in `snapshots/status.toml`.

### Comparator manifest

See `manifest.toml` for the full comparator registry: base image digests, versions
(Santuario 4.0.3, CXF 4.0.5, JDK 21), and which scenarios each comparator participates in.

### Deferred to Phase 1c

WS-Security conformance (CXF WSS4J), multi-service conformance, SOAP 1.1 framing, and
all interop tests (CXF/Zeep clients driving our server) are **out of scope for Phase 1b**.
These scenarios remain `unverified`; the Layer 2 report surfaces their count. A Phase 1c
plan covers them.

### CI

The Layer 2 conformance workflow (`.github/workflows/layer2.yml`) runs nightly (UTC 13:00)
and on `workflow_dispatch`. It does **not** run on push — the per-commit gate (`cargo test`)
is Layer 1 only (fast, no Docker). Promotion changes (newly `verified` snapshots) surfaced
by CI are committed deliberately in a follow-up PR, never auto-committed.
