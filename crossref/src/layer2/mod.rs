//! Layer-2 orchestration: compose lifecycle, endpoints, verdict, promotion, report.

pub mod compose;
pub mod interop;
pub mod promote;
pub mod report;
pub mod verdict;

use crate::normalize::{mask_only, mask_only_with_drops, AttrMaskRule, MaskRule};
use crate::oracle::Oracle;
use crate::scenario::{Outcome, Scenario, SoapVersion};
use crate::snapshot::SnapshotStore;
use promote::promote;
use report::Report;
use verdict::{Eval, Resp, Verdict};

/// Host-published port URLs for local runs (using the docker-compose.local.yml override).
pub struct Endpoints {
    pub our: String,
    pub cxf: String,
    pub oracle: String,
}

impl Endpoints {
    pub fn localhost() -> Self {
        Endpoints {
            our: "http://localhost:8080/soap".into(),
            cxf: "http://localhost:8082/soap".into(),
            oracle: "http://localhost:8081".into(),
        }
    }

    /// Base URL for our server (without the /soap path suffix).
    fn our_base(&self) -> &str {
        // our is always "http://host:port/soap" — strip the /soap suffix.
        self.our.strip_suffix("/soap").unwrap_or(&self.our)
    }

    /// Base URL for CXF (without the /soap path suffix).
    fn cxf_base(&self) -> &str {
        self.cxf.strip_suffix("/soap").unwrap_or(&self.cxf)
    }
}

/// Per-scenario routing config derived from soap_version.
struct VersionRouting {
    /// Path on our server (e.g. "/soap")
    our_path: &'static str,
    /// Full CXF URL for this scenario
    cxf_url: String,
    /// Oracle schema id to validate the SOAP envelope
    envelope_schema: &'static str,
}

impl VersionRouting {
    fn for_scenario(scenario: &Scenario, endpoints: &Endpoints) -> Self {
        match scenario.soap_version {
            SoapVersion::V11 => VersionRouting {
                our_path: "/soap",
                cxf_url: format!("{}/soap11", endpoints.cxf_base()),
                envelope_schema: "soap11-envelope",
            },
            SoapVersion::V12 => VersionRouting {
                our_path: "/soap",
                cxf_url: format!("{}/soap", endpoints.cxf_base()),
                envelope_schema: "soap12-envelope",
            },
        }
    }

    fn our_url(&self, endpoints: &Endpoints) -> String {
        format!("{}{}", endpoints.our_base(), self.our_path)
    }
}

/// WS-Security scenarios requiring outcome-equivalence comparison (not byte-diff).
/// These are handled by `run_wssec_scenario` instead of `run_scenario`.
const WSSEC_SCENARIOS: &[&str] = &[
    "wssec_digest_success",
    "wssec_bad_password",
    "wssec_wrong_username",
    "wssec_missing_auth",
    "wssec_stale_timestamp",
];

/// WSDL-rewrite scenarios (GET ?wsdl): promoted via oracle WSDL-schema validity +
/// structural rewrite-invariant assertion (not CXF byte diff).
const WSDL_REWRITE_SCENARIOS: &[&str] = &["wsdl_rewrite_single", "wsdl_rewrite_multi"];

/// All in-scope conformance scenarios for Phase 1b + Phase 1c SOAP 1.1 + WS-Security + WSDL-rewrite.
const IN_SCOPE: &[&str] = &[
    // Phase 1b: SOAP 1.2 scenarios
    "op_echo_success",
    "op_echo_missing_required",
    "op_echo_empty_text",
    "op_echo_special_chars",
    "doc_literal_named_present",
    "doc_literal_named_missing",
    "ns_on_envelope",
    "ns_on_header",
    "ns_on_body",
    "ns_on_operation",
    "ns_on_nested_payload",
    "ns_prefix_shadowing",
    // Phase 1c: SOAP 1.1 scenarios
    "soap11_echo_success",
    "soap11_fault",
    "soap11_named_present",
    // Phase 1c: WS-Security scenarios (outcome-equivalence)
    "wssec_digest_success",
    "wssec_bad_password",
    "wssec_wrong_username",
    "wssec_missing_auth",
    "wssec_stale_timestamp",
    // Phase 1c: WSDL-rewrite scenarios (oracle WSDL-schema validity + invariant)
    "wsdl_rewrite_single",
    "wsdl_rewrite_multi",
];

