# crossref (soap-server)

Differential conformance & interop harness. See the design spec:
`../docs/superpowers/specs/2026-06-02-crossref-harness-design.md`.

**Phase 1 (1a + 1b + 1c) is complete.** All 25 seed scenarios are `verified`:
23 conformance + 2 interop. Zero unverified, zero SutFail, zero HarnessError (spec §11.5).
Phase 2 (onvif-server) is the next spec→plan→build.

---

## Phase 1a — Rust Layer-1 (regression baseline)

- Replays `scenarios/*.toml` against a controlled in-process SUT.
- Normalizes (parse → path-scoped mask → deterministic serialize) and diffs against
  golden `snapshots/*.xml`.
- Snapshots begin as **`unverified`** (self-captured regression baselines). Conformance
  correctness arrives in Phase 1b/1c when the Java XML oracle + CXF + interop clients
  promote them to `verified`.

### Run Layer 1

- All tests (replay + wssec): `cargo test -p crossref`
- Diff against frozen snapshots: `cargo test -p crossref --test layer1_replay`
- (Re)capture snapshots: `CROSSREF_REGEN=1 cargo test -p crossref --test layer1_replay`

Snapshot changes are reviewed like any golden file.

---

## Phase 1b + 1c — Layer 2 (Docker conformance + interop)

Layer 2 validates our SOAP responses with independent external authority: a containerised
Java XML oracle (JAXP/Xerces schema validation + Apache Santuario exclusive C14N), an
Apache CXF reference server, and real third-party client containers (Phase 1c interop).
All authorities are fully containerised — **the host needs only Docker and the Rust
toolchain**. No Java, no Python, no CXF on the host.

### Prerequisites

- Docker (with `docker compose`)
- Rust toolchain (matching `rust-toolchain.toml` or `1.88`)

### Run Layer 2 locally (conformance + interop)

```sh
cargo run -p crossref --bin layer2 -- --promote --interop
```

This command:
1. Brings the compose topology up (`controlled-server`, `oracle`, `cxf`) and builds images.
2. Runs all 23 in-scope conformance scenarios against our server and the CXF reference
   server, validating each response via the oracle. SOAP 1.1, WS-Security (outcome-
   equivalence), WSDL-rewrite, and the raw-XML fault-detail scenario are included.
3. Runs the 2 interop scenarios: `cxf-client` and `zeep-client` containers drive our
   server directly, asserting their operations succeed.
4. On a `Pass` verdict, promotes the snapshot from `unverified` to `verified` and writes
   the oracle-canonical bytes to `snapshots/canonical/<name>.c14n`.
5. Tears the topology down when done (unless `--keep-up` is passed).
6. Prints a per-scenario verdict table split into Conformance and Interop sections, plus
   the still-`unverified` count (0 after Phase 1).

Use `--keep-up` to leave the compose topology running for manual inspection.
Use `--scenarios <csv>` to run a subset of conformance scenarios.

### What "verified" means

**Conformance scenarios** are `verified` when our server's SOAP response is schema-valid
and the oracle-canonical form agrees with the CXF reference server's response (or, for
WS-Security, outcome-equivalence holds; for WSDL-rewrite, oracle WSDL-schema validity +
the rewrite invariant hold).

**Interop scenarios** are `verified` when a real third-party client successfully drives
our server and completes its operations (exit 0). The client's captured response is stored
as canonical evidence.

Oracle-canonical evidence is stored in `snapshots/canonical/` as immutable artifacts.
Status is recorded in `snapshots/status.toml`.

### Comparator registry

See `manifest.toml` for the full comparator registry: base image digests (all pinned to
multi-arch index digest), versions (Santuario 4.0.3, CXF 4.0.5, JDK 21, Python 3.12),
and which scenarios each comparator participates in.

| Comparator | Role | Scenarios |
|---|---|---|
| `java-xml-oracle` | schema-oracle | all (validates/canonicalizes every conformance scenario) |
| `cxf` | reference-server | SOAP 1.2, SOAP 1.1, WS-Security conformance |
| `controlled-server` | sut | all |
| `cxf-client` | interop-client | `interop_cxf_echo` |
| `zeep-client` | interop-client | `interop_zeep_echo` |

### CI

The Layer 2 workflow (`.github/workflows/layer2.yml`) runs conformance + interop nightly
(UTC 13:00) and on `workflow_dispatch`. It does **not** run on push — the per-commit gate
(`cargo test`) is Layer 1 only (fast, no Docker). Promotion changes (newly `verified`
snapshots) surfaced by CI are committed deliberately in a follow-up PR, never
auto-committed.
