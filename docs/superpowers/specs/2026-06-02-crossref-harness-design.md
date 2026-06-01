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
  diff. **This is only meaningful when both servers run the *same* controlled
  service:** Phase 1 defines a controlled WSDL suite plus matching CXF service
  implementations whose responses are deterministic (fixed return values; no
  clocks/RNG outside masked fields). Otherwise a "difference" reflects
  handler/business-data divergence, not a SOAP conformance gap. See §5.8.
- **interop** — a real third-party *client* drives *our server* through an
  operation sequence; capture the `(client-request, our-response)` trace; the live
  run asserts the client's operations actually succeed, and the captured requests
  feed the offline replay/diff.

### 4.2 Two execution layers (both apply to both categories)

- **Layer 1 — fast offline (Rust):** replay captured/scenario requests against our
  server, normalize, and diff against frozen snapshots. No Docker, no network.
  This is the developer inner loop and the per-commit CI gate. It detects
  *regressions* against snapshots — Layer 1 alone proves "unchanged", not
  "correct". A snapshot carries conformance weight only once Layer 2 has
  **promoted** it (schema-valid + reference-agreeing); Phase-1a self-captured
  snapshots are explicitly **unverified** baselines until then (see §5.2).
- **Layer 2 — live Docker (Rust orchestrator):** `docker compose` brings up our
  server plus the comparator containers. The orchestrator drives every scenario,
  captures responses, **delegates XSD validation + C14N to the containerized Java
  XML oracle** (Xerces for validation; Apache Santuario / JDK XMLDSig for
  canonicalization), diffs against the references, **regenerates and promotes the
  snapshot corpus from the authorities**, runs the live interop clients, and emits
  a report with a per-scenario verdict (§5.7). Runs nightly / on-demand /
  pre-release.

### 4.3 Languages

- **Harness logic** (both layers: orchestration, volatile-field masking, diffing,
  reporting) = **Rust**. Single-language, cargo-native, and Layer 1 and Layer 2
  share the same normalization/diff code.
- **Authorities run in containers:**
  - **Java** — Apache CXF (SOAP reference server + interop client) and the
    **Java XML oracle**: Xerces/JAXP for XSD validation + Apache Santuario (or the
    JDK XML Digital Signature API) for exclusive C14N. (Xerces *validates*; it does
    not canonicalize — C14N is a separate XML-Security concern.)
  - **Python** — `python-onvif-zeep` (ONVIF interop client).
  - **C/gSOAP** — `onvif-srvd` (ONVIF reference server).
- **Spec-sensitive grading** (XSD validation, canonicalization) is delegated to the
  **Java XML oracle** (Xerces + Santuario) — reference-grade and independent of
  Rust. The Rust orchestrator invokes the oracle container; it never validates or
  canonicalizes XML itself.

> Rationale for Rust orchestrator: third-party clients are polyglot and must run
> as their own containers regardless of runner language, so an in-process client
> host buys nothing. With validation/C14N containerized in the Java XML oracle, the
> runner's remaining job is orchestration + masking + diff + reporting — best kept
> in one language shared with Layer 1.

## 5. Components

### 5.1 Scenarios (single source of truth)
A declarative `scenarios/` set, consumed by **both** layers so Rust and the Docker
orchestrator exercise identical cases. Each scenario declares:
- **operation name** + request body + request headers + auth context;
- **HTTP method + path** (e.g. `POST /onvif/device_service`, or `GET …?wsdl`);
- **expected HTTP status**;
- **expected SOAP version** (1.1 / 1.2);
- **expected `Content-Type`**;
- **expected outcome** — `success` or `fault`;
- for faults: the **expected fault class** (code/subcode) and the **detail policy**
  (e.g. "detail present with a raw XML child"; reason text is *not* asserted — see
  §10 on negative-case equivalence).

### 5.2 Snapshots (golden corpus)
Per scenario, the *normalized* expected response(s), consumed by Layer 1. Each
snapshot carries a **provenance status**:
- `unverified` — self-captured from our own server (Phase-1a bootstrap). Proves
  *unchanged*, never *correct*; carries no conformance weight.
- `verified` — **promoted by Layer 2**: schema-valid against the Java XML oracle
  AND in agreement with the reference server(s). Only `verified` snapshots count as
  conformance evidence.

Layer 2 **regenerates and promotes** snapshots from the authorities. Snapshots are
golden files: **drift is a reviewed change** — Layer 2 fails with the diff (and may
open a PR); snapshots are never updated silently. Reports and CI MUST surface the
count of still-`unverified` snapshots so self-captured baselines are never mistaken
for conformance evidence.

