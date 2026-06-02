//! Layer-2 orchestration: compose lifecycle, endpoints, verdict, promotion, report.

pub mod compose;
pub mod promote;
pub mod report;
pub mod verdict;

use crate::normalize::{mask_only, AttrMaskRule, MaskRule};
use crate::oracle::Oracle;
use crate::scenario::{Outcome, Scenario};
use crate::snapshot::SnapshotStore;
use promote::promote;
use report::Report;
use verdict::{Eval, Verdict};

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
}

/// The 12 in-scope conformance scenarios for Phase 1b.
const IN_SCOPE: &[&str] = &[
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
];

/// Drive all 12 in-scope conformance scenarios, return the verdict report.
///
/// For each scenario:
/// 1. POST the request to both servers.
/// 2. Validate both responses via the oracle.
/// 3. Apply masks → oracle C14N → compare.
/// 4. On Pass + promote_on_pass: write canonical evidence + flip status.
pub fn run(endpoints: &Endpoints, repo_root: &std::path::Path, promote_on_pass: bool) -> Report {
    let scenarios_dir = repo_root.join("crossref/scenarios");
    let snapshots_dir = repo_root.join("crossref/snapshots");
    let store = SnapshotStore::new(&snapshots_dir);
    let oracle = Oracle::new(&endpoints.oracle);

    let mut rows: Vec<(String, Verdict)> = Vec::new();

    for &name in IN_SCOPE {
        let verdict = run_scenario(
            name,
            &scenarios_dir,
            &endpoints.our,
            &endpoints.cxf,
            &oracle,
            promote_on_pass,
            &store,
        );
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
    our_url: &str,
    cxf_url: &str,
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

    // 2. Read request bytes.
    let request_path = scenarios_dir.join(&scenario.request_file);
    let request_bytes = match std::fs::read(&request_path) {
        Ok(b) => b,
        Err(e) => {
            return Verdict::HarnessError(format!("read request {}: {e}", scenario.request_file))
        }
    };

    // 3. POST to both servers.
    let (our_status, our_body) = match post(our_url, &request_bytes, &scenario.content_type) {
        Ok(r) => r,
        Err(e) => return Verdict::HarnessError(format!("POST our server: {e}")),
    };
    let (cxf_status, cxf_body) = match post(cxf_url, &request_bytes, &scenario.content_type) {
        Ok(r) => r,
        Err(e) => return Verdict::HarnessError(format!("POST CXF: {e}")),
    };

    // Log status codes for diagnosis.
    eprintln!(
        "[{name}] our={our_status} cxf={cxf_status} our_body_len={} cxf_body_len={}",
        our_body.len(),
        cxf_body.len()
    );

    // 4. Validate both responses via oracle.
    // 4a. Envelope schema validation (all scenarios).
    let our_env_valid = match oracle.validate(&our_body, "soap12-envelope") {
        Ok(r) => r,
        Err(e) => return Verdict::HarnessError(format!("oracle validate our envelope: {e}")),
    };
    let cxf_env_valid = match oracle.validate(&cxf_body, "soap12-envelope") {
        Ok(r) => r,
        Err(e) => return Verdict::HarnessError(format!("oracle validate cxf envelope: {e}")),
    };

    // 4b. Body child validation for SUCCESS scenarios only.
    let (our_body_valid, our_body_errors) = if scenario.outcome == Outcome::Success {
        match extract_body_child(&our_body) {
            Some(child) => match oracle.validate(&child, "controlled") {
                Ok(r) => (r.valid, r.errors),
                Err(e) => {
                    return Verdict::HarnessError(format!("oracle validate our body child: {e}"))
                }
            },
            None => {
                // No body child found — treat as validation failure.
                (
                    false,
                    vec!["no body child element found in our response".to_string()],
                )
            }
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

    // 5. Build per-scenario masks.
    // FAULT scenarios: mask Reason/Text content and xml:lang attr (non-asserted per spec §10).
    let (text_masks, attr_masks): (Vec<MaskRule>, Vec<AttrMaskRule>) =
        if scenario.outcome == Outcome::Fault {
            (
                vec![MaskRule::new("Envelope/Body/Fault/Reason/Text")],
                vec![AttrMaskRule::new(
                    "Envelope/Body/Fault/Reason/Text",
                    "xml:lang",
                )],
            )
        } else {
            (vec![], vec![])
        };

    // 6. Mask + oracle C14N.
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

    // 7. Evaluate verdict using existing evaluate() with Result-based Eval.
    // We build Result<Vec<u8>, String> for each side: Err if invalid, Ok(canon) if valid.
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

    // 8. Promote on pass.
    if v == Verdict::Pass && promote_on_pass {
        if let Err(e) = promote(store, name, &our_canon) {
            eprintln!("[{name}] promotion failed: {e}");
        }
    }

    v
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