/// Drive all in-scope conformance scenarios, return the verdict report.
///
/// `scenarios_filter`: if `Some`, only run the listed scenario names.
///
/// For each scenario:
/// 1. POST the request to both servers (using per-soap-version URLs).
/// 2. Validate both responses via the oracle (per-soap-version schema).
/// 3. Apply masks → oracle C14N → compare.
/// 4. On Pass + promote_on_pass: write canonical evidence + flip status.
pub fn run(
    endpoints: &Endpoints,
    repo_root: &std::path::Path,
    promote_on_pass: bool,
    scenarios_filter: Option<&[String]>,
) -> Report {
    let scenarios_dir = repo_root.join("crossref/scenarios");
    let snapshots_dir = repo_root.join("crossref/snapshots");
    let store = SnapshotStore::new(&snapshots_dir);
    let oracle = Oracle::new(&endpoints.oracle);

    let mut rows: Vec<(String, Verdict)> = Vec::new();

    for &name in IN_SCOPE {
        // Apply the --scenarios filter if provided.
        if let Some(filter) = scenarios_filter {
            if !filter.iter().any(|f| f == name) {
                continue;
            }
        }

        let verdict = if WSDL_REWRITE_SCENARIOS.contains(&name) {
            run_wsdl_rewrite_scenario(
                name,
                &scenarios_dir,
                endpoints,
                &oracle,
                promote_on_pass,
                &store,
            )
        } else if WSSEC_SCENARIOS.contains(&name) {
            run_wssec_scenario(
                name,
                &scenarios_dir,
                endpoints,
                &oracle,
                promote_on_pass,
                &store,
            )
        } else {
            run_scenario(
                name,
                &scenarios_dir,
                endpoints,
                &oracle,
                promote_on_pass,
                &store,
            )
        };
        rows.push((name.to_string(), verdict));
    }

    let unverified_remaining = store.unverified_count().unwrap_or(0);
    Report {
        rows,
        unverified_remaining,
    }
}