### 5.3 Normalization
Exclusive XML **C14N** (performed by the Java XML oracle) plus a **masking ruleset**
for volatile fields (message IDs, UUIDs, timestamps, nonces, generated tokens), so
diffs are stable and meaningful. **Mask rules MUST be path-scoped:** each rule binds
a canonical path (XPath or equivalent) and the scenario(s) it applies to.
**Global/value-pattern masking is prohibited** — e.g. masking every UUID-shaped
string everywhere would hide a wrong `EndpointReference/Address`. The masking + diff
comparison is plain Rust (not spec interpretation); canonicalization is delegated to
the oracle.

**Normalization pipeline (exact order):**
1. **parse** the response XML;
2. **validate** envelope / body / fault / headers (§5.6) via the oracle;
3. **apply path-scoped masks** on the parsed DOM / canonical-path model (so masks
   are path-bound, never string-bound);
4. **canonicalize** the masked tree with the Java XML oracle (exclusive C14N);
5. **diff** the resulting canonical bytes.

Masking happens on the tree **before** canonicalization, so a mask is always a path
selection — this is what structurally prevents accidental string masks.

### 5.4 Comparator manifest
A per-repo `manifest.toml` listing each comparator: `name`, `role`
(`reference-server` | `interop-client` | `schema-oracle`), the Docker image pinned
**by immutable digest** (`image@sha256:…`, never a mutable `latest`/`stable`/vendor
tag — a conformance oracle must not drift), the human-readable version recorded
alongside for clarity, and the scenarios it participates in. Adding/swapping a
comparator = a manifest entry + a container. Multi-version is a future extension of
this same manifest.

### 5.5 Comparators (current stable subset)
- **soap-server:** CXF (conformance reference server *and* interop client) + Zeep
  (second interop client). Java XML oracle (Xerces XSD validation + Santuario C14N)
  as the schema/canonicalization oracle.
- **onvif-server (Phase 2):** `onvif-srvd` (conformance reference server) +
  `python-onvif-zeep` (interop client) + ONVIF XSD validation via the Java XML oracle.

