# crossref (soap-server)

Differential conformance & interop harness. See the design spec:
`../docs/superpowers/specs/2026-06-02-crossref-harness-design.md`.

## Phase 1a (this) — Rust Layer-1 only
- Replays `scenarios/*.toml` against a controlled in-process SUT.
- Normalizes (parse → path-scoped mask → deterministic serialize) and diffs against
  golden `snapshots/*.xml`.
- Snapshots are **`unverified`** (self-captured regression baselines). Conformance
  correctness arrives in Phase 1b when the Java XML oracle + CXF promote them to
  `verified`.

## Run
- All tests (replay + wssec): `cargo test -p crossref`
- Diff against frozen snapshots: `cargo test -p crossref --test layer1_replay`
- (Re)capture snapshots: `CROSSREF_REGEN=1 cargo test -p crossref --test layer1_replay`

Snapshot changes are reviewed like any golden file.
