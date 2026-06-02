//! Layer-2 entrypoint. Runs UNSANDBOXED (CI or explicit local). Not part of per-commit CI.
//!
//! Usage: cargo run -p crossref --bin layer2 -- [--promote] [--keep-up] [--scenarios <csv>] [--interop]
//!
//! --promote              flip promoted scenarios to "verified" and write oracle-canonical evidence.
//! --keep-up              leave the docker compose topology running after the run (useful for debugging).
//! --scenarios <csv>      run only the listed scenario names (comma-separated); default: all in-scope.
//!                        Special value "__none__" skips all conformance scenarios (interop only).
//! --interop              after the conformance run, drive the interop client containers
//!                        (cxf-client + zeep-client) against our controlled server.

use crossref::layer2::{compose::Topology, interop, run, Endpoints};
use crossref::oracle::Oracle;
use crossref::snapshot::SnapshotStore;
use std::path::Path;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let promote = args.iter().any(|a| a == "--promote");
    let keep_up = args.iter().any(|a| a == "--keep-up");
    let run_interop = args.iter().any(|a| a == "--interop");

    // Parse optional --scenarios <csv> flag.
    // Special value "__none__" means skip all conformance scenarios (run interop only).
    let scenarios_filter: Option<Vec<String>> = args
        .iter()
        .position(|a| a == "--scenarios")
        .and_then(|i| args.get(i + 1))
        .map(|csv| {
            if csv == "__none__" {
                // Return an empty list — no conformance scenario will match.
                vec![]
            } else {
                csv.split(',').map(|s| s.trim().to_string()).collect()
            }
        });

    let root = Path::new(".");

    eprintln!(
        "Layer-2 run starting (promote={promote}, keep_up={keep_up}, \
         scenarios={scenarios_filter:?}, interop={run_interop})"
    );

    let _topo = match Topology::up(root, keep_up) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("FATAL: topology up failed: {e}");
            std::process::exit(2);
        }
    };

    let endpoints = Endpoints::localhost();
    let snapshots_dir = root.join("crossref/snapshots");
    let store = SnapshotStore::new(&snapshots_dir);
    let oracle = Oracle::new(&endpoints.oracle);

    // Run conformance scenarios.
    let mut report = run(&endpoints, root, promote, scenarios_filter.as_deref());

    // Run interop clients if --interop flag is set.
    if run_interop {
        eprintln!("Running interop clients ...");
        let interop_verdicts = interop::run_interop(root, &oracle, &store, promote);
        // Merge interop verdicts into the report.
        for (name, verdict) in interop_verdicts {
            report.rows.push((name, verdict));
        }
    }

    report.print();

    std::process::exit(if report.is_green() { 0 } else { 1 });
}
