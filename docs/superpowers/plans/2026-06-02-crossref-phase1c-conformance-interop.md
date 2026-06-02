# crossref Phase 1c — remaining conformance + interop Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Finish Phase 1 — promote the remaining 10 `unverified` scenarios (SOAP 1.1, WS-Security, WSDL-rewrite) to `verified` via the appropriate authority, and add the **interop** category (real CXF + Zeep clients driving our server) — so every §10 seed scenario reaches a §5.7 verdict and no seed scenario is left `unverified` or `harness-error` in a green run (spec §11.5).

**Architecture:** Builds directly on the Phase 1b Layer-2 pipeline (compose topology, Java XML oracle, CXF reference server, Rust orchestrator). Phase 1c (a) adds a SOAP 1.1 CXF binding + the real SOAP 1.1 envelope schema so the 3 soap11 scenarios diff our-vs-CXF like the 1.2 ones; (b) adds a WSS4J-secured CXF endpoint so the 5 WS-Security scenarios compare *outcome-equivalence* (both accept→equivalent body, or both reject→equivalent fault class) at the body level (response Security headers masked); (c) promotes the 2 WSDL-rewrite scenarios by **oracle WSDL-schema validity** of our served WSDL (CXF regenerates its own WSDL, so address-rewrite is not a meaningful our-vs-CXF byte diff — the authority here is "is our rewritten WSDL schema-valid + does the non-matched service address stay put", checked structurally); (d) adds the **interop** category: two new comparator containers (a CXF JAX-WS client and a Python Zeep client) that drive *our* controlled server, asserting their operations succeed, with the captured (request, response) traces fed through the same normalize/oracle pipeline.

**Tech Stack:** Everything from 1b, plus: CXF WSS4J (`cxf-rt-ws-security`), a CXF client (`JaxWsProxyFactoryBean`), a Python Zeep client container (`python:3.12-slim` + `zeep`), and additional vendored schemas (real SOAP 1.1 envelope, WSDL 1.1 + soap12 binding) in the oracle.

**Spec:** `docs/superpowers/specs/2026-06-02-crossref-harness-design.md` — completes §4.1 interop category, §5.5 comparators (CXF interop client + Zeep), §5.6 (SOAP 1.1 + WSDL schema levels), §8 phase 1c, §11.4 (interop clients complete sequences) + §11.5 (every seed scenario verdict).

**Scenarios completed by this plan (the 10 remaining + interop):** `soap11_echo_success`, `soap11_fault`, `soap11_named_present`, `wssec_digest_success`, `wssec_bad_password`, `wssec_stale_timestamp`, `wssec_wrong_username`, `wssec_missing_auth`, `wsdl_rewrite_single`, `wsdl_rewrite_multi`, plus new interop scenarios `interop_cxf_echo`, `interop_zeep_echo`.

---

## File Structure

- `crossref/comparators/oracle/src/main/resources/schemas/soap11-envelope.xsd` (replace placeholder with the real schema) + register WSDL/binding schemas.
- `crossref/comparators/oracle/src/main/java/crossref/oracle/Oracle.java` (modify) — register `soap11-envelope` (real) + `wsdl11` schema ids.
- `crossref/comparators/cxf/` (modify) — add a SOAP 1.1 binding endpoint + a WSS4J-secured endpoint; `pom.xml` adds `cxf-rt-ws-security`.
- `crossref/comparators/cxf-client/` (create) — CXF JAX-WS interop client container.
- `crossref/comparators/zeep-client/` (create) — Python Zeep interop client container.
- `crossref/docker-compose.yml` + `manifest.toml` (modify) — add cxf-client, zeep-client (interop role), pin their images.
- `crossref/src/layer2/verdict.rs` (modify) — add `evaluate_outcome_equivalence` for WS-Security; add interop verdict path.
- `crossref/src/layer2/mod.rs` (modify) — extend `run()`: soap11 conformance, wssec body-level comparison, wsdl-rewrite validity promotion, and a new interop driver.
- `crossref/src/layer2/interop.rs` (create) — drive the interop client containers, assert success, capture+normalize traces.
- `crossref/scenarios/interop_*.toml` + request fixtures (create).
- `crossref/snapshots/canonical/*.c14n` (generated) — evidence for the newly promoted scenarios.
- `crossref/README.md` (modify) — interop + completed-Phase-1 section.

---

## Task 1: Real SOAP 1.1 envelope schema + WSDL schema in the oracle