fn run_scenario(
    name: &str,
    scenarios_dir: &std::path::Path,
    endpoints: &Endpoints,
    oracle: &Oracle,
    promote_on_pass: bool,
    store: &SnapshotStore,
) -> Verdict {
    // 1. Load scenario metadata.
    let toml_path = scenarios_dir.join(format!("{name}.toml"));
    let toml_str = match std::fs::read_to_string(&toml_path) {
        Ok(s) => s,
        Err(e) => return Verdict::HarnessError(format!("read {name}.toml: {e}")),
    };
    let scenario: Scenario = match toml::from_str(&toml_str) {
        Ok(s) => s,
        Err(e) => return Verdict::HarnessError(format!("parse {name}.toml: {e}")),
    };

    // 2. Derive per-scenario routing from soap_version.
    let routing = VersionRouting::for_scenario(&scenario, endpoints);
    let our_url = routing.our_url(endpoints);
    let cxf_url = &routing.cxf_url;
    let envelope_schema = routing.envelope_schema;

    // 3. Read request bytes.
    let request_path = scenarios_dir.join(&scenario.request_file);
    let request_bytes = match std::fs::read(&request_path) {
        Ok(b) => b,
        Err(e) => {
            return Verdict::HarnessError(format!("read request {}: {e}", scenario.request_file))
        }
    };

    // 4. POST to both servers.
    let (our_status, our_body) = match post(&our_url, &request_bytes, &scenario.content_type) {
        Ok(r) => r,
        Err(e) => return Verdict::HarnessError(format!("POST our server: {e}")),
    };
    let (cxf_status, cxf_body) = match post(cxf_url, &request_bytes, &scenario.content_type) {
        Ok(r) => r,
        Err(e) => return Verdict::HarnessError(format!("POST CXF: {e}")),
    };

    // Log status codes and bodies for diagnosis.
    eprintln!(
        "[{name}] our={our_status} cxf={cxf_status} our_body_len={} cxf_body_len={}",
        our_body.len(),
        cxf_body.len()
    );
    eprintln!("[{name}] our_body: {}", String::from_utf8_lossy(&our_body));
    eprintln!("[{name}] cxf_body: {}", String::from_utf8_lossy(&cxf_body));

    // 5. Validate both responses via oracle.
    // 5a. Envelope schema validation (using per-soap-version schema).
    let our_env_valid = match oracle.validate(&our_body, envelope_schema) {
        Ok(r) => r,
        Err(e) => return Verdict::HarnessError(format!("oracle validate our envelope: {e}")),
    };
    let cxf_env_valid = match oracle.validate(&cxf_body, envelope_schema) {
        Ok(r) => r,
        Err(e) => return Verdict::HarnessError(format!("oracle validate cxf envelope: {e}")),
    };

    // 5b. Body child validation for SUCCESS scenarios only.
    let (our_body_valid, our_body_errors) = if scenario.outcome == Outcome::Success {
        match extract_body_child(&our_body) {
            Some(child) => match oracle.validate(&child, "controlled") {
                Ok(r) => (r.valid, r.errors),
                Err(e) => {
                    return Verdict::HarnessError(format!("oracle validate our body child: {e}"))
                }
            },
            None => (
                false,
                vec!["no body child element found in our response".to_string()],
            ),
        }
    } else {
        (true, vec![]) // fault scenarios: body child validation not required
    };

    let (cxf_body_valid, cxf_body_errors) = if scenario.outcome == Outcome::Success {
        match extract_body_child(&cxf_body) {
            Some(child) => match oracle.validate(&child, "controlled") {
                Ok(r) => (r.valid, r.errors),
                Err(e) => {
                    return Verdict::HarnessError(format!("oracle validate cxf body child: {e}"))
                }
            },
            None => (
                false,
                vec!["no body child element found in CXF response".to_string()],
            ),
        }
    } else {
        (true, vec![])
    };

    // Combine envelope + body-child validity.
    let our_valid = our_env_valid.valid && our_body_valid;
    let mut our_errors = our_env_valid.errors.clone();
    our_errors.extend(our_body_errors);

    let ref_valid = cxf_env_valid.valid && cxf_body_valid;
    let mut ref_errors = cxf_env_valid.errors.clone();
    ref_errors.extend(cxf_body_errors);

    // 6. Build per-scenario masks.
    // SOAP 1.2 FAULT: mask Reason/Text content and xml:lang attr (non-asserted per spec §10).
    // SOAP 1.1 FAULT: mask faultstring text (no xml:lang attr in 1.1 per spec, but mask if present).
    let (text_masks, attr_masks): (Vec<MaskRule>, Vec<AttrMaskRule>) =
        if scenario.outcome == Outcome::Fault {
            match scenario.soap_version {
                SoapVersion::V12 => (
                    vec![MaskRule::new("Envelope/Body/Fault/Reason/Text")],
                    vec![AttrMaskRule::new(
                        "Envelope/Body/Fault/Reason/Text",
                        "xml:lang",
                    )],
                ),
                SoapVersion::V11 => (
                    // SOAP 1.1 fault: faultstring is the reason text (§10 non-asserted).
                    // Also mask xml:lang if CXF adds it (some impls do).
                    vec![MaskRule::new("Envelope/Body/Fault/faultstring")],
                    vec![AttrMaskRule::new(
                        "Envelope/Body/Fault/faultstring",
                        "xml:lang",
                    )],
                ),
            }
        } else {
            (vec![], vec![])
        };

    // 7. Mask + oracle C14N.
    let our_masked = match mask_only(&our_body, &text_masks, &attr_masks) {
        Ok(b) => b,
        Err(e) => return Verdict::HarnessError(format!("mask_only our: {e}")),
    };
    let cxf_masked = match mask_only(&cxf_body, &text_masks, &attr_masks) {
        Ok(b) => b,
        Err(e) => return Verdict::HarnessError(format!("mask_only cxf: {e}")),
    };

    let our_canon = match oracle.c14n(&our_masked) {
        Ok(b) => b,
        Err(e) => return Verdict::HarnessError(format!("oracle c14n our: {e}")),
    };
    let cxf_canon = match oracle.c14n(&cxf_masked) {
        Ok(b) => b,
        Err(e) => return Verdict::HarnessError(format!("oracle c14n cxf: {e}")),
    };

    // 8. Evaluate verdict using existing evaluate() with Result-based Eval.
    let our_result: Result<Vec<u8>, String> = if our_valid {
        Ok(our_canon.clone())
    } else {
        Err(format!("our response schema-invalid: {our_errors:?}"))
    };
    let ref_result: Result<Vec<u8>, String> = if ref_valid {
        Ok(cxf_canon.clone())
    } else {
        Err(format!("reference schema-invalid: {ref_errors:?}"))
    };

    let eval = Eval {
        sut: our_result,
        reference: ref_result,
        known_divergences: vec![],
    };
    let v = verdict::evaluate(&eval);

    // 9. Promote on pass.
    if v == Verdict::Pass && promote_on_pass {
        if let Err(e) = promote(store, name, &our_canon) {
            eprintln!("[{name}] promotion failed: {e}");
        }
    }

    v
}

