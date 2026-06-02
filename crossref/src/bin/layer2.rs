//! Layer-2 entrypoint. Runs UNSANDBOXED (CI or explicit local). Not part of per-commit CI.
//!
//! Usage: cargo run -p crossref --bin layer2 -- [--promote] [--keep-up]
//!
//! --promote   flip promoted scenarios to "verified" and write oracle-canonical evidence.
//! --keep-up   leave the docker compose topology running after the run (useful for debugging).
//!
//! The topology (controlled-server + cxf + oracle) is brought up before the run
//! and torn down after (unless --keep-up). Port-publishing is via docker-compose.local.yml.

use crossref::layer2::{compose::Topology, run, Endpoints};
use std::path::Path;

fn main() {
    let promote = std::env::args().any(|a| a == "--promote");
    let keep_up = std::env::args().any(|a| a == "--keep-up");
    let root = Path::new(".");

    eprintln!("Layer-2 conformance run starting (promote={promote}, keep_up={keep_up})");

    let _topo = match Topology::up(root, keep_up) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("FATAL: topology up failed: {e}");
            std::process::exit(2);
        }
    };

    let endpoints = Endpoints::localhost();
    let report = run(&endpoints, root, promote);
    report.print();
    std::process::exit(if report.is_green() { 0 } else { 1 });
}