**Files:**
- Replace: `crossref/comparators/oracle/src/main/resources/schemas/soap11-envelope.xsd`
- Add: `crossref/comparators/oracle/src/main/resources/schemas/wsdl11.xsd` (+ `soap12-binding.xsd` if needed for the WSDL-rewrite validation)
- Modify: `Oracle.java` schema registration + `SCHEMAS.md`

- [ ] **Step 1: Vendor the real SOAP 1.1 envelope schema** from `http://schemas.xmlsoap.org/soap/envelope/` (Phase 1b left a placeholder). Save verbatim; document the source URL in SCHEMAS.md.
- [ ] **Step 2: Vendor the WSDL 1.1 schema** (`http://schemas.xmlsoap.org/wsdl/`) and the WSDL SOAP 1.2 binding schema (`http://schemas.xmlsoap.org/wsdl/soap12/`) for validating our served WSDL. Wire imports to local copies (same `LSResourceResolver` approach the oracle already uses).
- [ ] **Step 3: Register** `soap11-envelope` (now real) and `wsdl11` schema ids in `Oracle.java` (the resolver already handles classpath imports).
- [ ] **Step 4: Rebuild + smoke-test** (Bash `dangerouslyDisableSandbox: true` for docker):
```
docker build -t crossref-oracle:dev crossref/comparators/oracle
docker run -d --name ora -p 8081:8081 crossref-oracle:dev; sleep 3
# real SOAP 1.1 envelope validates:
curl -s -X POST 'localhost:8081/validate?schema=soap11-envelope' --data-binary @crossref/scenarios/soap11_echo_success.request.xml; echo
# our served WSDL validates against wsdl11 (capture our server's WSDL first if needed)
docker rm -f ora
```
Expected: the SOAP 1.1 request envelope validates `{"valid":true}` against the real soap11 schema.
- [ ] **Step 5: Commit** `git add crossref/comparators/oracle && git commit -m "feat(crossref): real SOAP 1.1 envelope + WSDL schemas in oracle"`

---

## Task 2: CXF SOAP 1.1 binding endpoint + soap11 conformance

**Files:**
- Modify: `crossref/comparators/cxf/src/main/java/crossref/cxf/Main.java` (publish a second endpoint with SOAP 1.1 binding at `/soap11`), or configure the existing service to also accept 1.1.
- Modify: `crossref/docker-compose.yml` (no new service; same cxf container exposes the extra path), `crossref/src/layer2/mod.rs` (route soap11 scenarios to the CXF 1.1 endpoint + our server, which auto-detects 1.1 from content-type).

- [ ] **Step 1:** Add a SOAP 1.1 endpoint in CXF `Main.java` using a `JaxWsServerFactoryBean` with `setBindingId("http://schemas.xmlsoap.org/wsdl/soap/http")` (SOAP 1.1) at address `http://0.0.0.0:8082/soap11`, same `ControlledImpl`. Our server already detects 1.1 from `content_type=text/xml` on the existing `/soap` path.
- [ ] **Step 2:** In `run()`, for the 3 soap11 scenarios (`soap11_echo_success`, `soap11_fault`, `soap11_named_present`): POST to our `/soap` (content-type `text/xml`) and to CXF `/soap11`; validate both against `soap11-envelope`; for the fault scenario mask the SOAP 1.1 `faultstring` text (path `Envelope/Body/Fault/faultstring`) — reason text non-asserted (§10); diff with prefix-canon; verdict; promote on Pass.
- [ ] **Step 3: Rebuild CXF + end-to-end run** the 3 soap11 scenarios (Bash unsandboxed):
```
docker compose -f crossref/docker-compose.yml -f crossref/docker-compose.local.yml up -d --build
cargo run -p crossref --bin layer2 -- --promote --scenarios soap11_echo_success,soap11_fault,soap11_named_present
```
(Add a `--scenarios <csv>` filter to `bin/layer2.rs` + `run()` to run a subset; if absent, add it — small change.)
Expected: 3 soap11 scenarios Pass; promoted to verified.
- [ ] **Step 4:** If a soap11 scenario is `SutFail`, STOP and report (real finding). Else commit:
`git add crossref/comparators/cxf crossref/src/layer2 crossref/src/bin/layer2.rs crossref/snapshots && git commit -m "feat(crossref): SOAP 1.1 conformance vs CXF (3 scenarios verified)"`