/// Run a WS-Security scenario using outcome-equivalence (spec §10).
///
/// Routing:
/// - `wssec_stale_timestamp` → our `/soapsec-strict` + CXF `/soapsec-strict`
/// - all others              → our `/soapsec`         + CXF `/soapsec`
///
/// Comparison model:
/// - Validate both responses via oracle `soap12-envelope` schema.
/// - Mask the entire `Envelope/Header` subtree (drops response Security/Timestamp).
/// - Mask `Envelope/Body/Fault/Reason/Text` + `xml:lang` (reason non-asserted).
/// - Compare via `evaluate_outcome_equivalence` (both-success + equal body,
///   or both-fault → Pass; mixed outcome → SutFail).
fn run_wssec_scenario(
    name: &str,
    scenarios_dir: &std::path::Path,
    endpoints: &Endpoints,
    oracle: &Oracle,
    promote_on_pass: bool,
    store: &SnapshotStore,
) -> Verdict {
    // 1. Load scenario metadata.
    let toml_path = scenarios_dir.join(format!("{name}.toml"));
    let toml_str = match std::fs::read_to_string(&toml_path) {
        Ok(s) => s,
        Err(e) => return Verdict::HarnessError(format!("read {name}.toml: {e}")),
    };
    let scenario: crate::scenario::Scenario = match toml::from_str(&toml_str) {
        Ok(s) => s,
        Err(e) => return Verdict::HarnessError(format!("parse {name}.toml: {e}")),
    };

    // 2. Choose lenient vs strict path.
    let is_strict = name == "wssec_stale_timestamp";
    let our_path = if is_strict {
        "/soapsec-strict"
    } else {
        "/soapsec"
    };
    let cxf_suffix = if is_strict {
        "soapsec-strict"
    } else {
        "soapsec"
    };
    let our_url = format!("{}{}", endpoints.our_base(), our_path);
    let cxf_url = format!("{}/{}", endpoints.cxf_base(), cxf_suffix);

    // 3. Read request bytes.
    let request_path = scenarios_dir.join(&scenario.request_file);
    let request_bytes = match std::fs::read(&request_path) {
        Ok(b) => b,
        Err(e) => {
            return Verdict::HarnessError(format!("read request {}: {e}", scenario.request_file))
        }
    };

    // 4. POST to both servers.
    let (our_status, our_body) = match post(&our_url, &request_bytes, &scenario.content_type) {
        Ok(r) => r,
        Err(e) => return Verdict::HarnessError(format!("POST our server ({our_url}): {e}")),
    };
    let (cxf_status, cxf_body) = match post(&cxf_url, &request_bytes, &scenario.content_type) {
        Ok(r) => r,
        Err(e) => return Verdict::HarnessError(format!("POST CXF ({cxf_url}): {e}")),
    };

    eprintln!(
        "[{name}] our={our_status} ({our_url}) cxf={cxf_status} ({cxf_url}) our_body_len={} cxf_body_len={}",
        our_body.len(),
        cxf_body.len()
    );
    eprintln!("[{name}] our_body: {}", String::from_utf8_lossy(&our_body));
    eprintln!("[{name}] cxf_body: {}", String::from_utf8_lossy(&cxf_body));

    // 5. Validate both responses against soap12-envelope schema.
    let our_valid = match oracle.validate(&our_body, "soap12-envelope") {
        Ok(r) => r.valid,
        Err(e) => return Verdict::HarnessError(format!("oracle validate our envelope: {e}")),
    };
    let cxf_valid = match oracle.validate(&cxf_body, "soap12-envelope") {
        Ok(r) => r.valid,
        Err(e) => return Verdict::HarnessError(format!("oracle validate cxf envelope: {e}")),
    };

    // 6. Determine success/fault outcome per side.
    // success = HTTP 200 (no Fault).
    let our_is_success = our_status == 200;
    let cxf_is_success = cxf_status == 200;

    // 7. For body comparison (used only when both succeed): mask Header subtree +
    //    Fault/Reason/Text (non-asserted), then C14N.
    let drop_header = vec![MaskRule::new("Envelope/Header")];
    let fault_text_masks = vec![MaskRule::new("Envelope/Body/Fault/Reason/Text")];
    let fault_attr_masks = vec![AttrMaskRule::new(
        "Envelope/Body/Fault/Reason/Text",
        "xml:lang",
    )];

    let our_masked_body = if our_valid {
        match mask_only_with_drops(
            &our_body,
            &fault_text_masks,
            &fault_attr_masks,
            &drop_header,
        ) {
            Ok(b) => b,
            Err(e) => return Verdict::HarnessError(format!("mask_only_with_drops our: {e}")),
        }
    } else {
        vec![]
    };
    let cxf_masked_body = if cxf_valid {
        match mask_only_with_drops(
            &cxf_body,
            &fault_text_masks,
            &fault_attr_masks,
            &drop_header,
        ) {
            Ok(b) => b,
            Err(e) => return Verdict::HarnessError(format!("mask_only_with_drops cxf: {e}")),
        }
    } else {
        vec![]
    };

    // C14N the masked bodies via oracle.
    let our_body_canon = if our_valid && !our_masked_body.is_empty() {
        match oracle.c14n(&our_masked_body) {
            Ok(b) => b,
            Err(e) => return Verdict::HarnessError(format!("oracle c14n our: {e}")),
        }
    } else {
        vec![]
    };
    let cxf_body_canon = if cxf_valid && !cxf_masked_body.is_empty() {
        match oracle.c14n(&cxf_masked_body) {
            Ok(b) => b,
            Err(e) => return Verdict::HarnessError(format!("oracle c14n cxf: {e}")),
        }
    } else {
        vec![]
    };

    // 8. Build Resp structs and evaluate outcome-equivalence.
    let our_resp = Resp {
        schema_valid: our_valid,
        is_success: our_is_success,
        masked_body_canon: our_body_canon.clone(),
    };
    let cxf_resp = Resp {
        schema_valid: cxf_valid,
        is_success: cxf_is_success,
        masked_body_canon: cxf_body_canon,
    };

    let v = verdict::evaluate_outcome_equivalence(&our_resp, &cxf_resp);

    eprintln!("[{name}] verdict: {v:?}");

    // 9. Promote on pass.
    if v == Verdict::Pass && promote_on_pass {
        // Use our canonical body as the evidence snapshot.
        if let Err(e) = promote(store, name, &our_body_canon) {
            eprintln!("[{name}] promotion failed: {e}");
        }
    }

    v
}

