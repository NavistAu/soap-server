# crossref Phase 1a — Rust foundation (soap-server) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Stand up the `crossref` harness as a `publish = false` workspace member in
soap-server with scenario definitions, a controlled WSDL fixture + deterministic
handlers, path-scoped normalization, and a Rust Layer-1 replay/diff test that
captures `unverified` snapshots — the foundation the Docker conformance (1b) and
interop (1c) phases build on.

**Architecture:** A new `crossref/` Cargo crate (workspace member, not published)
depends on `soap-server` by path. It builds the SUT in-process via `ServerBuilder`
+ `axum_test::TestServer`, replays declarative scenarios, normalizes responses
(parse → path-scoped DOM mask → deterministic serialize), and diffs against frozen
golden snapshots carrying a provenance status (`unverified` until Layer 1b promotes
them). No Docker, no Java, no network in this phase.

**Tech Stack:** Rust, `soap-server` (path dep), `axum-test`, `quick-xml`,
`serde` + `toml` (scenario/status files), `similar-asserts` (readable diffs).

**Spec:** `docs/superpowers/specs/2026-06-02-crossref-harness-design.md` (Phase 1a
covers §3 packaging, §5.1 scenarios, §5.2 snapshots/provenance, §5.3 normalization,
§5.8 controlled fixtures, and the §10 seed scenarios as self-captured `unverified`
baselines).

---

## File Structure

- `Cargo.toml` (modify) — add `[workspace]` with member `crossref`; add
  `exclude = ["/crossref"]` to `[package]` so the published soap-server tarball
  never contains the harness.
- `crossref/Cargo.toml` (create) — the harness crate, `publish = false`.
- `crossref/src/lib.rs` (create) — re-exports the harness modules.
- `crossref/src/scenario.rs` (create) — `Scenario` model + TOML loader. One file,
  one responsibility: turn `scenarios/*.toml` into typed `Scenario` values.
- `crossref/src/normalize.rs` (create) — parse + path-scoped masking + deterministic
  serialization. The only module that touches XML trees.
- `crossref/src/snapshot.rs` (create) — golden-file load/store + provenance status
  (`status.toml`).
- `crossref/src/sut.rs` (create) — builds the soap-server System-Under-Test from the
  controlled fixture WSDL + deterministic handlers; exposes a `replay(scenario)`
  that returns the raw response.
- `crossref/fixtures/controlled.wsdl` (create) — the controlled WSDL (§5.8).
- `crossref/scenarios/*.toml` (create) — the §10 seed scenarios.
- `crossref/snapshots/*.xml` + `crossref/snapshots/status.toml` (generated) — golden
  corpus + provenance.
- `crossref/tests/layer1_replay.rs` (create) — the Layer-1 replay/diff test.
- `crossref/README.md` (create) — how to run + regenerate.

---

## Task 1: Convert soap-server to a workspace and scaffold the crossref crate

**Files:**
- Modify: `Cargo.toml`
- Create: `crossref/Cargo.toml`
- Create: `crossref/src/lib.rs`

- [ ] **Step 1: Add the workspace table + exclude to the root `Cargo.toml`**

Add these two pieces. Append a `[workspace]` table at the end of the file, and add
the `exclude` key inside the existing `[package]` table (immediately after the
`description` line):

In `[package]`:
```toml
exclude = ["/crossref"]
```

At end of file:
```toml
[workspace]
members = ["crossref"]
```

- [ ] **Step 2: Create the crossref crate manifest**

Create `crossref/Cargo.toml`:
```toml
[package]
name = "crossref"
version = "0.0.0"
edition = "2021"
publish = false

[dependencies]
soap-server = { path = ".." }
quick-xml = "0.39"
serde = { version = "1", features = ["derive"] }
toml = "0.8"

[dev-dependencies]
tokio = { version = "1", features = ["full"] }
axum-test = "20"
similar-asserts = "1"
```

- [ ] **Step 3: Create an empty lib so the crate compiles**

Create `crossref/src/lib.rs`:
```rust
//! crossref — differential conformance & interop harness for soap-server.
//!
//! Phase 1a: scenario model, controlled-fixture SUT, path-scoped normalization,
//! and Layer-1 replay/diff against `unverified` golden snapshots.

pub mod normalize;
pub mod scenario;
pub mod snapshot;
pub mod sut;
```

(The four modules are created in later tasks; this will not compile until Task 2.
Create the module files as empty stubs now so the crate builds.)

Create empty stubs:
- `crossref/src/scenario.rs` → `// filled in Task 2`
- `crossref/src/normalize.rs` → `// filled in Task 3`
- `crossref/src/snapshot.rs` → `// filled in Task 4`
- `crossref/src/sut.rs` → `// filled in Task 5`

- [ ] **Step 4: Verify the workspace builds and soap-server packaging is unaffected**

Run: `cargo build --workspace`
Expected: PASS — both `soap-server` and `crossref` compile.

Run: `cargo package --list -p soap-server | grep -c '^crossref/' || true`
Expected: `0` — no `crossref/` files in the soap-server package.

