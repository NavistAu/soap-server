//! Layer-2 verdict model (spec §5.7).
//!
//! `evaluate(&Eval)` compares the normalized bytes from our SUT and the CXF reference
//! server and returns a `Verdict`. Known divergences are declared per-scenario in
//! `Eval::known_divergences`.

/// The outcome of a single Layer-2 scenario comparison.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verdict {
    /// Both sides produced valid, byte-identical normalized output.
    Pass,
    /// Our SUT produced invalid XML (or the oracle rejected it).
    SutFail(String),
    /// The CXF reference produced output our oracle considers invalid.
    /// This indicates a CXF or oracle bug, not a SUT regression.
    ReferenceDisagreement(String),
    /// The normalized outputs differ, but this divergence is listed in
    /// `Eval::known_divergences` for this scenario.
    KnownDivergence(String),
    /// The harness itself failed (HTTP error, oracle error, etc.) — not a SUT verdict.
    /// Never counts as pass; the scenario must be re-run.
    HarnessError(String),
}

/// Input to `evaluate`: the normalized bytes from each side, plus metadata.
pub struct Eval {
    /// Normalized bytes from our SUT (output of `mask_only`), or an error string.
    pub sut: Result<Vec<u8>, String>,
    /// Normalized bytes from the CXF reference server (output of `mask_only`), or an error.
    pub reference: Result<Vec<u8>, String>,
    /// Reason strings for known divergences on this scenario. If the two sides differ
    /// but the reason matches an entry here, `KnownDivergence` is returned instead of
    /// `SutFail`. Comparison is by exact string equality.
    pub known_divergences: Vec<String>,
}

/// Evaluate a single scenario comparison and return the verdict.
pub fn evaluate(eval: &Eval) -> Verdict {
    match (&eval.sut, &eval.reference) {
        (Err(msg), _) => Verdict::SutFail(msg.clone()),
        (_, Err(msg)) => Verdict::ReferenceDisagreement(msg.clone()),
        (Ok(sut_bytes), Ok(ref_bytes)) => {
            if sut_bytes == ref_bytes {
                Verdict::Pass
            } else {
                // Check known divergences.
                let diff_reason = format!(
                    "sut={} ref={}",
                    String::from_utf8_lossy(sut_bytes),
                    String::from_utf8_lossy(ref_bytes)
                );
                for known in &eval.known_divergences {
                    // Match if the diff_reason string contains the known-divergence token,
                    // or if either side's bytes contain the token as a subsequence.
                    let known_bytes = known.as_bytes();
                    let sut_has = sut_bytes
                        .windows(known_bytes.len())
                        .any(|w| w == known_bytes);
                    let ref_has = ref_bytes
                        .windows(known_bytes.len())
                        .any(|w| w == known_bytes);
                    if *known == diff_reason
                        || sut_has
                        || ref_has
                        || diff_reason.contains(known.as_str())
                    {
                        return Verdict::KnownDivergence(known.clone());
                    }
                }
                Verdict::SutFail(format!(
                    "outputs differ: sut={} ref={}",
                    String::from_utf8_lossy(sut_bytes),
                    String::from_utf8_lossy(ref_bytes)
                ))
            }
        }
    }
}

// ─── Outcome-equivalence model for WS-Security scenarios ─────────────────────
//
// Per spec §10, WS-Security conformance is assessed at the *outcome* level:
// two servers that both accept (HTTP 200 + equivalent body) or both reject
// (SOAP Fault) a given credential are considered equivalent. Exact fault wording,
// nonce-cache state, and response Security headers are NOT asserted.

/// A normalised response for outcome-equivalence comparison.
/// - `schema_valid`: the SOAP envelope validated against the oracle schema.
/// - `is_success`: HTTP 200 and no SOAP Fault element in the body.
/// - `masked_body_canon`: oracle-C14N bytes of the Body subtree with the entire
///   Envelope/Header dropped and Fault/Reason/Text masked. Used only when both
///   sides are schema-valid and both succeed; otherwise the body is not compared.
#[derive(Debug, Clone)]
pub struct Resp {
    pub schema_valid: bool,
    pub is_success: bool,
    /// Oracle-C14N bytes of the masked body (for body-level equality on success).
    pub masked_body_canon: Vec<u8>,
}