/// Run a WSDL-rewrite scenario using oracle WSDL-schema validity + structural invariant.
///
/// The authority here is NOT CXF (CXF regenerates its own WSDL, so a byte diff is
/// meaningless). Instead:
/// 1. GET `<our_base><http_path>?wsdl` from our server.
/// 2. Validate the returned WSDL via oracle `wsdl11` schema.
/// 3. Assert the rewrite invariant structurally (quick-xml parse):
///    - single (`/soap`): the `<soap:address location>` host is rewritten to the request
///      host (localhost:8080), not the original placeholder.
///    - multi (`/soap/a`): ServiceA's `<soap:address>` is rewritten to the request host
///      + path; ServiceB's `<soap:address>` is NOT clobbered (still `/soap/b`).
/// 4. Verdict: Pass if WSDL schema-valid AND invariant holds; SutFail otherwise.
///    On Pass, store the raw WSDL bytes as canonical evidence.
///
/// Host masking: for a fixed `localhost:8080` this is deterministic, so we store the
/// raw served WSDL as evidence without host masking (the location value is the
/// invariant signal, not noise).
fn run_wsdl_rewrite_scenario(
    name: &str,
    _scenarios_dir: &std::path::Path,
    endpoints: &Endpoints,
    oracle: &Oracle,
    promote_on_pass: bool,
    store: &SnapshotStore,
) -> Verdict {
    // 1. Determine which path to GET.
    //    wsdl_rewrite_single → /soap?wsdl
    //    wsdl_rewrite_multi  → /soap/a?wsdl
    let wsdl_path = match name {
        "wsdl_rewrite_single" => "/soap",
        "wsdl_rewrite_multi" => "/soap/a",
        other => return Verdict::HarnessError(format!("unknown wsdl_rewrite scenario: {other}")),
    };
    let wsdl_url = format!("{}{}?wsdl", endpoints.our_base(), wsdl_path);

    // 2. GET the WSDL.
    let wsdl_bytes = match get_bytes(&wsdl_url) {
        Ok(b) => b,
        Err(e) => return Verdict::HarnessError(format!("GET {wsdl_url}: {e}")),
    };

    eprintln!("[{name}] GET {wsdl_url} → {} bytes", wsdl_bytes.len());
    eprintln!("[{name}] WSDL:\n{}", String::from_utf8_lossy(&wsdl_bytes));

    // 3. Validate against oracle wsdl11 schema.
    let validation = match oracle.validate(&wsdl_bytes, "wsdl11") {
        Ok(r) => r,
        Err(e) => return Verdict::HarnessError(format!("oracle validate wsdl11: {e}")),
    };
    if !validation.valid {
        eprintln!("[{name}] WSDL schema-invalid: {:?}", validation.errors);
        return Verdict::SutFail(format!(
            "WSDL schema-invalid against wsdl11: {:?}",
            validation.errors
        ));
    }
    eprintln!("[{name}] WSDL schema-valid (wsdl11) ✓");

    // 4. Assert the rewrite invariant.
    let invariant_result = match name {
        "wsdl_rewrite_single" => check_single_rewrite_invariant(&wsdl_bytes, endpoints.our_base()),
        "wsdl_rewrite_multi" => check_multi_rewrite_invariant(&wsdl_bytes, endpoints.our_base()),
        _ => unreachable!(),
    };
    match invariant_result {
        Ok(()) => eprintln!("[{name}] rewrite invariant holds ✓"),
        Err(msg) => {
            eprintln!("[{name}] rewrite invariant FAILED: {msg}");
            return Verdict::SutFail(format!("rewrite invariant failed: {msg}"));
        }
    }

    // 5. Pass — promote on pass (store the raw WSDL bytes as evidence).
    if promote_on_pass {
        if let Err(e) = promote(store, name, &wsdl_bytes) {
            eprintln!("[{name}] promotion failed: {e}");
        }
    }

    Verdict::Pass
}

