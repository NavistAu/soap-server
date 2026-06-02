//! Layer-1: replay every scenario against the controlled SUT, normalize, and diff
//! against the frozen snapshot. Set CROSSREF_REGEN=1 to (re)capture unverified
//! snapshots instead of asserting.

use crossref::mask_rules::default_masks;
use crossref::normalize::normalize;
use crossref::scenario::Scenario;
use crossref::snapshot::SnapshotStore;
use crossref::sut::{
    build_controlled_sut, build_controlled_sut_authed, build_controlled_sut_authed_strict,
    build_multi_service_sut, Sut,
};
use std::path::PathBuf;

fn dir(rel: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(rel)
}

fn load_scenarios() -> Vec<Scenario> {
    let mut out = Vec::new();
    for entry in std::fs::read_dir(dir("scenarios")).unwrap() {
        let p = entry.unwrap().path();
        if p.extension().and_then(|e| e.to_str()) == Some("toml") {
            let s = std::fs::read_to_string(&p).unwrap();
            out.push(Scenario::from_toml_str(&s).unwrap());
        }
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}

/// Select the correct SUT for a given scenario based on naming conventions.
/// - Scenarios named "wssec_stale*" use the strict-tolerance authed SUT.
/// - Scenarios named "wssec_*" (other) use the lenient authed SUT.
/// - Scenarios named "wsdl_rewrite_multi*" use the multi-service SUT.
/// - All other scenarios use the unauthenticated controlled SUT.
fn select_sut(scenario_name: &str) -> Sut {
    // `wssec_stale` MUST be checked before `wssec_`: it is a strict sub-prefix, so
    // reversing these branches would silently route stale-timestamp scenarios to the
    // lenient SUT and make wssec_stale_timestamp pass when it must fail.
    if scenario_name.starts_with("wssec_stale") {
        build_controlled_sut_authed_strict()
    } else if scenario_name.starts_with("wssec_") {
        build_controlled_sut_authed()
    } else if scenario_name.starts_with("wsdl_rewrite_multi") {
        build_multi_service_sut()
    } else {
        build_controlled_sut()
    }
}

#[tokio::test]
async fn replay_all_scenarios() {
    let regen = std::env::var("CROSSREF_REGEN").is_ok();
    let store = SnapshotStore::new(dir("snapshots"));
    let masks = default_masks();

    for sc in load_scenarios() {
        // Skip interop-driven scenarios — they are driven by container clients,
        // not the Layer-1 POST loop.
        if sc.interop_driven {
            continue;
        }

        let sut = select_sut(&sc.name);
        let req = std::fs::read(dir("scenarios").join(&sc.request_file)).unwrap();

        let resp = if sc.http_method.eq_ignore_ascii_case("GET") {
            sut.replay_get_wsdl(&sc.http_path).await
        } else {
            sut.replay(&sc.http_path, &req, &sc.content_type).await
        };

        assert_eq!(resp.status, sc.expected_status, "{}: status", sc.name);
        let normalized = normalize(&resp.body, &masks)
            .unwrap_or_else(|e| panic!("{}: normalize failed: {e}", sc.name));

        // Fault-presence guard: a scenario marked outcome=Fault must produce a
        // response containing "Fault" so a silent regression is caught immediately.
        // sc.fault.code / sc.fault.detail_policy are reserved for the Phase 1b
        // validator (§5.6) and are intentionally NOT asserted in Phase 1a beyond
        // this presence check.
        if sc.outcome == crossref::scenario::Outcome::Fault {
            assert!(
                normalized.contains("Fault"),
                "{}: outcome=Fault but response contains no 'Fault' element",
                sc.name
            );
        }

        match store.read(&sc.name) {
            None => {
                assert!(
                    regen,
                    "{}: no snapshot — run with CROSSREF_REGEN=1",
                    sc.name
                );
                store.write_unverified(&sc.name, &normalized).unwrap();
            }
            Some(frozen) if regen => {
                if frozen != normalized {
                    store.write_unverified(&sc.name, &normalized).unwrap();
                }
            }
            Some(frozen) => {
                similar_asserts::assert_eq!(frozen, normalized, "{}", sc.name);
            }
        }
    }
}

/// Dedicated test for WS-Security nonce replay detection.
/// Uses a single lenient-authed SUT instance with a shared nonce cache.
/// First request with nonce AAAAAAAAAAAAAAAAAAAAAA== must succeed (200),
/// second request with the SAME nonce must be rejected as a replay (500).
#[tokio::test]
async fn wssec_replay() {
    // The replay request uses the valid digest for alice/secret with the fixed nonce.
    // Nonce: AAAAAAAAAAAAAAAAAAAAAA== (16 zero bytes), Created: 2020-01-01T00:00:00.000Z
    // Digest: 0NCKf1qtLSP6NW9ow1q3Mk71TR8= (computed at authoring time)
    let request = br#"<env:Envelope xmlns:env="http://www.w3.org/2003/05/soap-envelope"><env:Header><wsse:Security xmlns:wsse="http://docs.oasis-open.org/wss/2004/01/oasis-200401-wss-wssecurity-secext-1.0.xsd" xmlns:wsu="http://docs.oasis-open.org/wss/2004/01/oasis-200401-wss-wssecurity-utility-1.0.xsd"><wsse:UsernameToken><wsse:Username>alice</wsse:Username><wsse:Password Type="http://docs.oasis-open.org/wss/2004/01/oasis-200401-wss-username-token-profile-1.0#PasswordDigest">0NCKf1qtLSP6NW9ow1q3Mk71TR8=</wsse:Password><wsse:Nonce>AAAAAAAAAAAAAAAAAAAAAA==</wsse:Nonce><wsu:Created>2020-01-01T00:00:00.000Z</wsu:Created></wsse:UsernameToken></wsse:Security></env:Header><env:Body><c:Echo xmlns:c="http://crossref.example/controlled"><c:Text>replay_test</c:Text></c:Echo></env:Body></env:Envelope>"#;

    let sut = build_controlled_sut_authed();
    let ct = "application/soap+xml; charset=utf-8";

    // First request: must succeed.
    let resp1 = sut.replay("/soap", request, ct).await;
    assert_eq!(
        resp1.status,
        200,
        "First wssec request should succeed: {}",
        resp1.body_utf8()
    );

    // Second request with SAME nonce: must be rejected as replay.
    let resp2 = sut.replay("/soap", request, ct).await;
    assert_eq!(
        resp2.status,
        500,
        "Second wssec request with same nonce should be rejected as replay: {}",
        resp2.body_utf8()
    );
    assert!(
        resp2.body_utf8().contains("replay")
            || resp2.body_utf8().contains("Replay")
            || resp2.body_utf8().contains("Authentication"),
        "Replay fault reason should mention replay or authentication, got: {}",
        resp2.body_utf8()
    );
}
