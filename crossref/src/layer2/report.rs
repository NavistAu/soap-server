//! Per-scenario verdict report (spec §5.2, §7). Surfaces the count of still-unverified
//! snapshots so self-captured baselines are never mistaken for conformance evidence.

use crate::layer2::verdict::Verdict;

pub struct Report {
    pub rows: Vec<(String, Verdict)>,
    pub unverified_remaining: usize,
}

impl Report {
    pub fn print(&self) {
        for (name, v) in &self.rows {
            println!("{name:45} {v:?}");
        }
        let pass = self
            .rows
            .iter()
            .filter(|(_, v)| matches!(v, Verdict::Pass))
            .count();
        let fail = self
            .rows
            .iter()
            .filter(|(_, v)| matches!(v, Verdict::SutFail(_) | Verdict::HarnessError(_)))
            .count();
        println!(
            "\n{} scenario(s) run: {} Pass, {} Fail/Error; {} snapshot(s) still unverified (conformance pending)",
            self.rows.len(),
            pass,
            fail,
            self.unverified_remaining
        );
    }

    /// Returns false if any SutFail or HarnessError is present.
    pub fn is_green(&self) -> bool {
        !self
            .rows
            .iter()
            .any(|(_, v)| matches!(v, Verdict::SutFail(_) | Verdict::HarnessError(_)))
    }
}
