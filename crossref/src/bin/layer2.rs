//! Layer-2 entrypoint. Runs UNSANDBOXED (CI or explicit local). Not part of per-commit CI.
//!
//! Usage: cargo run -p crossref --bin layer2 -- [--promote] [--keep-up] [--scenarios <csv>]
//!
//! --promote              flip promoted scenarios to "verified" and write oracle-canonical evidence.
//! --keep-up              leave the docker compose topology running after the run (useful for debugging).
//! --scenarios <csv>      run only the listed scenario names (comma-separated); default: all in-scope.

use crossref::layer2::{compose::Topology, run, Endpoints};
use std::path::Path;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let promote = args.iter().any(|a| a == "--promote");
    let keep_up = args.iter().any(|a| a == "--keep-up");

    // Parse optional --scenarios <csv> flag.
    let scenarios_filter: Option<Vec<String>> = args
        .iter()
        .position(|a| a == "--scenarios")
        .and_then(|i| args.get(i + 1))
        .map(|csv| csv.split(',').map(|s| s.trim().to_string()).collect());

    let root = Path::new(".");

    eprintln!(
        "Layer-2 conformance run starting (promote={promote}, keep_up={keep_up}, scenarios={scenarios_filter:?})"
    );

    let _topo = match Topology::up(root, keep_up) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("FATAL: topology up failed: {e}");
            std::process::exit(2);
        }
    };

    let endpoints = Endpoints::localhost();
    let report = run(&endpoints, root, promote, scenarios_filter.as_deref());
    report.print();
    std::process::exit(if report.is_green() { 0 } else { 1 });
}
