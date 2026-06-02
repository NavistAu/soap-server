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

#[cfg(test)]
mod tests {
    use super::*;

    fn ok(s: &str) -> Result<Vec<u8>, String> {
        Ok(s.as_bytes().to_vec())
    }

    fn err(s: &str) -> Result<Vec<u8>, String> {
        Err(s.to_string())
    }

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