/// Parse the served WSDL and extract all `<soap:address location="...">` values,
/// keyed by the service name containing that port.
///
/// Returns a list of `(service_name, location_value)` pairs.
fn extract_soap_addresses(wsdl: &[u8]) -> Result<Vec<(String, String)>, String> {
    use quick_xml::events::Event;
    use quick_xml::Reader;

    let mut reader = Reader::from_reader(wsdl);
    reader.config_mut().trim_text(false);
    let mut buf = Vec::new();

    let mut addresses: Vec<(String, String)> = Vec::new();
    // Track the current service name.
    let mut current_service: Option<String> = Vec::new().into_iter().next();
    let mut in_service = false;

    loop {
        match reader
            .read_event_into(&mut buf)
            .map_err(|e| e.to_string())?
        {
            Event::Start(ref e) | Event::Empty(ref e) => {
                let raw = e.name();
                let bytes = raw.as_ref();
                let colon_pos = bytes.iter().rposition(|&b| b == b':');
                let local = match colon_pos {
                    Some(i) => &bytes[i + 1..],
                    None => bytes,
                };

                if local == b"service" {
                    // Extract the name attribute.
                    for attr in e.attributes().flatten() {
                        if attr.key.as_ref() == b"name" {
                            current_service =
                                Some(String::from_utf8_lossy(&attr.value).into_owned());
                        }
                    }
                    in_service = true;
                } else if local == b"address" && in_service {
                    // Extract location attribute.
                    for attr in e.attributes().flatten() {
                        if attr.key.as_ref() == b"location" {
                            let location = String::from_utf8_lossy(&attr.value).into_owned();
                            if let Some(ref svc) = current_service {
                                addresses.push((svc.clone(), location));
                            }
                        }
                    }
                }
            }
            Event::End(ref e) => {
                let raw = e.name();
                let bytes = raw.as_ref();
                let local = match bytes.iter().rposition(|&b| b == b':') {
                    Some(i) => &bytes[i + 1..],
                    None => bytes,
                };
                if local == b"service" {
                    in_service = false;
                    current_service = None;
                }
            }
            Event::Eof => break,
            _ => {}
        }
        buf.clear();
    }
    Ok(addresses)
}

