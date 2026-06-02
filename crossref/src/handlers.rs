//! Controlled-service request handlers shared between the Layer-1 in-process SUT
//! and the forthcoming Layer-2 server binary.

use bytes::Bytes;
use soap_server::FnHandler;

/// Resolve a standard XML predefined entity name to its character.
/// Returns `None` for unrecognised entity names (caller appends nothing).
fn resolve_predefined_entity(name: &str) -> Option<char> {
    match name {
        "lt" => Some('<'),
        "gt" => Some('>'),
        "amp" => Some('&'),
        "apos" => Some('\''),
        "quot" => Some('"'),
        _ => None,
    }
}

/// Extract the text content of the first element whose local name ends with `suffix`.
///
/// Accumulates all `Event::Text` and `Event::GeneralRef` fragments between the
/// target element's `Start` and its matching `End`, preserving significant
/// whitespace and faithfully decoding entity references (e.g. `&lt;` → `<`).
fn extract_first_text_by_suffix(body: &[u8], suffix: &str) -> Option<String> {
    use quick_xml::events::Event;
    use quick_xml::Reader;

    let mut reader = Reader::from_reader(body);
    // Do NOT trim — whitespace inside element content is significant.
    reader.config_mut().trim_text(false);
    let mut in_target = false;
    let mut accumulated = String::new();
    loop {
        match reader.read_event() {
            Ok(Event::Start(e)) => {
                let local = e.local_name();
                let local_str = std::str::from_utf8(local.as_ref()).unwrap_or("");
                if local_str.ends_with(suffix) {
                    in_target = true;
                    accumulated.clear();
                }
            }
            Ok(Event::Text(t)) if in_target => {
                accumulated.push_str(&t.decode().unwrap_or_default());
            }
            Ok(Event::GeneralRef(r)) if in_target => {
                let name = r.decode().unwrap_or_default();
                if let Some(ch) = resolve_predefined_entity(name.as_ref()) {
                    accumulated.push(ch);
                }
                // Unrecognised entity: append nothing (safe degradation).
            }
            Ok(Event::End(_)) if in_target => {
                return Some(accumulated);
            }
            Ok(Event::Eof) => return None,
            Err(_) => return None,
            _ => {}
        }
    }
}

pub fn extract_text(body: &[u8]) -> Option<String> {
    extract_first_text_by_suffix(body, "Text")
}

pub fn extract_value(body: &[u8]) -> Option<String> {
    extract_first_text_by_suffix(body, "Value")
}

pub fn echo_handler() -> impl soap_server::SoapHandler {
    FnHandler::new(|body: Bytes| async move {
        let text = extract_text(&body).unwrap_or_default();
        let escaped = soap_server::escape_text(&text);
        let resp = format!(
            r#"<c:EchoResponse xmlns:c="http://crossref.example/controlled"><c:Text>{escaped}</c:Text></c:EchoResponse>"#
        );
        Ok::<Bytes, soap_server::SoapFault>(Bytes::from(resp))
    })
}

pub fn echo_named_handler() -> impl soap_server::SoapHandler {
    FnHandler::new(|body: Bytes| async move {
        let value = extract_value(&body).unwrap_or_default();
        let escaped = soap_server::escape_text(&value);
        let resp = format!(
            r#"<c:EchoNamedResponse xmlns:c="http://crossref.example/controlled"><c:Value>{escaped}</c:Value></c:EchoNamedResponse>"#
        );
        Ok::<Bytes, soap_server::SoapFault>(Bytes::from(resp))
    })
}

/// Handler for the `Faulty` operation. Always returns a Sender fault whose
/// `<env:Detail>` contains a raw XML child element (not escaped text).
pub fn faulty_handler() -> impl soap_server::SoapHandler {
    FnHandler::new(|_body: Bytes| async move {
        Err::<Bytes, soap_server::SoapFault>(
            soap_server::SoapFault::sender("operation failed").with_detail_xml(
                r#"<c:ErrorInfo xmlns:c="http://crossref.example/controlled"><c:Field>missing-text</c:Field></c:ErrorInfo>"#,
            ),
        )
    })
}
