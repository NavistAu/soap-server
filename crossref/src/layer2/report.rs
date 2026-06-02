//! Per-scenario verdict report (spec §5.2, §7). Surfaces conformance and interop verdicts
//! separately, plus the count of still-unverified snapshots so self-captured baselines are
//! never mistaken for conformance evidence. Target after Phase 1c: 0 unverified.

use crate::layer2::verdict::Verdict;

pub struct Report {
    pub rows: Vec<(String, Verdict)>,
    pub unverified_remaining: usize,
}

impl Report {
    pub fn print(&self) {
        // Split rows into conformance (non-interop) and interop (prefix "interop_").
        let conformance: Vec<_> = self
            .rows
            .iter()
            .filter(|(name, _)| !name.starts_with("interop_"))
            .collect();
        let interop: Vec<_> = self
            .rows
            .iter()
            .filter(|(name, _)| name.starts_with("interop_"))
            .collect();

        // --- Conformance verdicts ---
        println!("=== Conformance ({} scenarios) ===", conformance.len());
        for (name, v) in &conformance {
            println!("  {name:43} {v:?}");
        }
        let conf_pass = conformance
            .iter()
            .filter(|(_, v)| matches!(v, Verdict::Pass | Verdict::KnownDivergence(_)))
            .count();
        let conf_fail = conformance
            .iter()
            .filter(|(_, v)| matches!(v, Verdict::SutFail(_) | Verdict::HarnessError(_)))
            .count();
        println!("  → {conf_pass} Pass/KnownDivergence, {conf_fail} Fail/Error\n");

        // --- Interop verdicts ---
        println!("=== Interop ({} scenarios) ===", interop.len());
        for (name, v) in &interop {
            println!("  {name:43} {v:?}");
        }
        let interop_pass = interop
            .iter()
            .filter(|(_, v)| matches!(v, Verdict::Pass | Verdict::KnownDivergence(_)))
            .count();
        let interop_fail = interop
            .iter()
            .filter(|(_, v)| matches!(v, Verdict::SutFail(_) | Verdict::HarnessError(_)))
            .count();
        println!("  → {interop_pass} Pass/KnownDivergence, {interop_fail} Fail/Error\n");

        // --- Summary ---
        let total = self.rows.len();
        let total_pass = conf_pass + interop_pass;
        let total_fail = conf_fail + interop_fail;
        println!(
            "=== Summary: {total} total ({conf} conformance + {interop} interop) ===",
            conf = conformance.len(),
            interop = interop.len()
        );
        println!("  {total_pass} Pass/KnownDivergence, {total_fail} Fail/Error");
        println!(
            "  {} snapshot(s) still unverified (target: 0 after Phase 1)",
            self.unverified_remaining
        );
        if self.is_green() && self.unverified_remaining == 0 {
            println!("  ✓ All seed scenarios verified — Phase 1 complete (spec §11.5)");
        } else if !self.is_green() {
            let bad = self
                .rows
                .iter()
                .filter(|(_, v)| {
                    matches!(
                        v,
                        Verdict::SutFail(_)
                            | Verdict::HarnessError(_)
                            | Verdict::ReferenceDisagreement(_)
                    )
                })
                .count();
            println!("  ✗ run has {bad} Fail/Error verdict(s) — NOT release-green");
        }
    }

    /// Returns true only when every verdict is Pass or KnownDivergence (spec §11.5 green gate).
    ///
    /// SutFail, HarnessError, AND ReferenceDisagreement all make the run non-green:
    /// a ReferenceDisagreement means the scenario is NOT verified and requires triage
    /// before a release decision can be made.
    pub fn is_green(&self) -> bool {
        self.rows
            .iter()
            .all(|(_, v)| matches!(v, Verdict::Pass | Verdict::KnownDivergence(_)))
    }
}