### 5.6 Schema-validation targets
"Schema-validate the response" is made precise — every response is validated at
these levels (failure at any level fails the scenario):
1. **SOAP envelope structure** against the SOAP 1.1 / 1.2 envelope schema (matching
   the scenario's SOAP version).
2. **Body response element** against the WSDL/XSD output-message type for that
   operation (the document/literal element or RPC wrapper).
3. **Fault structure** against the SOAP-version fault schema (1.1 `faultcode`/
   `faultstring`/`detail` vs 1.2 `Code`/`Reason`/`Detail`).
4. **Headers** — e.g. WS-Security/WS-Addressing:
   - If a scenario **declares expected response headers**, validate them against
     their schemas and **fail the scenario on invalidity**.
   - If a scenario **does not declare header validation**, report headers as
     **`unvalidated`** in the scenario output (never silently passed).

The oracle validates the *envelope* and the *payload* separately (envelope schema
for the SOAP frame; the operation's output schema for the body child) rather than
attempting one whole-document validation, which is awkward because envelope and
payload come from different schemas.

### 5.7 Verdict model
Every scenario run yields exactly one verdict; the report aggregates them:
- **`pass`** — our response is schema-valid AND agrees with the reference(s) under
  the masked diff.
- **`sut-fail`** — our response is schema-invalid, or disagrees with a reference
  that is itself schema-valid. This is the signal we care about.
- **`reference-disagreement`** — references disagree *with each other* (both
  schema-valid, structurally different). Not a SUT verdict: flagged for human
  triage and resolved by recording an allowed-divergence (below) or dropping a
  reference for that scenario. Never auto-passes or auto-fails the SUT.
- **`known-divergence`** — a recorded, justified, scenario- and path-scoped
  allowance where our output legitimately differs from a reference (documented with
  a reason). Counts as pass but is listed explicitly.
- **`harness-error`** — infrastructure failure (container down, oracle crash,
  timeout). Never silently counted as pass.

### 5.8 Controlled service fixtures (conformance)
For conformance to measure *SOAP* differences and not business-data differences,
Phase 1 ships a **controlled WSDL suite** plus **matching CXF service
implementations** with deterministic responses (fixed values; any nondeterminism
confined to masked fields). Our `soap-server` is wired with handlers returning the
*same* deterministic data for the same WSDLs. The fixtures deliberately exercise the
hard cases (namespaces, faults, doc/literal, WS-Security) — see §10.

## 6. Per-repo layout

```
<repo>/crossref/                 # cargo workspace member, publish = false
├── Cargo.toml                   # the crossref member (orchestrator + Layer-1 tests)
├── scenarios/                   # declarative scenario fixtures (shared by both layers)
├── snapshots/                   # golden normalized responses (regenerated by Layer 2)
├── manifest.toml                # comparator registry (name/role/image@sha256/version/scenarios)
├── normalize/                   # shared C14N config + path-scoped mask rules
├── fixtures/                    # controlled WSDLs + deterministic CXF service impls (§5.8)
├── conformance/                 # conformance-category drivers
├── interop/                     # interop-category drivers
├── comparators/                 # Dockerfiles/config per comparator (CXF, Zeep, onvif-srvd, Java XML oracle)
├── docker-compose.yml           # Layer-2 topology: our server + comparators + oracle
└── src/ or tests/               # Rust orchestrator (Layer 2) + Rust replay tests (Layer 1)
```

## 7. CI

- **Per-commit (existing CI):** run the crossref Rust **Layer 1** (replay vs frozen
  snapshots). Fast, no Docker.
- **Nightly / on-demand / pre-release (new workflow, Linux + Docker):** run
  **Layer 2** — `docker compose up`, drive scenarios, schema-validate via the Java
  XML oracle, diff vs references, regenerate + promote snapshots, run live interop
  clients, emit a per-scenario verdict report (§5.7).
- **Snapshot drift** surfaces as a failing Layer-2 run with the diff, to be
  reviewed and committed deliberately.

## 8. Phasing

- **Phase 1 (this spec): framework + soap-server suite.**
  - **1a** — controlled WSDL fixtures + scenarios + normalization + snapshot format + **Rust Layer-1 replay/diff**, seeded with self-captured **`unverified`** baselines (immediate regression value, no Docker; not yet conformance evidence).
  - **1b** — Docker Layer 2: CXF **conformance** reference server (running the §5.8 fixtures) + Rust orchestrator that validates via the **Java XML oracle**, diffs vs CXF, and **promotes** snapshots `unverified`→`verified`. (External correctness first enters here.)
  - **1c** — **interop**: CXF + Zeep clients drive our server; capture/replay traces; live runs assert client operations succeed.
- **Phase 2:** onvif-server suite — same framework, ONVIF comparators
  (`onvif-srvd`, `python-onvif-zeep`, ONVIF XSD via the Java XML oracle). Own
  spec→plan→build.
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

## 10. Required Phase 1 seed scenarios

These named cases are mandatory in Phase 1 — they encode the highest-value
regressions (including the exact classes found in the round-2 review). Phase 1 is
not complete until each exists and reaches a verdict:

- SOAP 1.1 success **and** fault response.
- SOAP 1.2 success **and** fault response.
- Fault `detail` carrying a raw XML child element (not escaped text).
- Namespace declarations placed at Envelope, Header, Body, operation root, **and**
  nested payload (one scenario per placement).
- Namespace **prefix shadowing** — Envelope and Body redeclare the same prefix to
  different URIs.
- Document/literal operation with an **inline** complex type — required child
  present (pass) and omitted (fault).
- Document/literal operation with a **named** type reference — same present/omitted
  pair.
- WS-Security UsernameToken: **digest success**, **bad password**, **stale
  timestamp**, **replay** (nonce reuse), and **missing auth** on a non-bypassed op.
- `GET ?wsdl` address rewrite for a **single-service** and a **multi-service** WSDL
  (non-matched service address preserved).

**Negative-case agreement semantics.** For fault/negative scenarios, "reference
agreement" means **both produce a schema-valid SOAP fault of the equivalent fault
class** (matching code/subcode per §5.1) — **not** identical `reason`/`faultstring`
text, which legitimately differs between implementations. The masked diff and the
verdict model (§5.7) treat reason text as non-asserted for faults; the fault *class*
and *structure* are what must agree.

(ONVIF-specific seed scenarios — WhiteBalance single-element, PTZ coordinate
rejection, pull-point WS-Addressing, discovery probe matching — are defined in the
Phase 2 spec.)

## 11. Success criteria (Phase 1)

1. `crossref` exists as a `publish = false` workspace member in soap-server;
   `cargo publish --dry-run` of the library crate is unaffected.
2. Layer 1 runs in the existing per-commit CI with no Docker and diffs our
   server's responses against the snapshot corpus, reporting the count of
   still-`unverified` snapshots.
3. Layer 2 (Docker) brings up CXF (running the §5.8 controlled fixtures) + the Java
   XML oracle + our server, applies the §5.6 schema-validation levels to every
   response, diffs our responses against CXF for the conformance scenarios, and
   **promotes** the snapshot corpus `unverified`→`verified`.
4. Interop: a CXF client and a Zeep client complete their scenario operation
   sequences against our server in Layer 2.
5. Every §10 seed scenario exists and resolves to a §5.7 verdict; no seed scenario
   is left `unverified` or `harness-error` in a green Phase-1 run.
6. The comparator manifest (digest-pinned) drives which comparators run; adding one
   needs only a manifest entry + container.
7. All mask rules are path-scoped (no global value-pattern masks).