- [ ] **Step 5: Commit**

```bash
git add Cargo.toml crossref/Cargo.toml crossref/src/
git commit -m "feat(crossref): scaffold publish=false workspace member"
```

---

## Task 2: Scenario model + TOML loader

**Files:**
- Create: `crossref/src/scenario.rs`
- Test: `crossref/src/scenario.rs` (unit tests in-file)

The scenario schema encodes §5.1: operation, request, HTTP method/path, expected
status, SOAP version, content-type, outcome, and (for faults) fault class + detail
policy.

- [ ] **Step 1: Write the failing test**

Put at the bottom of `crossref/src/scenario.rs`:
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_a_success_scenario_toml() {
        let toml = r#"
            name = "op_echo_success"
            operation = "Echo"
            http_method = "POST"
            http_path = "/soap"
            content_type = "application/soap+xml; charset=utf-8"
            soap_version = "1.2"
            expected_status = 200
            outcome = "success"
            request_file = "op_echo_success.request.xml"
        "#;
        let s = Scenario::from_toml_str(toml).unwrap();
        assert_eq!(s.name, "op_echo_success");
        assert_eq!(s.soap_version, SoapVersion::V12);
        assert_eq!(s.outcome, Outcome::Success);
        assert_eq!(s.expected_status, 200);
        assert!(s.fault.is_none());
    }

    #[test]
    fn parses_a_fault_scenario_with_fault_class() {
        let toml = r#"
            name = "op_echo_missing_required"
            operation = "Echo"
            http_method = "POST"
            http_path = "/soap"
            content_type = "application/soap+xml; charset=utf-8"
            soap_version = "1.2"
            expected_status = 200
            outcome = "fault"
            request_file = "op_echo_missing_required.request.xml"

            [fault]
            code = "Sender"
            detail_policy = "absent"
        "#;
        let s = Scenario::from_toml_str(toml).unwrap();
        assert_eq!(s.outcome, Outcome::Fault);
        let f = s.fault.unwrap();
        assert_eq!(f.code, "Sender");
        assert_eq!(f.detail_policy, DetailPolicy::Absent);
    }
}
```

- [ ] **Step 2: Run it to verify it fails**

Run: `cargo test -p crossref scenario::tests -- --nocapture`
Expected: FAIL — `Scenario` / `SoapVersion` / `Outcome` not defined.

- [ ] **Step 3: Write the implementation**

Put at the top of `crossref/src/scenario.rs`:
```rust
//! Declarative scenario model (spec §5.1). Each scenario is one request against
//! the SUT with the full set of HTTP/SOAP expectations.

