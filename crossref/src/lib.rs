//! crossref — differential conformance & interop harness for soap-server.
//!
//! Phase 1a: scenario model, controlled-fixture SUT, path-scoped normalization,
//! and Layer-1 replay/diff against `unverified` golden snapshots.

pub mod normalize;
pub mod scenario;
pub mod snapshot;
pub mod sut;
