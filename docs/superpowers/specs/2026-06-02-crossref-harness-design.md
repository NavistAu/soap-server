# crossref — differential conformance & interop harness (design)

**Date:** 2026-06-02
**Status:** Approved design (v1). Scope of this spec: **Phase 1** — the `crossref`
framework + the **soap-server** suite. Phase 2 (onvif-server) and Phase 3 are
documented as follow-ons and get their own spec→plan→build cycles.
**Working name:** `crossref` (renameable).

---

## 1. Purpose & motivation

`soap-server` and `onvif-server` implement a well-understood spec (SOAP 1.1/1.2,
WSDL/XSD, WS-Security, WS-Addressing, WS-Discovery, ONVIF), but their correctness
currently rests on the *implementer's* judgment rather than the project owner's
deep knowledge of the protocols. `crossref` exists to **anchor correctness to
independent, authoritative external tools** — Apache CXF (Java/Xerces), Zeep,
gSOAP, and the ONVIF schemas — instead of to our own assertions.

**Governing principle:** our own server's output is a *regression baseline only*.
The correctness signal is **agreement with the reference tools + schema-validity**,
never our own say-so. Anything that interprets the spec (XSD validation, XML
canonicalization) MUST be performed by trusted external tooling, never by our own
Rust code — otherwise the harness would grade the system under test with the
system under test.

## 2. Goals / non-goals

**Goals**
- Differentially verify our servers against established, widely-used, respected
  reference implementations.
- Two verification categories: **conformance** (our server vs reference servers)
  and **interop** (real third-party clients driving our server).
- A fast offline dev/CI loop plus a heavyweight live-systems verification loop.
- Pluggable comparators (multiple per scenario over time), pinned to current
  stable versions, declared in a manifest.
- Keep the published library crates pristine — the harness must not affect
  `cargo publish`.

**Non-goals (v1)**
- Multi-version comparator matrices (future; the manifest is designed to grow
  into it).
- Automating the official ONVIF Device Test Tool (Windows-only GUI; remains a
  manual pre-release gate).
- Byte-identical response equality against reference *devices* (legitimately
  impossible for ONVIF — see §9 caveats).

## 3. Placement & packaging

Each crate carries its **own** `crossref` harness in-repo — the same design and
name in both, but no shared dependency and no separate repository. The harness is
a **Cargo workspace member with `publish = false`**, so the published library
crate and `cargo publish` are unaffected. The spec-sensitive grading and the
heavyweight comparator tooling live in containers, not in the published crate.

## 4. Architecture

### 4.1 Two categories (both crates implement both)

- **conformance** — for a given scenario request, send the *identical* request to
  *our server* and to each *reference server*; capture both responses; normalize;
  diff.
- **interop** — a real third-party *client* drives *our server* through an
  operation sequence; capture the `(client-request, our-response)` trace; the live
  run asserts the client's operations actually succeed, and the captured requests
  feed the offline replay/diff.

### 4.2 Two execution layers (both apply to both categories)

- **Layer 1 — fast offline (Rust):** replay captured/scenario requests against our
  server, normalize, and diff against frozen snapshots. No Docker, no network.
  This is the developer inner loop and the per-commit CI gate. It detects
  *regressions* against snapshots that were proven correct (schema-valid +
  reference-agreeing) at generation time by Layer 2.
- **Layer 2 — live Docker (Rust orchestrator):** `docker compose` brings up our
  server plus the comparator containers. The orchestrator drives every scenario,
  captures responses, **delegates C14N + XSD validation to a containerized
  Java/Xerces oracle**, diffs against the references, **regenerates the snapshot
  corpus from the authorities**, runs the live interop clients, and emits a
  report. Runs nightly / on-demand / pre-release.

### 4.3 Languages

- **Harness logic** (both layers: orchestration, volatile-field masking, diffing,
  reporting) = **Rust**. Single-language, cargo-native, and Layer 1 and Layer 2
  share the same normalization/diff code.
- **Authorities run in containers:**
  - **Java** — Apache CXF (SOAP reference server + interop client) and
    **Xerces/JAXP** (the XSD validation + C14N oracle).
  - **Python** — `python-onvif-zeep` (ONVIF interop client).
  - **C/gSOAP** — `onvif-srvd` (ONVIF reference server).
- **Spec-sensitive grading** (XSD validation, canonicalization) is delegated to
  **Xerces** — reference-grade and independent of Rust. The Rust orchestrator
  invokes the validator container; it never validates XML itself.

> Rationale for Rust orchestrator: third-party clients are polyglot and must run
> as their own containers regardless of runner language, so an in-process client
> host buys nothing. With validation/C14N containerized in Xerces, the runner's
> remaining job is orchestration + masking + diff + reporting — best kept in one
> language shared with Layer 1.

## 5. Components

### 5.1 Scenarios (single source of truth)
A declarative `scenarios/` set. Each scenario = operation name + request
body/headers + auth context + expectation (succeed, or a specific fault). Consumed
by **both** layers so Rust and the Docker orchestrator exercise identical cases.

