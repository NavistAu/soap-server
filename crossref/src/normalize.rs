//! Layer-1 normalization (spec §5.3): parse → path-scoped DOM mask →
//! deterministic serialize. This is REGRESSION canonicalization (our output vs our
//! own frozen baseline), NOT the authoritative C14N — that is delegated to the Java
//! XML oracle in Layer 1b and never done here.

use quick_xml::events::{BytesStart, BytesText, Event};
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
        stack.len() == self.segments.len() && stack.iter().zip(&self.segments).all(|(a, b)| a == b)
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

/// Re-serialize a start tag with attributes sorted by full QName (stable output).
/// NOTE: sorting by raw QName bytes is stable only while namespace prefix assignments
/// are deterministic (true for the controlled SUT; Task 7's ns_prefix_shadowing
/// scenarios remain within that boundary).
fn write_sorted_start<W: std::io::Write>(
    w: &mut Writer<W>,
    e: &BytesStart,
    empty: bool,
) -> Result<(), String> {
    let mut elem = BytesStart::new(String::from_utf8_lossy(e.name().as_ref()).into_owned());
    // Collect raw key/value bytes so we can sort by key without re-escaping values.
    let mut attrs: Vec<(Vec<u8>, Vec<u8>)> = Vec::new();
    for a in e.attributes() {
        let a = a.map_err(|err| err.to_string())?;
        attrs.push((a.key.as_ref().to_owned(), a.value.into_owned()));
    }
    // Sort by key bytes for deterministic output.
    attrs.sort_by(|x, y| x.0.cmp(&y.0));
    for (k, v) in &attrs {
        // From<(&[u8], &[u8])> stores the value verbatim — no re-escape pass.
        elem.push_attribute((k.as_slice(), v.as_slice()));
    }
    let ev = if empty {
        Event::Empty(elem)
    } else {
        Event::Start(elem)
    };
    w.write_event(ev).map_err(|err| err.to_string())
}

/// Parse `xml`, mask path-scoped element text, and emit deterministic output.
pub fn normalize(xml: &[u8], masks: &[MaskRule]) -> Result<String, String> {
    let mut reader = Reader::from_reader(xml);
    // check_end_names defaults to true in quick-xml 0.39 — mismatched end tags
    // surface as an error from read_event_into, satisfying the malformed_xml_errors test.
    // trim_text(false): quick-xml fragments text around entity refs; trimming edges of each
    // fragment silently drops significant whitespace adjacent to entities (e.g. spaces around
    // &apos; or &amp;). soap-server emits compact XML so indentation whitespace is not a concern.
    reader.config_mut().trim_text(false);
    let mut writer = Writer::new(Cursor::new(Vec::new()));
    let mut stack: Vec<String> = Vec::new();
    let mut buf = Vec::new();

    loop {
        match reader
            .read_event_into(&mut buf)
            .map_err(|e| e.to_string())?
        {
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
                        .write_event(Event::Text(quick_xml::events::BytesText::new(
                            MASK_SENTINEL,
                        )))
                        .map_err(|err| err.to_string())?;
                } else {
                    writer
                        .write_event(Event::Text(t.to_owned()))
                        .map_err(|err| err.to_string())?;
                }
            }
            Event::CData(cd) => {
                let masked = masks.iter().any(|m| m.matches(&stack));
                if masked {
                    writer
                        .write_event(Event::Text(BytesText::new(MASK_SENTINEL)))
                        .map_err(|err| err.to_string())?;
                } else {
                    writer
                        .write_event(Event::CData(cd.into_owned()))
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

    #[test]
    fn attribute_values_are_not_double_escaped() {
        let out = normalize(br#"<E a="x &amp; y"/>"#, &[]).unwrap();
        assert!(out.contains("x &amp; y"), "got: {out}");
        assert!(!out.contains("&amp;amp;"), "double-escaped: {out}");
    }

    #[test]
    fn preserves_significant_whitespace_around_entities() {
        // quick-xml splits text at entity refs; normalization must NOT drop the
        // significant spaces adjacent to an entity (regression: trim_text was eating them).
        let xml = br#"<E>a &amp; b 'c' d</E>"#;
        let out = normalize(xml, &[]).unwrap();
        assert!(out.contains("a &amp; b"), "lost space around &amp;: {out}");
        // the ' characters re-serialize as &apos; (quick-xml writer) but spaces must remain:
        assert!(
            out.contains("b ") && out.contains(" d"),
            "lost surrounding spaces: {out}"
        );
    }

    #[test]
    fn masks_cdata_content_at_a_masked_path() {
        let xml = br#"<Envelope><Header><Nonce><![CDATA[AAAA]]></Nonce></Header></Envelope>"#;
        let rules = vec![MaskRule::new("Envelope/Header/Nonce")];
        let out = normalize(xml, &rules).unwrap();
        assert!(out.contains("__MASKED__"), "got: {out}");
        assert!(!out.contains("AAAA"), "unmasked CDATA leaked: {out}");
    }
}