/// For the single-service scenario: the one `<soap:address location>` must have its
/// host rewritten to the request host (our server's base URL host+port), not the
/// original `localhost` placeholder (which has no port in the fixture).
fn check_single_rewrite_invariant(wsdl: &[u8], our_base: &str) -> Result<(), String> {
    let addresses = extract_soap_addresses(wsdl)?;
    if addresses.is_empty() {
        return Err("no <soap:address> elements found in WSDL".to_string());
    }
    // Extract the host+port from our_base (e.g. "http://localhost:8080" → "localhost:8080").
    let expected_host = our_base
        .trim_start_matches("http://")
        .trim_start_matches("https://")
        .split('/')
        .next()
        .unwrap_or("");

    for (svc, location) in &addresses {
        // The location host must contain our request host (with port).
        let location_host = location
            .trim_start_matches("http://")
            .trim_start_matches("https://")
            .split('/')
            .next()
            .unwrap_or("");
        if location_host != expected_host {
            return Err(format!(
                "service '{svc}' address '{location}' host is '{location_host}', \
                 expected '{expected_host}' (rewrite invariant failed)"
            ));
        }
    }
    eprintln!(
        "  single rewrite: {} address(es), all host='{expected_host}'",
        addresses.len()
    );
    Ok(())
}

/// For the multi-service scenario (`/soap/a` request):
/// - ServiceA's `<soap:address>` must be rewritten to the request host+path.
/// - ServiceB's `<soap:address>` must be preserved (not clobbered to `/soap/a`).
fn check_multi_rewrite_invariant(wsdl: &[u8], our_base: &str) -> Result<(), String> {
    let addresses = extract_soap_addresses(wsdl)?;

    let expected_host = our_base
        .trim_start_matches("http://")
        .trim_start_matches("https://")
        .split('/')
        .next()
        .unwrap_or("");

    let mut found_a = false;
    let mut found_b = false;

    for (svc, location) in &addresses {
        let location_host = location
            .trim_start_matches("http://")
            .trim_start_matches("https://")
            .split('/')
            .next()
            .unwrap_or("");

        if svc.contains('A') || svc.to_lowercase().contains("servicea") {
            // ServiceA: must have been rewritten to the request host.
            if location_host != expected_host {
                return Err(format!(
                    "ServiceA address '{location}' host is '{location_host}', \
                     expected '{expected_host}' (ServiceA rewrite invariant failed)"
                ));
            }
            // Must also point to /soap/a path.
            if !location.contains("/soap/a") {
                return Err(format!(
                    "ServiceA address '{location}' does not contain '/soap/a' \
                     (expected rewritten path)"
                ));
            }
            eprintln!("  ServiceA address rewritten → '{location}' ✓");
            found_a = true;
        } else if svc.contains('B') || svc.to_lowercase().contains("serviceb") {
            // ServiceB: must NOT have been clobbered. Its path must still be /soap/b.
            if !location.contains("/soap/b") {
                return Err(format!(
                    "ServiceB address '{location}' does not contain '/soap/b' \
                     (non-matched service address was clobbered — BLOCK-OS-C08-class finding)"
                ));
            }
            eprintln!("  ServiceB address preserved → '{location}' ✓");
            found_b = true;
        }
    }

    if !found_a {
        return Err(format!(
            "ServiceA not found in WSDL addresses: {addresses:?}"
        ));
    }
    if !found_b {
        return Err(format!(
            "ServiceB not found in WSDL addresses: {addresses:?}"
        ));
    }
    Ok(())
}

/// GET bytes from `url`. Returns `(body_bytes)`.
fn get_bytes(url: &str) -> Result<Vec<u8>, String> {
    let client = reqwest::blocking::Client::new();
    let resp = client.get(url).send().map_err(|e| e.to_string())?;
    let status = resp.status().as_u16();
    let bytes = resp.bytes().map_err(|e| e.to_string())?.to_vec();
    if status != 200 {
        return Err(format!(
            "GET {url} returned HTTP {status}: {}",
            String::from_utf8_lossy(&bytes)
        ));
    }
    Ok(bytes)
}

/// POST XML bytes to `url` with the given Content-Type header.
/// Returns `(status_code, body_bytes)`.
fn post(url: &str, body: &[u8], content_type: &str) -> Result<(u16, Vec<u8>), String> {
    let client = reqwest::blocking::Client::new();
    let resp = client
        .post(url)
        .header("Content-Type", content_type)
        .body(body.to_vec())
        .send()
        .map_err(|e| e.to_string())?;
    let status = resp.status().as_u16();
    let bytes = resp.bytes().map_err(|e| e.to_string())?.to_vec();
    Ok((status, bytes))
}