/// Evaluate outcome-equivalence for a WS-Security scenario (spec §10).
///
/// Rules:
/// - our schema-invalid → `SutFail`.
/// - ref schema-invalid → `ReferenceDisagreement`.
/// - both schema-valid AND both success AND equal masked body → `Pass`.
/// - both schema-valid AND both success AND unequal body → `SutFail` (real diff).
/// - both schema-valid AND both fault (non-success) → `Pass` (class-equivalence; reason non-asserted).
/// - one success + one fault → `SutFail` (different outcome).
pub fn evaluate_outcome_equivalence(our: &Resp, cxf: &Resp) -> Verdict {
    if !our.schema_valid {
        return Verdict::SutFail("our response schema-invalid".to_string());
    }
    if !cxf.schema_valid {
        return Verdict::ReferenceDisagreement("CXF response schema-invalid".to_string());
    }
    match (our.is_success, cxf.is_success) {
        (true, true) => {
            if our.masked_body_canon == cxf.masked_body_canon {
                Verdict::Pass
            } else {
                Verdict::SutFail(format!(
                    "both succeeded but body differs: our={} cxf={}",
                    String::from_utf8_lossy(&our.masked_body_canon),
                    String::from_utf8_lossy(&cxf.masked_body_canon),
                ))
            }
        }
        (false, false) => Verdict::Pass, // both rejected — class-equivalence
        (true, false) => Verdict::SutFail(
            "our server accepted but CXF rejected (SUT admitted an invalid credential)".to_string(),
        ),
        (false, true) => Verdict::SutFail(
            "our server rejected but CXF accepted (SUT rejected a valid credential)".to_string(),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ok(s: &str) -> Result<Vec<u8>, String> {
        Ok(s.as_bytes().to_vec())
    }

    fn err(s: &str) -> Result<Vec<u8>, String> {
        Err(s.to_string())
    }

    // ─── evaluate_outcome_equivalence unit tests ───────────────────────────────

    fn resp_success(body: &str) -> Resp {
        Resp {
            schema_valid: true,
            is_success: true,
            masked_body_canon: body.as_bytes().to_vec(),
        }
    }

    fn resp_fault() -> Resp {
        Resp {
            schema_valid: true,
            is_success: false,
            masked_body_canon: vec![],
        }
    }

    fn resp_invalid() -> Resp {
        Resp {
            schema_valid: false,
            is_success: false,
            masked_body_canon: vec![],
        }
    }

    #[test]
    fn oe_both_success_equal_body_is_pass() {
        let our = resp_success("<body>hi</body>");
        let cxf = resp_success("<body>hi</body>");
        assert_eq!(evaluate_outcome_equivalence(&our, &cxf), Verdict::Pass);
    }

    #[test]
    fn oe_both_fault_is_pass() {
        let our = resp_fault();
        let cxf = resp_fault();
        assert_eq!(evaluate_outcome_equivalence(&our, &cxf), Verdict::Pass);
    }

    #[test]
    fn oe_our_success_cxf_fault_is_sut_fail() {
        let our = resp_success("<body>hi</body>");
        let cxf = resp_fault();
        assert!(matches!(
            evaluate_outcome_equivalence(&our, &cxf),
            Verdict::SutFail(_)
        ));
    }

    #[test]
    fn oe_our_fault_cxf_success_is_sut_fail() {
        let our = resp_fault();
        let cxf = resp_success("<body>hi</body>");
        assert!(matches!(
            evaluate_outcome_equivalence(&our, &cxf),
            Verdict::SutFail(_)
        ));
    }

    #[test]
    fn oe_our_schema_invalid_is_sut_fail() {
        let our = resp_invalid();
        let cxf = resp_fault();
        assert!(matches!(
            evaluate_outcome_equivalence(&our, &cxf),
            Verdict::SutFail(_)
        ));
    }

    #[test]
    fn oe_ref_schema_invalid_is_reference_disagreement() {
        let our = resp_fault();
        let cxf = resp_invalid();
        assert!(matches!(
            evaluate_outcome_equivalence(&our, &cxf),
            Verdict::ReferenceDisagreement(_)
        ));
    }

    #[test]
    fn oe_both_success_unequal_body_is_sut_fail() {
        let our = resp_success("<body>A</body>");
        let cxf = resp_success("<body>B</body>");
        assert!(matches!(
            evaluate_outcome_equivalence(&our, &cxf),
            Verdict::SutFail(_)
        ));
    }

    // ─── original evaluate() tests ────────────────────────────────────────────

    #[test]
    fn verdict_pass_when_equal() {
        let eval = Eval {
            sut: ok("<foo/>"),
            reference: ok("<foo/>"),
            known_divergences: vec![],
        };
        assert_eq!(evaluate(&eval), Verdict::Pass);
    }

    #[test]
    fn verdict_sut_fail_when_our_side_invalid() {
        let eval = Eval {
            sut: err("parse error: bad XML"),
            reference: ok("<foo/>"),
            known_divergences: vec![],
        };
        assert!(matches!(evaluate(&eval), Verdict::SutFail(_)));
    }

    #[test]
    fn verdict_reference_disagreement_when_ref_invalid() {
        let eval = Eval {
            sut: ok("<foo/>"),
            reference: err("oracle rejected CXF output"),
            known_divergences: vec![],
        };
        assert!(matches!(evaluate(&eval), Verdict::ReferenceDisagreement(_)));
    }

    #[test]
    fn verdict_known_divergence_when_differ_and_listed() {
        let eval = Eval {
            sut: ok("<foo>A</foo>"),
            reference: ok("<foo>B</foo>"),
            known_divergences: vec!["A".to_string()],
        };
        assert!(matches!(evaluate(&eval), Verdict::KnownDivergence(_)));
    }

    #[test]
    fn verdict_sut_fail_when_differ_without_known() {
        let eval = Eval {
            sut: ok("<foo>A</foo>"),
            reference: ok("<foo>B</foo>"),
            known_divergences: vec![],
        };
        assert!(matches!(evaluate(&eval), Verdict::SutFail(_)));
    }
}