use serde::Deserialize;

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub enum SoapVersion {
    #[serde(rename = "1.1")]
    V11,
    #[serde(rename = "1.2")]
    V12,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Outcome {
    Success,
    Fault,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DetailPolicy {
    /// fault detail must be absent
    Absent,
    /// fault detail must be present (text only)
    Present,
    /// fault detail must be present and contain a raw XML child element
    RawXmlChild,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct FaultExpectation {
    /// Equivalent fault class (code/subcode). Reason text is NOT asserted (spec §10).
    pub code: String,
    #[serde(default)]
    pub subcode: Option<String>,
    pub detail_policy: DetailPolicy,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Scenario {
    pub name: String,
    pub operation: String,
    pub http_method: String,
    pub http_path: String,
    pub content_type: String,
    pub soap_version: SoapVersion,
    pub expected_status: u16,
    pub outcome: Outcome,
    /// Path (relative to `scenarios/`) of the request body XML.
    pub request_file: String,
    #[serde(default)]
    pub fault: Option<FaultExpectation>,
}

impl Scenario {
    pub fn from_toml_str(s: &str) -> Result<Self, toml::de::Error> {
        toml::from_str(s)
    }
}
```

- [ ] **Step 4: Run it to verify it passes**

Run: `cargo test -p crossref scenario::tests -- --nocapture`
Expected: PASS (2 tests).

- [ ] **Step 5: Commit**

```bash
git add crossref/src/scenario.rs
git commit -m "feat(crossref): scenario model + TOML loader (spec 5.1)"
```

---

## Task 3: Path-scoped normalization (parse → mask → deterministic serialize)

**Files:**
- Create: `crossref/src/normalize.rs`
- Test: `crossref/src/normalize.rs` (unit tests in-file)

Layer-1 normalization. Masks are **path-scoped** (spec §5.3): a mask is a list of
element local-name *paths* (e.g. `Envelope/Header/Security/UsernameToken/Nonce`)
whose text content is replaced with a fixed sentinel. Global value-pattern masking
is structurally impossible here — there is no regex over text. Output is a
deterministic re-serialization (stable element order as encountered, attributes
sorted by name) suitable for regression diffing.

- [ ] **Step 1: Write the failing test**

Put at the bottom of `crossref/src/normalize.rs`:
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn masks_only_the_path_scoped_element_text() {
        let xml = br#"<Envelope><Header><Nonce>AAAA</Nonce></Header><Body><Nonce>keepme</Nonce></Body></Envelope>"#;
        let rules = vec![MaskRule::new("Envelope/Header/Nonce")];
        let out = normalize(xml, &rules).unwrap();
        // Header Nonce masked, Body Nonce (different path) preserved.
        assert!(out.contains("<Nonce>__MASKED__</Nonce>"));
        assert!(out.contains("<Nonce>keepme</Nonce>"));
    }

    #[test]
    fn sorts_attributes_for_stable_output() {
        let a = normalize(br#"<E b="2" a="1"/>"#, &[]).unwrap();
        let b = normalize(br#"<E a="1" b="2"/>"#, &[]).unwrap();
        assert_eq!(a, b);
    }

    #[test]
    fn malformed_xml_errors() {
        assert!(normalize(b"<E></WRONG>", &[]).is_err());
    }
}
```

- [ ] **Step 2: Run it to verify it fails**

Run: `cargo test -p crossref normalize::tests -- --nocapture`
Expected: FAIL — `normalize` / `MaskRule` not defined.

- [ ] **Step 3: Write the implementation**

Put at the top of `crossref/src/normalize.rs`:
```rust
//! Layer-1 normalization (spec §5.3): parse → path-scoped DOM mask →
//! deterministic serialize. This is REGRESSION canonicalization (our output vs our
//! own frozen baseline), NOT the authoritative C14N — that is delegated to the Java
//! XML oracle in Layer 1b and never done here.

use quick_xml::events::{BytesStart, Event};
use quick_xml::{Reader, Writer};
use std::io::Cursor;

pub const MASK_SENTINEL: &str = "__MASKED__";

/// A path-scoped mask: the slash-joined local-name path whose text is masked.
#[derive(Debug, Clone)]
pub struct MaskRule {
    segments: Vec<String>,
}

impl MaskRule {
    pub fn new(path: &str) -> Self {
        MaskRule {
            segments: path.split('/').map(|s| s.to_string()).collect(),
        }
    }
    fn matches(&self, stack: &[String]) -> bool {
        stack.len() == self.segments.len()
            && stack.iter().zip(&self.segments).all(|(a, b)| a == b)
    }
}

fn local_name(e: &BytesStart) -> String {
    let full = e.name();
    let bytes = full.as_ref();
    let local = match bytes.iter().rposition(|&b| b == b':') {
        Some(i) => &bytes[i + 1..],
        None => bytes,
    };
    String::from_utf8_lossy(local).into_owned()
}

/// Re-serialize a start tag with attributes sorted by full name (stable output).
fn write_sorted_start<W: std::io::Write>(
    w: &mut Writer<W>,
    e: &BytesStart,
    empty: bool,
) -> Result<(), String> {
    let mut elem = BytesStart::new(String::from_utf8_lossy(e.name().as_ref()).into_owned());
    let mut attrs: Vec<(String, String)> = Vec::new();
    for a in e.attributes() {
        let a = a.map_err(|err| err.to_string())?;
        attrs.push((
            String::from_utf8_lossy(a.key.as_ref()).into_owned(),
            String::from_utf8_lossy(&a.value).into_owned(),
        ));
    }
    attrs.sort_by(|x, y| x.0.cmp(&y.0));
    for (k, v) in attrs {
        elem.push_attribute((k.as_str(), v.as_str()));
    }
    let ev = if empty { Event::Empty(elem) } else { Event::Start(elem) };
    w.write_event(ev).map_err(|err| err.to_string())
}

/// Parse `xml`, mask path-scoped element text, and emit deterministic output.
pub fn normalize(xml: &[u8], masks: &[MaskRule]) -> Result<String, String> {
    let mut reader = Reader::from_reader(xml);
    reader.config_mut().trim_text(true);
    let mut writer = Writer::new(Cursor::new(Vec::new()));
    let mut stack: Vec<String> = Vec::new();
    let mut buf = Vec::new();

    loop {
        match reader.read_event_into(&mut buf).map_err(|e| e.to_string())? {
            Event::Start(e) => {
                stack.push(local_name(&e));
                write_sorted_start(&mut writer, &e, false)?;
            }
            Event::Empty(e) => {
                stack.push(local_name(&e));
                write_sorted_start(&mut writer, &e, true)?;
                stack.pop();
            }
            Event::End(e) => {
                writer
                    .write_event(Event::End(e.to_owned()))
                    .map_err(|err| err.to_string())?;
                stack.pop();
            }
            Event::Text(t) => {
                let masked = masks.iter().any(|m| m.matches(&stack));
                if masked {
                    writer
                        .write_event(Event::Text(quick_xml::events::BytesText::new(MASK_SENTINEL)))
                        .map_err(|err| err.to_string())?;
                } else {
                    writer
                        .write_event(Event::Text(t.to_owned()))
                        .map_err(|err| err.to_string())?;
                }
            }
            Event::Eof => break,
            other => {
                writer
                    .write_event(other.to_owned())
                    .map_err(|err| err.to_string())?;
            }
        }
        buf.clear();
    }
    let bytes = writer.into_inner().into_inner();
    String::from_utf8(bytes).map_err(|e| e.to_string())
}
```

- [ ] **Step 4: Run it to verify it passes**

Run: `cargo test -p crossref normalize::tests -- --nocapture`
Expected: PASS (3 tests).

- [ ] **Step 5: Commit**

```bash
git add crossref/src/normalize.rs
git commit -m "feat(crossref): path-scoped normalization (spec 5.3)"
```

---

## Task 4: Snapshot store with provenance status

**Files:**
- Create: `crossref/src/snapshot.rs`
- Test: `crossref/src/snapshot.rs` (unit tests in-file, using a temp dir)

Snapshots are golden files `snapshots/<name>.xml` (the normalized response) plus a
`snapshots/status.toml` mapping scenario name → `unverified | verified` (spec §5.2).
Phase 1a only ever writes `unverified`.

- [ ] **Step 1: Write the failing test**

Put at the bottom of `crossref/src/snapshot.rs`:
```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn tmp() -> std::path::PathBuf {
        let mut p = std::env::temp_dir();
        p.push(format!("crossref-snap-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&p);
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    #[test]
    fn writes_unverified_and_reads_back() {
        let dir = tmp();
        let store = SnapshotStore::new(&dir);
        store.write_unverified("op_x", "<normalized/>").unwrap();
        assert_eq!(store.read("op_x").unwrap(), "<normalized/>");
        assert_eq!(store.status("op_x").unwrap(), Status::Unverified);
    }

    #[test]
    fn missing_snapshot_reads_none() {
        let dir = tmp();
        let store = SnapshotStore::new(&dir);
        assert!(store.read("absent").is_none());
    }

    #[test]
    fn counts_unverified() {
        let dir = tmp();
        let store = SnapshotStore::new(&dir);
        store.write_unverified("a", "<a/>").unwrap();
        store.write_unverified("b", "<b/>").unwrap();
        assert_eq!(store.unverified_count().unwrap(), 2);
    }
}
```

- [ ] **Step 2: Run it to verify it fails**

Run: `cargo test -p crossref snapshot::tests -- --nocapture`
Expected: FAIL — `SnapshotStore` / `Status` not defined.

- [ ] **Step 3: Write the implementation**

Put at the top of `crossref/src/snapshot.rs`:
```rust
//! Golden snapshot store with provenance status (spec §5.2).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    Unverified,
    Verified,
}

impl Status {
    fn as_str(self) -> &'static str {
        match self {
            Status::Unverified => "unverified",
            Status::Verified => "verified",
        }
    }
    fn parse(s: &str) -> Option<Status> {
        match s {
            "unverified" => Some(Status::Unverified),
            "verified" => Some(Status::Verified),
            _ => None,
        }
    }
}

pub struct SnapshotStore {
    dir: PathBuf,
}

impl SnapshotStore {
    pub fn new(dir: impl AsRef<Path>) -> Self {
        SnapshotStore { dir: dir.as_ref().to_path_buf() }
    }

    fn snap_path(&self, name: &str) -> PathBuf {
        self.dir.join(format!("{name}.xml"))
    }
    fn status_path(&self) -> PathBuf {
        self.dir.join("status.toml")
    }

    fn load_status_map(&self) -> BTreeMap<String, String> {
        match std::fs::read_to_string(self.status_path()) {
            Ok(s) => toml::from_str(&s).unwrap_or_default(),
            Err(_) => BTreeMap::new(),
        }
    }
    fn save_status_map(&self, map: &BTreeMap<String, String>) -> Result<(), String> {
        let s = toml::to_string_pretty(map).map_err(|e| e.to_string())?;
        std::fs::write(self.status_path(), s).map_err(|e| e.to_string())
    }

    pub fn read(&self, name: &str) -> Option<String> {
        std::fs::read_to_string(self.snap_path(name)).ok()
    }

    pub fn status(&self, name: &str) -> Option<Status> {
        self.load_status_map().get(name).and_then(|s| Status::parse(s))
    }

    pub fn write_unverified(&self, name: &str, normalized: &str) -> Result<(), String> {
        std::fs::create_dir_all(&self.dir).map_err(|e| e.to_string())?;
        std::fs::write(self.snap_path(name), normalized).map_err(|e| e.to_string())?;
        let mut map = self.load_status_map();
        map.insert(name.to_string(), Status::Unverified.as_str().to_string());
        self.save_status_map(&map)
    }

    pub fn unverified_count(&self) -> Result<usize, String> {
        Ok(self
            .load_status_map()
            .values()
            .filter(|v| v.as_str() == Status::Unverified.as_str())
            .count())
    }
}
```

- [ ] **Step 4: Run it to verify it passes**

Run: `cargo test -p crossref snapshot::tests -- --nocapture`
Expected: PASS (3 tests).

- [ ] **Step 5: Commit**

```bash
git add crossref/src/snapshot.rs
git commit -m "feat(crossref): snapshot store with provenance status (spec 5.2)"
```

---

## Task 5: Controlled fixture WSDL + SUT builder

**Files:**
- Create: `crossref/fixtures/controlled.wsdl`
- Create: `crossref/src/sut.rs`
- Test: `crossref/src/sut.rs` (one async test)

The controlled service (spec §5.8) is a tiny deterministic `Echo` document/literal
operation: request carries a required `Text` element; success echoes it; a missing
`Text` produces a Sender fault from the dispatch validator. Both our server and
(in Phase 1b) CXF will implement exactly this.

- [ ] **Step 1: Create the controlled WSDL**

Create `crossref/fixtures/controlled.wsdl`:
```xml
<?xml version="1.0" encoding="utf-8"?>
<wsdl:definitions
    xmlns:wsdl="http://schemas.xmlsoap.org/wsdl/"
    xmlns:soap="http://schemas.xmlsoap.org/wsdl/soap12/"
    xmlns:xs="http://www.w3.org/2001/XMLSchema"
    xmlns:tns="http://crossref.example/controlled"
    targetNamespace="http://crossref.example/controlled">
  <wsdl:types>
    <xs:schema targetNamespace="http://crossref.example/controlled" elementFormDefault="qualified">
      <xs:element name="Echo">
        <xs:complexType><xs:sequence>
          <xs:element name="Text" type="xs:string" minOccurs="1"/>
        </xs:sequence></xs:complexType>
      </xs:element>
      <xs:element name="EchoResponse">
        <xs:complexType><xs:sequence>
          <xs:element name="Text" type="xs:string" minOccurs="1"/>
        </xs:sequence></xs:complexType>
      </xs:element>
    </xs:schema>
  </wsdl:types>
  <wsdl:message name="EchoRequest"><wsdl:part name="parameters" element="tns:Echo"/></wsdl:message>
  <wsdl:message name="EchoResponse"><wsdl:part name="parameters" element="tns:EchoResponse"/></wsdl:message>
  <wsdl:portType name="ControlledPort">
    <wsdl:operation name="Echo">
      <wsdl:input message="tns:EchoRequest"/>
      <wsdl:output message="tns:EchoResponse"/>
    </wsdl:operation>
  </wsdl:portType>
  <wsdl:binding name="ControlledBinding" type="tns:ControlledPort">
    <soap:binding style="document" transport="http://schemas.xmlsoap.org/soap/http"/>
    <wsdl:operation name="Echo">
      <soap:operation soapAction="http://crossref.example/controlled/Echo"/>
      <wsdl:input><soap:body use="literal"/></wsdl:input>
      <wsdl:output><soap:body use="literal"/></wsdl:output>
    </wsdl:operation>
  </wsdl:binding>
  <wsdl:service name="ControlledService">
    <wsdl:port name="ControlledPort" binding="tns:ControlledBinding">
      <soap:address location="http://localhost/soap"/>
    </wsdl:port>
  </wsdl:service>
</wsdl:definitions>
```

- [ ] **Step 2: Write the failing test**

Put at the bottom of `crossref/src/sut.rs`:
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn echo_success_returns_echoresponse() {
        let sut = build_controlled_sut();
        let body = br#"<env:Envelope xmlns:env="http://www.w3.org/2003/05/soap-envelope"><env:Body><c:Echo xmlns:c="http://crossref.example/controlled"><c:Text>hi</c:Text></c:Echo></env:Body></env:Envelope>"#;
        let resp = sut.replay("/soap", body, "application/soap+xml; charset=utf-8").await;
        assert_eq!(resp.status, 200);
        assert!(resp.body_utf8().contains("EchoResponse"));
        assert!(resp.body_utf8().contains("hi"));
    }
}
```

- [ ] **Step 3: Run it to verify it fails**

Run: `cargo test -p crossref sut::tests -- --nocapture`
Expected: FAIL — `build_controlled_sut` / `Sut` not defined.

- [ ] **Step 4: Write the implementation**

Put at the top of `crossref/src/sut.rs`:
```rust
//! Builds the soap-server System-Under-Test from the controlled fixture (spec §5.8)
//! and replays requests against it in-process via axum_test.

use axum_test::TestServer;
use bytes::Bytes;
use soap_server::{FnHandler, ServerBuilder};

pub const CONTROLLED_WSDL: &[u8] = include_bytes!("../fixtures/controlled.wsdl");

pub struct Response {
    pub status: u16,
    pub content_type: String,
    pub body: Vec<u8>,
}
impl Response {
    pub fn body_utf8(&self) -> String {
        String::from_utf8_lossy(&self.body).into_owned()
    }
}

pub struct Sut {
    server: TestServer,
}

impl Sut {
    /// Replay a raw request body against the SUT and capture the response.
    pub async fn replay(&self, path: &str, body: &[u8], content_type: &str) -> Response {
        let r = self
            .server
            .post(path)
            .content_type(content_type)
            .bytes(Bytes::copy_from_slice(body))
            .await;
        let content_type = r
            .maybe_header("content-type")
            .map(|h| h.to_str().unwrap_or("").to_string())
            .unwrap_or_default();
        Response {
            status: r.status_code().as_u16(),
            content_type,
            body: r.as_bytes().to_vec(),
        }
    }
}

/// Deterministic Echo handler: echoes the request's `Text` element verbatim.
fn echo_handler() -> FnHandler<impl Fn(Bytes) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<Bytes, soap_server::SoapFault>> + Send>> + Clone + Send + Sync + 'static> {
    FnHandler::new(|body: Bytes| async move {
        let text = extract_text(&body).unwrap_or_default();
        let resp = format!(
            r#"<c:EchoResponse xmlns:c="http://crossref.example/controlled"><c:Text>{}</c:Text></c:EchoResponse>"#,
            soap_server::escape_text(&text)
        );
        Ok(Bytes::from(resp))
    })
}

fn extract_text(body: &[u8]) -> Option<String> {
    use quick_xml::events::Event;
    let mut reader = quick_xml::Reader::from_reader(body);
    reader.config_mut().trim_text(true);
    let mut in_text = false;
    let mut buf = Vec::new();
    loop {
        match reader.read_event_into(&mut buf).ok()? {
            Event::Start(e) => {
                let n = e.name();
                if n.as_ref().ends_with(b"Text") {
                    in_text = true;
                }
            }
            Event::Text(t) if in_text => {
                return Some(String::from_utf8_lossy(t.as_ref()).into_owned());
            }
            Event::Eof => return None,
            _ => {}
        }
        buf.clear();
    }
}

pub fn build_controlled_sut() -> Sut {
    let svc = ServerBuilder::from_wsdl_bytes(CONTROLLED_WSDL.to_vec())
        .path("/soap")
        .handler("Echo", echo_handler())
        .build()
        .expect("controlled SUT must build");
    let server = TestServer::new(svc.into_router()).expect("test server");
    Sut { server }
}
```

> NOTE for the implementer: the exact `FnHandler::new` closure signature must match
> soap-server's `handler.rs`. If the generic bound above does not compile, simplify
> by defining a named unit struct implementing `SoapHandler` directly (see
> `soap-server/src/handler.rs` for the trait: `async fn handle(&self, body: Bytes)
> -> Result<Bytes, SoapFault>` and the additive `handle_with_headers`). Prefer
> whichever the existing `tests/integration_test.rs` uses.

- [ ] **Step 5: Run it to verify it passes**

Run: `cargo test -p crossref sut::tests -- --nocapture`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add crossref/fixtures/controlled.wsdl crossref/src/sut.rs
git commit -m "feat(crossref): controlled WSDL fixture + SUT replay (spec 5.8)"
```

---

## Task 6: Layer-1 replay/diff harness + first two seed scenarios

**Files:**
- Create: `crossref/scenarios/op_echo_success.toml`
- Create: `crossref/scenarios/op_echo_success.request.xml`
- Create: `crossref/scenarios/op_echo_missing_required.toml`
- Create: `crossref/scenarios/op_echo_missing_required.request.xml`
- Create: `crossref/src/mask_rules.rs` (+ register in `lib.rs`)
- Create: `crossref/tests/layer1_replay.rs`

The harness loads every scenario, replays it, normalizes the response with the
path-scoped mask rules, and diffs against the frozen snapshot. If no snapshot
exists, it **captures** one as `unverified` (regen mode); otherwise it asserts
equality. Mode is chosen by the `CROSSREF_REGEN` env var.

- [ ] **Step 1: Create the two seed scenarios (success + fault)**

`crossref/scenarios/op_echo_success.toml`:
```toml
name = "op_echo_success"
operation = "Echo"
http_method = "POST"
http_path = "/soap"
content_type = "application/soap+xml; charset=utf-8"
soap_version = "1.2"
expected_status = 200
outcome = "success"
request_file = "op_echo_success.request.xml"
```

`crossref/scenarios/op_echo_success.request.xml`:
```xml
<env:Envelope xmlns:env="http://www.w3.org/2003/05/soap-envelope"><env:Body><c:Echo xmlns:c="http://crossref.example/controlled"><c:Text>hello</c:Text></c:Echo></env:Body></env:Envelope>
```

`crossref/scenarios/op_echo_missing_required.toml`:
```toml
name = "op_echo_missing_required"
operation = "Echo"
http_method = "POST"
http_path = "/soap"
content_type = "application/soap+xml; charset=utf-8"
soap_version = "1.2"
expected_status = 200
outcome = "fault"
request_file = "op_echo_missing_required.request.xml"

[fault]
code = "Sender"
detail_policy = "absent"
```

`crossref/scenarios/op_echo_missing_required.request.xml`:
```xml
<env:Envelope xmlns:env="http://www.w3.org/2003/05/soap-envelope"><env:Body><c:Echo xmlns:c="http://crossref.example/controlled"/></env:Body></env:Envelope>
```

- [ ] **Step 2: Create the default mask-rule set**

Create `crossref/src/mask_rules.rs`:
```rust
//! Default path-scoped mask rules for SOAP responses (spec §5.3). Extend per
//! scenario as volatile fields appear; never use value-pattern masks.
use crate::normalize::MaskRule;

pub fn default_masks() -> Vec<MaskRule> {
    // The controlled Echo service is fully deterministic, so the default set is
    // empty. WS-Security / WS-Addressing scenarios add path-scoped rules here,
    // e.g. MaskRule::new("Envelope/Header/Security/UsernameToken/Created").
    Vec::new()
}
```

Add `pub mod mask_rules;` to `crossref/src/lib.rs`.

- [ ] **Step 3: Write the replay/diff test**

Create `crossref/tests/layer1_replay.rs`:
```rust
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
                assert!(regen, "{}: no snapshot — run with CROSSREF_REGEN=1", sc.name);
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
```

- [ ] **Step 4: Capture the initial unverified snapshots**

Run: `CROSSREF_REGEN=1 cargo test -p crossref --test layer1_replay -- --nocapture`
Expected: PASS; creates `crossref/snapshots/op_echo_success.xml`,
`crossref/snapshots/op_echo_missing_required.xml`, and `crossref/snapshots/status.toml`
with both entries `unverified`.

- [ ] **Step 5: Run again WITHOUT regen to verify the diff gate passes**

Run: `cargo test -p crossref --test layer1_replay`
Expected: PASS (snapshots match).

- [ ] **Step 6: Sanity-check the fault snapshot is actually a fault**

Run: `grep -l Fault crossref/snapshots/op_echo_missing_required.xml`
Expected: the file is listed (the missing-required request produced a SOAP fault).

- [ ] **Step 7: Commit**

```bash
git add crossref/scenarios crossref/snapshots crossref/src/mask_rules.rs crossref/src/lib.rs crossref/tests/layer1_replay.rs
git commit -m "feat(crossref): Layer-1 replay/diff harness + echo seed scenarios"
```

---

## Task 7: Author the remaining §10 seed scenarios

**Files:**
- Create: `crossref/scenarios/*.toml` + `*.request.xml` (one pair per scenario below)
- Modify: `crossref/src/mask_rules.rs` (add path-scoped rules for volatile fields)
- Modify: `crossref/fixtures/controlled.wsdl` if a scenario needs an operation the
  controlled service does not yet expose (e.g. add a `Faulty` op, or rely on the
  validator-driven fault from `Echo` missing-required for doc/literal cases)

Each scenario is authored using the exact schema from Task 6 (a `.toml` + a
`.request.xml`). For each, add the pair, then run Step "capture" then "verify".
Required scenarios (spec §10) — author one per line, all driven against the
controlled SUT (these are all requests to *our* server; the CXF cross-check that
promotes them to `verified` is Phase 1b):

- [ ] `soap12_echo_success` — SOAP 1.2 success (covered by Task 6; keep).
- [ ] `soap12_fault` — SOAP 1.2 fault (covered by Task 6 missing-required; keep).
- [ ] `soap11_echo_success` — same Echo request but SOAP 1.1 envelope
  (`xmlns="http://schemas.xmlsoap.org/soap/envelope/"`, `content_type =
  "text/xml; charset=utf-8"`, `soap_version = "1.1"`).
- [ ] `soap11_fault` — SOAP 1.1 missing-required → 1.1 fault
  (`faultcode`/`faultstring`).
- [ ] `fault_detail_raw_xml` — a request that triggers a fault whose `detail`
  carries a raw XML child; `detail_policy = "raw_xml_child"`. (Add a `Faulty`
  operation to the controlled WSDL + a handler that returns a `SoapFault` with an
  XML detail, if the validator fault has no detail.)
- [ ] `ns_on_envelope` — Echo with `c:` declared on `Envelope`.
- [ ] `ns_on_header` — `c:` declared on `Header`.
- [ ] `ns_on_body` — `c:` declared on `Body`.
- [ ] `ns_on_operation` — `c:` declared on the `Echo` element.
- [ ] `ns_on_nested_payload` — `c:` declared on the nested `Text` element.
- [ ] `ns_prefix_shadowing` — `Envelope` and `Body` declare the same prefix to
  different URIs.
- [ ] `doc_literal_inline_present` / `doc_literal_inline_missing` — Echo's inline
  type (present = success, missing = fault). (Echo already uses an inline complex
  type with a required `Text`; these are the present/missing pair.)
- [ ] `doc_literal_named_present` / `doc_literal_named_missing` — add an operation
  whose input references a *named* complex type with a required child; author the
  present/missing pair.
- [ ] `wssec_digest_success` — Echo with a valid WS-Security digest UsernameToken
  (build the SUT in `sut.rs` with `.auth(...)` for a WS-Security variant; add a
  `build_controlled_sut_authed()` helper). Add path-scoped masks for
  `…/UsernameToken/Nonce` and `…/UsernameToken/Created`.
- [ ] `wssec_bad_password` — wrong password → fault.
- [ ] `wssec_stale_timestamp` — `Created` far in the past → fault.
- [ ] `wssec_replay` — same nonce twice → second is a fault. (Two requests in one
  scenario; extend the scenario schema with an optional `replay_of` field, OR model
  as a dedicated test in `layer1_replay.rs`. Prefer the dedicated test to keep the
  scenario schema simple.)
- [ ] `wssec_missing_auth` — no Security header on the authed SUT → fault.
- [ ] `wsdl_rewrite_single` — `GET /soap?wsdl` returns the WSDL with the address
  rewritten to the request host (use `server.get("/soap").add_query_param("wsdl",
  "")`). Snapshot the rewritten WSDL (mask the host with a path-scoped attribute
  rule).
- [ ] `wsdl_rewrite_multi` — multi-service WSDL (reuse the
  `tests/integration_test.rs` `MULTI_SERVICE_WSDL`): `GET /soap/a?wsdl` keeps
  ServiceB's address at `/soap/b`. Build a second SUT for the multi-service WSDL.

- [ ] **Step A: After authoring each pair, capture its snapshot**

Run: `CROSSREF_REGEN=1 cargo test -p crossref --test layer1_replay`
Expected: PASS; new `unverified` snapshots written.

- [ ] **Step B: Verify the gate (no regen)**

Run: `cargo test -p crossref --test layer1_replay`
Expected: PASS.

- [ ] **Step C: Confirm every §10 scenario exists**

Run: `ls crossref/scenarios/*.toml | wc -l`
Expected: the count matches the §10 list (≥ 22 scenario files).

- [ ] **Step D: Commit (one commit per logical group is fine)**

```bash
git add crossref/scenarios crossref/snapshots crossref/src
git commit -m "feat(crossref): author §10 seed scenarios as unverified baselines"
```

---

## Task 8: Wire crossref into CI + document

**Files:**
- Modify: `.github/workflows/ci.yml`
- Create: `crossref/README.md`

- [ ] **Step 1: Confirm crossref runs under the existing workspace test**

Run: `cargo test --workspace`
Expected: PASS — soap-server tests AND crossref Layer-1 replay all pass. (The
existing CI `test` job runs `cargo test --workspace`, so crossref Layer-1 is now in
per-commit CI automatically. Verify by reading `.github/workflows/ci.yml` — if the
test job runs `cargo test` without `--workspace`, change it to `--workspace`.)

- [ ] **Step 2: Add a step that surfaces the unverified-snapshot count**

In `.github/workflows/ci.yml`, in the `test` job after the test step, add:
```yaml
      - name: crossref unverified-snapshot count
        run: |
          python3 - <<'PY'
          import tomllib, pathlib
          p = pathlib.Path("crossref/snapshots/status.toml")
          if p.exists():
              data = tomllib.loads(p.read_text())
              n = sum(1 for v in data.values() if v == "unverified")
              print(f"crossref: {n} unverified snapshot(s) (correctness pending Phase 1b)")
          else:
              print("crossref: no snapshots yet")
          PY
```

- [ ] **Step 3: Write the README**

Create `crossref/README.md`:
```markdown
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
- Diff against frozen snapshots: `cargo test -p crossref --test layer1_replay`
- (Re)capture snapshots: `CROSSREF_REGEN=1 cargo test -p crossref --test layer1_replay`

Snapshot changes are reviewed like any golden file.
```

- [ ] **Step 4: Final full gate**

Run: `cargo test --workspace && cargo fmt --check && cargo clippy --workspace --all-targets -- -D warnings`
Expected: all PASS (crossref included).

- [ ] **Step 5: Commit**

```bash
git add .github/workflows/ci.yml crossref/README.md
git commit -m "ci(crossref): run Layer-1 in CI + surface unverified count; README"
```

---

## Self-review notes (author)

- **Spec coverage:** §3 packaging (Task 1), §5.1 scenarios (Task 2, 7), §5.2
  snapshots/provenance (Task 4), §5.3 normalization + path-scoped masks (Task 3, 6,
  7), §5.8 controlled fixtures (Task 5), §10 seed scenarios (Task 6, 7). §5.6/§5.7
  (schema-validation targets, verdict model) and conformance diffing are **Phase 1b**
  — explicitly out of scope here; Layer-1 snapshots are `unverified` until 1b.
- **Phase boundary:** Phase 1a deliberately proves *unchanged*, not *correct*. The
  CI step in Task 8 surfaces the unverified count so no one mistakes 1a snapshots
  for conformance evidence.
- **Type consistency:** `Scenario`/`SoapVersion`/`Outcome`/`DetailPolicy`/`MaskRule`/
  `SnapshotStore`/`Status`/`Sut`/`Response` are defined once (Tasks 2–5) and used
  consistently in Task 6's harness.
- **Known implementer risk:** the `FnHandler` closure bound in Task 5 may need to
  match soap-server's exact signature — the task flags the fallback (named
  `SoapHandler` impl per `tests/integration_test.rs`).
