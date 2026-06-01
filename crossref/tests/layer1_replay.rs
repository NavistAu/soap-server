//! Layer-1: replay every scenario against the controlled SUT, normalize, and diff
//! against the frozen snapshot. Set CROSSREF_REGEN=1 to (re)capture unverified
//! snapshots instead of asserting.

use crossref::mask_rules::default_masks;
use crossref::normalize::normalize;
use crossref::scenario::Scenario;
use crossref::snapshot::SnapshotStore;
use crossref::sut::build_controlled_sut;
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

#[tokio::test]
async fn replay_all_scenarios() {
    let regen = std::env::var("CROSSREF_REGEN").is_ok();
    let sut = build_controlled_sut();
    let store = SnapshotStore::new(dir("snapshots"));
    let masks = default_masks();

    for sc in load_scenarios() {
        let req = std::fs::read(dir("scenarios").join(&sc.request_file)).unwrap();
        let resp = sut.replay(&sc.http_path, &req, &sc.content_type).await;
        assert_eq!(resp.status, sc.expected_status, "{}: status", sc.name);
        let normalized = normalize(&resp.body, &masks)
            .unwrap_or_else(|e| panic!("{}: normalize failed: {e}", sc.name));

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