/// Extract the first child element of `soap:Body` from the full envelope bytes.
/// Returns the child's full bytes including its namespace declarations.
/// Uses quick-xml to find the Body element (by local name) and captures the first child element.
fn extract_body_child(envelope: &[u8]) -> Option<Vec<u8>> {
    use quick_xml::events::Event;
    use quick_xml::Reader;

    let mut reader = Reader::from_reader(envelope);
    reader.config_mut().trim_text(false);
    let mut buf = Vec::new();

    // Simple state machine: track whether we're inside Body, and collect child bytes.
    let mut in_body = false;
    let mut collecting = false;
    let mut depth: usize = 0;
    let mut out: Vec<u8> = Vec::new();

    while let Ok(evt) = reader.read_event_into(&mut buf) {
        match evt {
            Event::Eof => break,

            Event::Start(ref e) => {
                let raw = e.name();
                let name_bytes: Vec<u8> = raw.as_ref().to_vec();
                let local_start = name_bytes
                    .iter()
                    .rposition(|&b| b == b':')
                    .map(|i| i + 1)
                    .unwrap_or(0);
                let local = &name_bytes[local_start..];

                if !in_body {
                    if local == b"Body" {
                        in_body = true;
                    }
                } else if !collecting {
                    // First child of Body — start collecting.
                    collecting = true;
                    depth = 1;
                    out.clear();
                    out.push(b'<');
                    out.extend_from_slice(&name_bytes);
                    for attr in e.attributes().flatten() {
                        out.push(b' ');
                        out.extend_from_slice(attr.key.as_ref());
                        out.extend_from_slice(b"=\"");
                        out.extend_from_slice(&attr.value);
                        out.push(b'"');
                    }
                    out.push(b'>');
                } else {
                    // Nested element inside child.
                    depth += 1;
                    out.push(b'<');
                    out.extend_from_slice(&name_bytes);
                    for attr in e.attributes().flatten() {
                        out.push(b' ');
                        out.extend_from_slice(attr.key.as_ref());
                        out.extend_from_slice(b"=\"");
                        out.extend_from_slice(&attr.value);
                        out.push(b'"');
                    }
                    out.push(b'>');
                }
            }

            Event::Empty(ref e) => {
                let raw = e.name();
                let name_bytes: Vec<u8> = raw.as_ref().to_vec();

                if in_body && !collecting {
                    // Empty first child of Body — return immediately.
                    let mut result = Vec::new();
                    result.push(b'<');
                    result.extend_from_slice(&name_bytes);
                    for attr in e.attributes().flatten() {
                        result.push(b' ');
                        result.extend_from_slice(attr.key.as_ref());
                        result.extend_from_slice(b"=\"");
                        result.extend_from_slice(&attr.value);
                        result.push(b'"');
                    }
                    result.extend_from_slice(b"/>");
                    return Some(result);
                } else if collecting {
                    out.push(b'<');
                    out.extend_from_slice(&name_bytes);
                    for attr in e.attributes().flatten() {
                        out.push(b' ');
                        out.extend_from_slice(attr.key.as_ref());
                        out.extend_from_slice(b"=\"");
                        out.extend_from_slice(&attr.value);
                        out.push(b'"');
                    }
                    out.extend_from_slice(b"/>");
                }
            }

            Event::End(ref e) => {
                let raw = e.name();
                let name_bytes: Vec<u8> = raw.as_ref().to_vec();

                if collecting {
                    if depth == 1 {
                        // Closing our collected child element.
                        out.extend_from_slice(b"</");
                        out.extend_from_slice(&name_bytes);
                        out.push(b'>');
                        return Some(std::mem::take(&mut out));
                    } else {
                        depth -= 1;
                        out.extend_from_slice(b"</");
                        out.extend_from_slice(&name_bytes);
                        out.push(b'>');
                    }
                }
                // End of Body (in_body but not collecting) — nothing to do.
            }

            Event::Text(ref t) => {
                if collecting {
                    out.extend_from_slice(t.as_ref());
                }
            }

            _ => {}
        }
        buf.clear();
    }
    None
}
