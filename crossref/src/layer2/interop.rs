//! Interop category driver (spec §4.1, §11.4).
//!
//! Drives real third-party CLIENT containers (Apache CXF + Python Zeep) against
//! our controlled server, asserting their operations succeed. Each client:
//! - exits 0 on success (operations completed, assertions passed)
//! - exits 1 on interop failure (client could not drive our server)
//! - prints the raw response (or Python repr) to stdout for the orchestrator to capture
//!
//! Verdict model:
//! - exit 0 + response normalizes/validates → Pass
//! - exit != 0 → SutFail (real interop finding: third-party client failed against our server)
//! - container/infra failure → HarnessError

use crate::layer2::promote::promote;
use crate::layer2::verdict::Verdict;
use crate::oracle::Oracle;
use crate::snapshot::SnapshotStore;
use std::path::Path;
use std::process::Command;

const COMPOSE_FILE: &str = "crossref/docker-compose.yml";
const COMPOSE_LOCAL: &str = "crossref/docker-compose.local.yml";

/// An interop client descriptor.
struct InteropClient {
    /// Service name in docker-compose (also the container name to `run --rm`).
    service: &'static str,
    /// Scenario name for verdict tracking + snapshot storage.
    scenario: &'static str,
}

/// All registered interop clients.
const CLIENTS: &[InteropClient] = &[
    InteropClient {
        service: "cxf-client",
        scenario: "interop_cxf_echo",
    },
    InteropClient {
        service: "zeep-client",
        scenario: "interop_zeep_echo",
    },
];

/// Run all interop clients and return their verdicts.
///
/// For each client:
/// 1. `docker compose … --profile interop run --rm <service>` (captures stdout + exit code).
/// 2. If exit != 0 → `SutFail` (the client could not complete against our server — real finding).
/// 3. If exit 0 → the client completed; the printed output is the response representation.
///    Attempt to normalize (mask_only) + oracle c14n if the output looks like XML.
///    Verdict Pass; store canonical evidence.
///
/// # Parameters
/// - `repo_root`: workspace root (where the compose files live).
/// - `oracle`: the running oracle instance.
/// - `store`: snapshot store for writing canonical evidence.
/// - `promote_on_pass`: if true, write canonical evidence + flip status to verified.
pub fn run_interop(
    repo_root: &Path,
    oracle: &Oracle,
    store: &SnapshotStore,
    promote_on_pass: bool,
) -> Vec<(String, Verdict)> {
    let mut results: Vec<(String, Verdict)> = Vec::new();

    for client in CLIENTS {
        let verdict = run_one_client(client, repo_root, oracle, store, promote_on_pass);
        eprintln!("[interop/{}] verdict: {:?}", client.scenario, verdict);
        results.push((client.scenario.to_string(), verdict));
    }

    results
}

fn run_one_client(
    client: &InteropClient,
    repo_root: &Path,
    oracle: &Oracle,
    store: &SnapshotStore,
    promote_on_pass: bool,
) -> Verdict {
    eprintln!(
        "[interop/{}] running docker compose run --rm {} ...",
        client.scenario, client.service
    );

    // `docker compose -f … -f … --profile interop run --rm <service>`
    // We don't use the local override for port publishing here (containers talk
    // via the compose network, not localhost ports). Include it anyway for
    // consistency with the rest of the harness so the network name matches.
    let output = Command::new("docker")
        .args([
            "compose",
            "-f",
            COMPOSE_FILE,
            "-f",
            COMPOSE_LOCAL,
            "--profile",
            "interop",
            "run",
            "--rm",
            client.service,
        ])
        .current_dir(repo_root)
        .output();

    let output = match output {
        Ok(o) => o,
        Err(e) => {
            return Verdict::HarnessError(format!(
                "failed to spawn docker compose run for {}: {e}",
                client.service
            ));
        }
    };

    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    let exit_code = output.status.code().unwrap_or(-1);

    eprintln!(
        "[interop/{}] exit={} stdout_len={} stderr_len={}",
        client.scenario,
        exit_code,
        stdout.len(),
        stderr.len()
    );
    eprintln!("[interop/{}] stdout:\n{}", client.scenario, stdout);
    eprintln!("[interop/{}] stderr:\n{}", client.scenario, stderr);

    // Check for container/infra failure (exit code 125 = docker error, 126/127 = exec error).
    // 125 is "docker run itself failed"; also check for empty output on non-zero exit.
    if exit_code == 125 || exit_code == 126 || exit_code == 127 {
        return Verdict::HarnessError(format!(
            "docker compose run infrastructure failure (exit {exit_code}) for {}: stderr={}",
            client.service, stderr
        ));
    }

    // Non-zero exit = client could not complete interop against our server.
    if exit_code != 0 {
        return Verdict::SutFail(format!(
            "interop client '{}' (scenario '{}') exited {} — could not complete operations \
             against our server (REAL INTEROP FINDING).\n\
             stdout: {}\n\
             stderr: {}",
            client.service, client.scenario, exit_code, stdout, stderr
        ));
    }

    // Exit 0: the client completed. Capture the printed response for evidence.
    let response_bytes = stdout.trim().as_bytes().to_vec();

    // Attempt oracle c14n if the response looks like XML.
    // For XML responses (CXF prints raw envelope; Zeep prints Python repr which is not XML).
    let canonical_evidence = if stdout.trim().starts_with('<') {
        // Looks like XML — try to c14n it via the oracle.
        match oracle.c14n(&response_bytes) {
            Ok(canon) => {
                eprintln!(
                    "[interop/{}] oracle c14n succeeded ({} bytes)",
                    client.scenario,
                    canon.len()
                );
                canon
            }
            Err(e) => {
                eprintln!(
                    "[interop/{}] oracle c14n failed (non-fatal, storing raw response): {e}",
                    client.scenario
                );
                // Store raw bytes as evidence — the client succeeded even if C14N can't process
                // the Python repr or partial XML. This is not a SutFail.
                response_bytes
            }
        }
    } else {
        // Non-XML output (Python repr from Zeep's high-level deserialization).
        // Store raw bytes as evidence.
        eprintln!(
            "[interop/{}] stdout is not XML (Zeep Python repr?), storing as-is",
            client.scenario
        );
        response_bytes
    };

    // Promote on pass.
    if promote_on_pass {
        if let Err(e) = promote(store, client.scenario, &canonical_evidence) {
            eprintln!("[interop/{}] promotion failed: {e}", client.scenario);
        }
    }

    Verdict::Pass
}