### 5.2 Snapshots (golden corpus)
Per scenario, the *normalized* expected response(s). **Regenerated by Layer 2 from
the authorities** and consumed by Layer 1. Snapshots are golden files: **drift is a
reviewed change** — Layer 2 fails with the diff (and may open a PR); snapshots are
never updated silently.

### 5.3 Normalization
Exclusive XML **C14N** plus a **masking ruleset** for volatile fields (message IDs,
UUIDs, timestamps, nonces, generated tokens), so diffs are stable and meaningful.
The masking/diff comparison is plain Rust (not spec interpretation); the
canonicalization itself is performed by the external validator container (Xerces).

### 5.4 Comparator manifest
A per-repo `manifest.toml` listing each comparator: `name`, `role`
(`reference-server` | `interop-client` | `schema-oracle`), pinned Docker image +
stable version, and the scenarios it participates in. Adding/swapping a comparator
= a manifest entry + a container. Multi-version is a future extension of this same
manifest.

### 5.5 Comparators (current stable subset)
- **soap-server:** CXF (conformance reference server *and* interop client) + Zeep
  (second interop client). Xerces as the schema-oracle.
- **onvif-server (Phase 2):** `onvif-srvd` (conformance reference server) +
  `python-onvif-zeep` (interop client) + ONVIF XSD validation via Xerces.

## 6. Per-repo layout

```
<repo>/crossref/                 # cargo workspace member, publish = false
├── Cargo.toml                   # the crossref member (orchestrator + Layer-1 tests)
├── scenarios/                   # declarative scenario fixtures (shared by both layers)
├── snapshots/                   # golden normalized responses (regenerated by Layer 2)
├── manifest.toml                # comparator registry (name/role/image+version/scenarios)
├── normalize/                   # shared C14N + masking rules
├── conformance/                 # conformance-category drivers
├── interop/                     # interop-category drivers
├── comparators/                 # Dockerfiles/config per comparator (CXF, Zeep, onvif-srvd, Xerces)
├── docker-compose.yml           # Layer-2 topology: our server + comparators + validator
└── src/ or tests/               # Rust orchestrator (Layer 2) + Rust replay tests (Layer 1)
```

## 7. CI

- **Per-commit (existing CI):** run the crossref Rust **Layer 1** (replay vs frozen
  snapshots). Fast, no Docker.
- **Nightly / on-demand / pre-release (new workflow, Linux + Docker):** run
  **Layer 2** — `docker compose up`, drive scenarios, schema-validate via Xerces,
  diff vs references, regenerate snapshots, run live interop clients, emit report.
- **Snapshot drift** surfaces as a failing Layer-2 run with the diff, to be
  reviewed and committed deliberately.

## 8. Phasing

- **Phase 1 (this spec): framework + soap-server suite.**
  - **1a** — scenarios + normalization + snapshot format + **Rust Layer-1 replay/diff**, seeded with self-captured baselines (immediate regression value, no Docker).
  - **1b** — Docker Layer 2: CXF **conformance** reference server + Rust orchestrator that regenerates snapshots from CXF and validates via **Xerces**. (External correctness first enters here.)
  - **1c** — **interop**: CXF + Zeep clients drive our server; capture/replay traces; live runs assert client operations succeed.
- **Phase 2:** onvif-server suite — same framework, ONVIF comparators
  (`onvif-srvd`, `python-onvif-zeep`, ONVIF XSD via Xerces). Own spec→plan→build.
- **Phase 3 (future):** additional comparators per scenario, multi-version
  matrices, richer reporting.

## 9. Caveats & risks

- **ONVIF conformance diffing is structural/schema-level + masked**, not exact
  content: a reference *device* legitimately differs in tokens, capabilities, and
  profiles. soap-server↔CXF on a *controlled* test WSDL allows much tighter,
  near-structural diffs.
- **Official ONVIF Device Test Tool** is a Windows-only GUI and not CI-automatable;
  it remains a documented **manual pre-release gate**.
- **Comparator availability:** `onvif-srvd`/gSOAP reference servers must be
  Dockerized and pinned; if a chosen reference proves unsuitable, the manifest
  makes swapping it cheap.
- **Snapshot churn:** legitimate reference/version changes will move snapshots;
  the reviewed-drift workflow keeps this honest but adds review overhead.

## 10. Success criteria (Phase 1)

1. `crossref` exists as a `publish = false` workspace member in soap-server;
   `cargo publish --dry-run` of the library crate is unaffected.
2. Layer 1 runs in the existing per-commit CI with no Docker and diffs our
   server's responses against the snapshot corpus for a seed set of scenarios.
3. Layer 2 (Docker) brings up CXF + Xerces + our server, schema-validates every
   response, diffs our responses against CXF for the conformance scenarios, and
   regenerates the snapshot corpus.
4. Interop: a CXF client and a Zeep client complete their scenario operation
   sequences against our server in Layer 2.
5. The comparator manifest drives which comparators run; adding one needs only a
   manifest entry + container.