---

## Task 3: WS-Security conformance vs CXF WSS4J (5 scenarios)

**Files:**
- Modify: `crossref/comparators/cxf/pom.xml` (add `cxf-rt-ws-security`), `Main.java` (a WSS4J-secured endpoint at `/soapsec`), add a `PasswordCallback`.
- Modify: `crossref/src/layer2/verdict.rs` (`evaluate_outcome_equivalence`), `crossref/src/layer2/mod.rs` (wssec routing + body-level compare with response-Security-header masking).

**Comparison model (read first):** WS-Security state (nonce caches, timestamp windows, exact fault wording) legitimately differs between independent servers. Per §10, the wssec conformance signal is **outcome-equivalence + schema-validity**, NOT byte-equality:
- success → both return HTTP 200 with a schema-valid envelope whose **body** (ignoring any response `wsse:Security` header) is the equivalent EchoResponse;
- reject → both return a schema-valid SOAP fault of the equivalent class (Sender / security). Reason text + fault detail NOT asserted.

- [ ] **Step 1:** Add `cxf-rt-ws-security` to the CXF pom. Add a WSS4J-secured endpoint `/soapsec` in `Main.java`: `WSS4JInInterceptor` with `ACTION=UsernameToken`, a `PasswordCallbackHandler` that supplies `secret` for `alice` (and the digest is verified by WSS4J), and a large timestamp TTL (`"ttl"`/`"futureTimeToLive"` set high) so the fixed-2020 `Created` in the success fixture is accepted — MIRRORING our lenient authed SUT. For the stale scenario, WSS4J with a normal TTL would reject; route `wssec_stale_timestamp` to a SECOND secured endpoint `/soapsec-strict` with a normal TTL (mirrors our strict SUT).
- [ ] **Step 2:** Add `evaluate_outcome_equivalence(our, cxf)` to `verdict.rs`: Pass if (both schema-valid) AND (both success with equal masked bodies, OR both faults). Add unit tests (both-success-equal→Pass; both-fault→Pass; one-success-one-fault→SutFail; our-invalid→SutFail).
- [ ] **Step 3:** In `run()`, route the 5 wssec scenarios: our server uses its authed paths (lenient for digest_success/bad_password/wrong_username/missing_auth; strict for stale) — but our controlled-server binary currently serves only the unauthed Echo. **Sub-decision:** the Layer-2 controlled-server binary must also expose authed endpoints mirroring the Layer-1 SUT. Add to `bin/controlled_server.rs`: mount authed services (lenient at `/soapsec`, strict at `/soapsec-strict`) using `ServerBuilder…auth(alice/secret)…timestamp_tolerance_secs(huge|300)`. Route wssec scenarios to the matching paths on BOTH our server and CXF. For the response body comparison, mask any `Envelope/Header` (the whole header subtree — response Security/Timestamp) so only the body is compared.
- [ ] **Step 4: Rebuild + run** the 5 wssec scenarios (unsandboxed). Expected: digest_success → both 200 + equal EchoResponse body; bad_password/wrong_username/missing_auth → both fault; stale → both fault (against strict). All Pass via outcome-equivalence; promote.
- [ ] **Step 5: RISK handling.** If WSS4J refuses the 2020 timestamp regardless of TTL, or computes the digest differently (it shouldn't — same OASIS algorithm), a scenario may be `SutFail` or need a `known-divergence`. If `SutFail`: STOP, capture both responses + report (could be a real our-server WS-Sec bug). If it's a CXF/WSS4J config artifact (e.g. WSS4J adds a response Timestamp we already mask, or rejects on a policy we can't disable), record a documented `known-divergence` with the reason. Report which path was taken per scenario.
- [ ] **Step 6: Commit** `git add crossref/comparators/cxf crossref/src/{layer2,bin} crossref/snapshots && git commit -m "feat(crossref): WS-Security conformance vs CXF WSS4J (5 scenarios)"`

---

## Task 4: WSDL-rewrite promotion via oracle WSDL-schema validity (2 scenarios)

**Files:**
- Modify: `crossref/src/layer2/mod.rs` (a validity-only promotion path for `wsdl_rewrite_single`/`wsdl_rewrite_multi`).

**Model:** CXF regenerates its own WSDL, so our-vs-CXF byte diff is not meaningful for the rewrite. The authority here is: (1) our served WSDL is **schema-valid WSDL** (oracle `wsdl11`), and (2) the address-rewrite invariant holds — the requested service's `<soap:address location>` is rewritten to the request host, and for multi-service the *non-matched* service's address is preserved. (1) is oracle-validated; (2) is a structural assertion in Rust on the served WSDL (path-scoped, not byte-diff).

- [ ] **Step 1:** In `run()`, for the 2 wsdl_rewrite scenarios: GET the WSDL from our server (`/soap?wsdl` single; `/soap/a?wsdl` multi — our controlled-server must serve these; if the controlled-server binary doesn't mount the multi-service WSDL, add a multi-service mount as the Layer-1 SUT does). Validate the returned WSDL via `oracle.validate(wsdl, "wsdl11")`. Assert the rewrite invariant structurally (parse, check the matched service address host == request host; for multi, the other service address path unchanged).
- [ ] **Step 2:** Verdict: `Pass` if WSDL is schema-valid AND the rewrite invariant holds (this is a validity+invariant promotion, no CXF reference). Promote (store the oracle-canonical WSDL as evidence).
- [ ] **Step 3: Run** the 2 scenarios (unsandboxed); expected both Pass + verified. Commit:
`git add crossref/src/layer2 crossref/snapshots && git commit -m "feat(crossref): WSDL-rewrite verified via oracle WSDL-schema validity + rewrite invariant"`

---

## Task 5: Interop category — CXF client + Zeep client containers

**Files:**
- Create: `crossref/comparators/cxf-client/` (pom.xml, `Client.java`, Dockerfile) — a CXF JAX-WS client that calls our server's `Echo`, asserts the response, exits 0/1.
- Create: `crossref/comparators/zeep-client/` (`client.py`, `requirements.txt`, Dockerfile) — a Python Zeep client that loads our served WSDL, calls `Echo`, asserts, exits 0/1.
- Modify: `crossref/docker-compose.yml` (add `cxf-client`, `zeep-client` as one-shot services, `profiles: ["interop"]` so they don't run in the conformance up), `manifest.toml` (interop-client role, pinned images).
- Create: `crossref/src/layer2/interop.rs` + `crossref/scenarios/interop_cxf_echo.toml`, `interop_zeep_echo.toml`.

**Model (§4.1 interop):** a real third-party CLIENT drives OUR server. The live run asserts the client's operations succeed (exit 0). The client also emits its (request,response) trace to stdout/a mounted file; the orchestrator normalizes the response through the same pipeline and records it as the interop snapshot (verified-by-successful-interop).

- [ ] **Step 1: CXF client** (`Client.java`): `JaxWsProxyFactoryBean` (or `Service`/`Dispatch`) targeting `http://controlled-server:8080/soap`, calls `Echo("interop")`, asserts the response text == `interop`, prints the raw response envelope to stdout, exits 0 on success / 1 on failure. Dockerfile multi-stage (Maven build), digest-pinned base.
- [ ] **Step 2: Zeep client** (`client.py`): `from zeep import Client`; load our WSDL from `http://controlled-server:8080/soap?wsdl`; call the `Echo` op with `Text="interop"`; assert the result; print the response; `sys.exit(0/1)`. `requirements.txt` pins `zeep==4.*`. Dockerfile `FROM python:3.12-slim@sha256:…`, `pip install -r requirements.txt`.
   - NOTE: Zeep needs our WSDL to be consumable. If Zeep can't parse the controlled WSDL (e.g. needs a reachable schema import), adjust the controlled WSDL serving or embed the schema. Report if Zeep rejects our WSDL — that itself is an interop finding.
- [ ] **Step 3: compose** — add both as `profiles: ["interop"]` one-shot services depending on `controlled-server` healthy. `manifest.toml` records them (role `interop-client`, pinned image, scenarios `interop_*`).
- [ ] **Step 4: `interop.rs` driver** — `run_interop(repo_root) -> Vec<(String, Verdict)>`: for each interop client, `docker compose … run --rm <client>`; capture exit code + stdout. Verdict `Pass` if exit 0 (operations succeeded) and the printed response normalizes/validates clean; `SutFail` if the client could not complete its operations against our server (a real interop failure); `HarnessError` on container failure. Normalize the captured response via `mask_only`+oracle and store as `snapshots/canonical/interop_*.c14n`; promote.
- [ ] **Step 5: scenarios** — `interop_cxf_echo.toml` / `interop_zeep_echo.toml` (outcome=success; marker that these are interop-category, driven by `interop.rs` not the POST loop).
- [ ] **Step 6: wire into `bin/layer2.rs`** — after conformance `run()`, call `interop::run_interop(...)` (gated by `--interop` flag or always in the full run), merge verdicts into the Report.
- [ ] **Step 7: Run** (unsandboxed): bring topology up, `docker compose … --profile interop run --rm cxf-client` and `… zeep-client`, confirm both exit 0 and their Echo round-trips. Then the orchestrator interop path. Expected: `interop_cxf_echo` + `interop_zeep_echo` Pass.
   - If a client CANNOT complete (e.g. Zeep rejects our WSDL, or CXF client gets a malformed response), that is a REAL interop SutFail — STOP and report with the client's error output. This is exactly the kind of cross-impl problem interop exists to catch.
- [ ] **Step 8: Commit** `git add crossref/comparators/cxf-client crossref/comparators/zeep-client crossref/docker-compose.yml crossref/manifest.toml crossref/src/layer2 crossref/scenarios crossref/snapshots && git commit -m "feat(crossref): interop category — CXF + Zeep clients drive our server"`

---

## Task 6: Report, CI, README close-out (Phase 1 complete)

**Files:**
- Modify: `crossref/src/layer2/report.rs` (interop section), `.github/workflows/layer2.yml` (run conformance + interop), `crossref/README.md`.

- [ ] **Step 1:** Extend the report to show conformance + interop verdicts and the still-`unverified` count (target: **0** after this plan). `is_green()` already fails on SutFail/HarnessError.
- [ ] **Step 2:** Update `layer2.yml` so the nightly job runs the full set (conformance + `--interop`) and the `--profile interop` clients. Tear down with `if: always()`.
- [ ] **Step 3:** Full end-to-end run (unsandboxed): `cargo run -p crossref --bin layer2 -- --promote --interop`. Expected: **all 22 conformance scenarios + 2 interop scenarios verdict = Pass/KnownDivergence; 0 unverified; 0 SutFail/HarnessError.** Confirm `grep -c unverified crossref/snapshots/status.toml` → 0 (or only documented exceptions).
- [ ] **Step 4: Verify Layer-1 still green** (`cargo test -p crossref --test layer1_replay`) and `.xml` snapshots unchanged; full `cargo test --workspace` green; clippy/fmt clean; `cargo package --list -p soap-server | grep -c '^crossref/'` == 0.
- [ ] **Step 5:** README: mark Phase 1 complete (1a+1b+1c); document interop; update the comparator list.
- [ ] **Step 6: Commit** `git add -A crossref .github && git commit -m "feat(crossref): Phase 1 complete — all seed scenarios verified + interop in CI"`

---

## Self-review notes (author)

- **Spec coverage:** §4.1 interop (Task 5), §5.5 comparators CXF-client + Zeep (Task 5), §5.6 SOAP 1.1 + WSDL schema levels (Tasks 1,2,4), §8 phase 1c (all), §11.4 interop clients complete sequences (Task 5), §11.5 every seed scenario reaches a verdict + none left unverified (Task 6 Step 3).
- **Comparison-model decisions (documented, not silent):** WS-Security uses outcome-equivalence + body-level diff (response Security headers masked), not byte-equality, because WS-Sec server state legitimately differs (§10). WSDL-rewrite uses oracle WSDL-validity + a structural rewrite-invariant assertion, not our-vs-CXF byte diff, because CXF regenerates its own WSDL.
- **Known implementer risks flagged inline:** (1) WSS4J accepting the fixed-2020 timestamp (large TTL; else known-divergence — Task 3 Step 5); (2) Zeep consuming our served WSDL (Task 5 Step 2 — a real interop finding if it can't); (3) the controlled-server binary must grow authed + multi-service mounts to mirror the Layer-1 SUT (Tasks 3,4).
- **SutFail honesty:** every task says STOP + report on SutFail (real our-server finding), never mask to force green — same discipline that (correctly) found nothing in 1b and the envelope bug in 1a.
- **Publish safety + Layer-1 integrity:** all additions under `crossref/`/`.github/`; Layer-1 `.xml` snapshots never overwritten (promotion flips status + writes `canonical/`); re-verify `cargo package` exclusion == 0 (Task 6 Step 4).
- **Scope honesty:** this completes Phase 1. Phase 2 (onvif-server) remains its own spec→plan→build.
