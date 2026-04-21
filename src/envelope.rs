//! SOAP envelope parsing and serialization for SOAP 1.1 and 1.2.

use bytes::Bytes;
use crate::fault::SoapFault;
use crate::wsdl::definitions::SoapVersion;

#[derive(Debug)]
pub struct ParsedEnvelope {
    pub soap_version: SoapVersion,
    /// Raw bytes for each child of soap:Header (may be empty if no Header)
    pub header_children: Vec<Bytes>,
    /// The Body first child element as self-contained XML bytes.
    /// Ancestor namespace declarations are re-emitted on the fragment root.
    pub body_element: Bytes,
}

/// Detect SOAP version from Content-Type header value.
pub fn detect_soap_version(content_type: &str) -> Result<SoapVersion, SoapFault> {
    if content_type.contains("application/soap+xml") {
        Ok(SoapVersion::Soap12)
    } else if content_type.contains("text/xml") {
        Ok(SoapVersion::Soap11)
    } else {
        Err(SoapFault::version_mismatch())
    }
}

/// Parse a SOAP envelope from raw bytes.
pub fn parse_envelope(input: &[u8]) -> Result<ParsedEnvelope, SoapFault> {
    use quick_xml::NsReader;
    use quick_xml::events::Event;

    const SOAP12_NS: &[u8] = b"http://www.w3.org/2003/05/soap-envelope";
    const SOAP11_NS: &[u8] = b"http://schemas.xmlsoap.org/soap/envelope/";

    let mut reader = NsReader::from_reader(input);
    reader.config_mut().trim_text(true);

    // Step 1: Find the Envelope start element
    let soap_version;
    let mut envelope_ns_bindings: Vec<(String, String)> = Vec::new();

    loop {
        match reader.read_resolved_event().map_err(|e| SoapFault::sender(format!("XML parse error: {e}")))? {
            (_, Event::Eof) => return Err(SoapFault::sender("Missing SOAP Envelope element")),
            (resolved_ns, Event::Start(e)) => {
                let local = e.local_name();
                if local.as_ref() == b"Envelope" {
                    let ns_bytes = match resolved_ns {
                        quick_xml::name::ResolveResult::Bound(ns) => ns.0.to_vec(),
                        _ => Vec::new(),
                    };
                    if ns_bytes == SOAP12_NS {
                        soap_version = SoapVersion::Soap12;
                    } else if ns_bytes == SOAP11_NS {
                        soap_version = SoapVersion::Soap11;
                    } else {
                        return Err(SoapFault::sender("Unknown SOAP Envelope namespace"));
                    }
                    // Collect namespace bindings by inspecting attributes
                    for attr in e.attributes() {
                        let attr = attr.map_err(|_| SoapFault::sender("Invalid attribute"))?;
                        let key = std::str::from_utf8(attr.key.as_ref()).unwrap_or("");
                        if key.starts_with("xmlns:") {
                            let prefix = key.trim_start_matches("xmlns:").to_string();
                            let value = std::str::from_utf8(attr.value.as_ref()).unwrap_or("").to_string();
                            envelope_ns_bindings.push((prefix, value));
                        } else if key == "xmlns" {
                            envelope_ns_bindings.push((String::new(), std::str::from_utf8(attr.value.as_ref()).unwrap_or("").to_string()));
                        }
                    }
                    break;
                }
                // Skip any non-Envelope start elements (e.g., XML declaration processing)
            }
            _ => {}
        }
    }

    // Step 2: Scan Envelope children for Header and Body
    let mut header_children: Vec<Bytes> = Vec::new();
    let mut body_element: Option<Bytes> = None;
    let mut found_body = false;

    loop {
        match reader.read_resolved_event().map_err(|e| SoapFault::sender(format!("XML parse error: {e}")))? {
            (_, Event::Eof) => break,
            (resolved_ns, Event::Start(e)) => {
                let ns_bytes = match &resolved_ns {
                    quick_xml::name::ResolveResult::Bound(ns) => ns.0.to_vec(),
                    _ => Vec::new(),
                };
                let local = e.local_name();
                let is_soap_ns = ns_bytes == SOAP12_NS || ns_bytes == SOAP11_NS;

                if is_soap_ns && local.as_ref() == b"Header" {
                    // Collect all Header children
                    header_children = collect_header_children(&mut reader, &envelope_ns_bindings)?;
                } else if is_soap_ns && local.as_ref() == b"Body" {
                    found_body = true;
                    // Extract first child of Body
                    body_element = Some(extract_body_first_child(&mut reader, &envelope_ns_bindings, &soap_version)?);
                    // Skip until Body end
                    skip_to_end(&mut reader, b"Body")?;
                } else {
                    // Skip unknown Envelope children
                    skip_element(&mut reader)?;
                }
            }
            (_, Event::End(_)) => break, // End of Envelope
            _ => {}
        }
    }

    if !found_body {
        return Err(SoapFault::sender("Missing Body"));
    }

    let body_element = body_element.unwrap_or_default();

    Ok(ParsedEnvelope {
        soap_version,
        header_children,
        body_element,
    })
}

fn collect_header_children(
    reader: &mut quick_xml::NsReader<&[u8]>,
    ns_bindings: &[(String, String)],
) -> Result<Vec<Bytes>, SoapFault> {
    use quick_xml::events::Event;
    let mut children = Vec::new();
    let mut depth = 0i32;
    let mut current_buf = Vec::new();

    loop {
        match reader.read_resolved_event().map_err(|e| SoapFault::sender(format!("XML parse error: {e}")))? {
            (_, Event::Eof) => return Err(SoapFault::sender("Unexpected EOF in Header")),
            (_, Event::Start(e)) => {
                if depth == 0 {
                    // New child element — start collecting.
                    // Re-emit envelope namespace bindings on the child root so that
                    // parsers consuming the extracted bytes can resolve prefixes like
                    // wsse:, wsu:, tds: etc. that were declared on the Envelope element.
                    current_buf.clear();
                    current_buf.extend_from_slice(b"<");
                    current_buf.extend_from_slice(e.name().as_ref());

                    // Collect the element's own attribute keys (to avoid double-declaring).
                    let mut own_attr_keys: std::collections::HashSet<String> = std::collections::HashSet::new();
                    let own_attrs: Vec<_> = e.attributes().filter_map(|a| a.ok()).collect();
                    for attr in &own_attrs {
                        own_attr_keys.insert(
                            std::str::from_utf8(attr.key.as_ref()).unwrap_or("").to_string()
                        );
                    }

                    // Emit inherited namespace bindings first (skip if already declared on element).
                    for (prefix, uri) in ns_bindings {
                        let key = if prefix.is_empty() {
                            "xmlns".to_string()
                        } else {
                            format!("xmlns:{prefix}")
                        };
                        if !own_attr_keys.contains(&key) {
                            current_buf.extend_from_slice(b" ");
                            current_buf.extend_from_slice(key.as_bytes());
                            current_buf.extend_from_slice(b"=\"");
                            current_buf.extend_from_slice(uri.as_bytes());
                            current_buf.extend_from_slice(b"\"");
                        }
                    }

                    // Emit element's own attributes.
                    for attr in &own_attrs {
                        current_buf.extend_from_slice(b" ");
                        current_buf.extend_from_slice(attr.key.as_ref());
                        current_buf.extend_from_slice(b"=\"");
                        current_buf.extend_from_slice(attr.value.as_ref());
                        current_buf.extend_from_slice(b"\"");
                    }
                    current_buf.extend_from_slice(b">");
                } else {
                    current_buf.extend_from_slice(b"<");
                    current_buf.extend_from_slice(e.name().as_ref());
                    for attr in e.attributes() {
                        let attr = attr.map_err(|_| SoapFault::sender("Invalid attribute"))?;
                        current_buf.extend_from_slice(b" ");
                        current_buf.extend_from_slice(attr.key.as_ref());
                        current_buf.extend_from_slice(b"=\"");
                        current_buf.extend_from_slice(attr.value.as_ref());
                        current_buf.extend_from_slice(b"\"");
                    }
                    current_buf.extend_from_slice(b">");
                }
                depth += 1;
            }
            (_, Event::Empty(e)) => {
                if depth == 0 {
                    // Self-closing child element
                    current_buf.clear();
                    current_buf.extend_from_slice(b"<");
                    current_buf.extend_from_slice(e.name().as_ref());
                    for attr in e.attributes() {
                        let attr = attr.map_err(|_| SoapFault::sender("Invalid attribute"))?;
                        current_buf.extend_from_slice(b" ");
                        current_buf.extend_from_slice(attr.key.as_ref());
                        current_buf.extend_from_slice(b"=\"");
                        current_buf.extend_from_slice(attr.value.as_ref());
                        current_buf.extend_from_slice(b"\"");
                    }
                    current_buf.extend_from_slice(b"/>");
                    children.push(Bytes::copy_from_slice(&current_buf));
                    current_buf.clear();
                } else {
                    current_buf.extend_from_slice(b"<");
                    current_buf.extend_from_slice(e.name().as_ref());
                    for attr in e.attributes() {
                        let attr = attr.map_err(|_| SoapFault::sender("Invalid attribute"))?;
                        current_buf.extend_from_slice(b" ");
                        current_buf.extend_from_slice(attr.key.as_ref());
                        current_buf.extend_from_slice(b"=\"");
                        current_buf.extend_from_slice(attr.value.as_ref());
                        current_buf.extend_from_slice(b"\"");
                    }
                    current_buf.extend_from_slice(b"/>");
                }
            }
            (_, Event::End(e)) => {
                if depth == 0 {
                    // End of Header element itself
                    break;
                }
                depth -= 1;
                current_buf.extend_from_slice(b"</");
                current_buf.extend_from_slice(e.name().as_ref());
                current_buf.extend_from_slice(b">");
                if depth == 0 {
                    // Finished a child element
                    children.push(Bytes::copy_from_slice(&current_buf));
                    current_buf.clear();
                }
            }
            (_, Event::Text(t)) => {
                if depth > 0 {
                    current_buf.extend_from_slice(t.as_ref());
                }
            }
            _ => {}
        }
    }

    Ok(children)
}

/// Extract the first child of Body with all in-scope namespace declarations re-emitted.
fn extract_body_first_child(
    reader: &mut quick_xml::NsReader<&[u8]>,
    envelope_ns_bindings: &[(String, String)],
    _soap_version: &SoapVersion,
) -> Result<Bytes, SoapFault> {
    use quick_xml::events::Event;

    // Find the first start element inside Body
    loop {
        match reader.read_resolved_event().map_err(|e| SoapFault::sender(format!("XML parse error: {e}")))? {
            (_, Event::Eof) => return Err(SoapFault::sender("Missing Body child element")),
            (_, Event::Start(e)) => {
                // Found first child — build self-contained bytes
                let mut buf = Vec::new();
                buf.extend_from_slice(b"<");
                buf.extend_from_slice(e.name().as_ref());

                // Collect the element's own xmlns attributes
                let mut own_prefixes = std::collections::HashSet::new();
                let mut attr_buf = Vec::new();
                for attr in e.attributes() {
                    let attr = attr.map_err(|_| SoapFault::sender("Invalid attribute"))?;
                    let key = std::str::from_utf8(attr.key.as_ref()).unwrap_or("");
                    if key.starts_with("xmlns:") {
                        let prefix = key.trim_start_matches("xmlns:").to_string();
                        own_prefixes.insert(prefix);
                    } else if key == "xmlns" {
                        own_prefixes.insert(String::new());
                    }
                    attr_buf.extend_from_slice(b" ");
                    attr_buf.extend_from_slice(attr.key.as_ref());
                    attr_buf.extend_from_slice(b"=\"");
                    attr_buf.extend_from_slice(attr.value.as_ref());
                    attr_buf.extend_from_slice(b"\"");
                }

                // Re-emit ancestor namespace declarations not overridden by this element
                for (prefix, uri) in envelope_ns_bindings {
                    if !own_prefixes.contains(prefix.as_str()) {
                        if prefix.is_empty() {
                            buf.extend_from_slice(b" xmlns=\"");
                        } else {
                            buf.extend_from_slice(b" xmlns:");
                            buf.extend_from_slice(prefix.as_bytes());
                            buf.extend_from_slice(b"=\"");
                        }
                        buf.extend_from_slice(uri.as_bytes());
                        buf.extend_from_slice(b"\"");
                    }
                }

                // Now append the element's own attributes
                buf.extend_from_slice(&attr_buf);
                buf.extend_from_slice(b">");

                // Collect remaining content until matching end tag
                let mut depth = 1i32;
                loop {
                    match reader.read_resolved_event().map_err(|e| SoapFault::sender(format!("XML parse error: {e}")))? {
                        (_, Event::Eof) => return Err(SoapFault::sender("Unexpected EOF in Body child")),
                        (_, Event::Start(e2)) => {
                            depth += 1;
                            buf.extend_from_slice(b"<");
                            buf.extend_from_slice(e2.name().as_ref());
                            for attr in e2.attributes() {
                                let attr = attr.map_err(|_| SoapFault::sender("Invalid attribute"))?;
                                buf.extend_from_slice(b" ");
                                buf.extend_from_slice(attr.key.as_ref());
                                buf.extend_from_slice(b"=\"");
                                buf.extend_from_slice(attr.value.as_ref());
                                buf.extend_from_slice(b"\"");
                            }
                            buf.extend_from_slice(b">");
                        }
                        (_, Event::Empty(e2)) => {
                            buf.extend_from_slice(b"<");
                            buf.extend_from_slice(e2.name().as_ref());
                            for attr in e2.attributes() {
                                let attr = attr.map_err(|_| SoapFault::sender("Invalid attribute"))?;
                                buf.extend_from_slice(b" ");
                                buf.extend_from_slice(attr.key.as_ref());
                                buf.extend_from_slice(b"=\"");
                                buf.extend_from_slice(attr.value.as_ref());
                                buf.extend_from_slice(b"\"");
                            }
                            buf.extend_from_slice(b"/>");
                        }
                        (_, Event::End(e2)) => {
                            depth -= 1;
                            if depth == 0 {
                                buf.extend_from_slice(b"</");
                                buf.extend_from_slice(e2.name().as_ref());
                                buf.extend_from_slice(b">");
                                break;
                            }
                            buf.extend_from_slice(b"</");
                            buf.extend_from_slice(e2.name().as_ref());
                            buf.extend_from_slice(b">");
                        }
                        (_, Event::Text(t)) => {
                            buf.extend_from_slice(t.as_ref());
                        }
                        _ => {}
                    }
                }

                return Ok(Bytes::from(buf));
            }
            (_, Event::Empty(e)) => {
                // Self-closing first child
                let mut buf = Vec::new();
                buf.extend_from_slice(b"<");
                buf.extend_from_slice(e.name().as_ref());

                let mut own_prefixes = std::collections::HashSet::new();
                let mut attr_buf = Vec::new();
                for attr in e.attributes() {
                    let attr = attr.map_err(|_| SoapFault::sender("Invalid attribute"))?;
                    let key = std::str::from_utf8(attr.key.as_ref()).unwrap_or("");
                    if key.starts_with("xmlns:") {
                        let prefix = key.trim_start_matches("xmlns:").to_string();
                        own_prefixes.insert(prefix);
                    } else if key == "xmlns" {
                        own_prefixes.insert(String::new());
                    }
                    attr_buf.extend_from_slice(b" ");
                    attr_buf.extend_from_slice(attr.key.as_ref());
                    attr_buf.extend_from_slice(b"=\"");
                    attr_buf.extend_from_slice(attr.value.as_ref());
                    attr_buf.extend_from_slice(b"\"");
                }

                for (prefix, uri) in envelope_ns_bindings {
                    if !own_prefixes.contains(prefix.as_str()) {
                        if prefix.is_empty() {
                            buf.extend_from_slice(b" xmlns=\"");
                        } else {
                            buf.extend_from_slice(b" xmlns:");
                            buf.extend_from_slice(prefix.as_bytes());
                            buf.extend_from_slice(b"=\"");
                        }
                        buf.extend_from_slice(uri.as_bytes());
                        buf.extend_from_slice(b"\"");
                    }
                }
                buf.extend_from_slice(&attr_buf);
                buf.extend_from_slice(b"/>");
                return Ok(Bytes::from(buf));
            }
            (_, Event::End(_)) => {
                // End of Body with no children
                return Err(SoapFault::sender("Missing Body"));
            }
            _ => {}
        }
    }
}

fn skip_to_end(reader: &mut quick_xml::NsReader<&[u8]>, _tag: &[u8]) -> Result<(), SoapFault> {
    use quick_xml::events::Event;
    let mut depth = 1i32;
    loop {
        match reader.read_resolved_event().map_err(|e| SoapFault::sender(format!("XML parse error: {e}")))? {
            (_, Event::Eof) => return Err(SoapFault::sender("Unexpected EOF")),
            (_, Event::Start(_)) => depth += 1,
            (_, Event::End(_)) => {
                depth -= 1;
                if depth == 0 {
                    break;
                }
            }
            _ => {}
        }
    }
    Ok(())
}

fn skip_element(reader: &mut quick_xml::NsReader<&[u8]>) -> Result<(), SoapFault> {
    skip_to_end(reader, b"")
}

/// Serialize body bytes into a SOAP envelope.
pub fn serialize_envelope(body: Bytes, version: SoapVersion) -> Bytes {
    let (ns, prefix) = match version {
        SoapVersion::Soap12 => ("http://www.w3.org/2003/05/soap-envelope", "env"),
        SoapVersion::Soap11 => ("http://schemas.xmlsoap.org/soap/envelope/", "env"),
    };

    let mut buf = Vec::new();
    buf.extend_from_slice(format!(
        "<{prefix}:Envelope xmlns:{prefix}=\"{ns}\"><{prefix}:Body>"
    ).as_bytes());
    buf.extend_from_slice(&body);
    buf.extend_from_slice(format!(
        "</{prefix}:Body></{prefix}:Envelope>"
    ).as_bytes());

    Bytes::from(buf)
}

/// Returns the appropriate Content-Type for SOAP responses.
pub fn response_content_type(version: &SoapVersion) -> &'static str {
    match version {
        SoapVersion::Soap12 => "application/soap+xml; charset=utf-8",
        SoapVersion::Soap11 => "text/xml; charset=utf-8",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // detect_soap_version tests
    #[test]
    fn detect_soap12_from_application_soap_xml() {
        assert_eq!(
            detect_soap_version("application/soap+xml").unwrap(),
            SoapVersion::Soap12
        );
    }

    #[test]
    fn detect_soap12_from_content_type_with_action() {
        assert_eq!(
            detect_soap_version("application/soap+xml; action=\"urn:op\"").unwrap(),
            SoapVersion::Soap12
        );
    }

    #[test]
    fn detect_soap11_from_text_xml() {
        assert_eq!(
            detect_soap_version("text/xml").unwrap(),
            SoapVersion::Soap11
        );
    }

    #[test]
    fn detect_version_mismatch_for_unknown() {
        let result = detect_soap_version("application/json");
        assert!(result.is_err());
        let fault = result.unwrap_err();
        assert_eq!(fault.code, crate::fault::FaultCode::VersionMismatch);
    }

    // parse_envelope tests
    #[test]
    fn parse_envelope_minimal_soap12_empty_body_child() {
        let xml = r#"<env:Envelope xmlns:env="http://www.w3.org/2003/05/soap-envelope">
  <env:Body>
    <op:DoSomething xmlns:op="urn:test"/>
  </env:Body>
</env:Envelope>"#;
        let parsed = parse_envelope(xml.as_bytes()).unwrap();
        assert_eq!(parsed.soap_version, SoapVersion::Soap12);
        assert!(parsed.header_children.is_empty());
        let body_str = std::str::from_utf8(&parsed.body_element).unwrap();
        assert!(body_str.contains("DoSomething"), "body: {body_str}");
    }

    #[test]
    fn parse_envelope_with_header_child() {
        let xml = r#"<env:Envelope xmlns:env="http://www.w3.org/2003/05/soap-envelope">
  <env:Header>
    <wsse:Security xmlns:wsse="urn:wssec"/>
  </env:Header>
  <env:Body>
    <op:GetCapabilities xmlns:op="urn:onvif"/>
  </env:Body>
</env:Envelope>"#;
        let parsed = parse_envelope(xml.as_bytes()).unwrap();
        assert_eq!(parsed.header_children.len(), 1);
        let header_str = std::str::from_utf8(&parsed.header_children[0]).unwrap();
        assert!(header_str.contains("Security"), "header: {header_str}");
    }

    #[test]
    fn parse_envelope_body_bytes_contain_ancestor_ns_declarations() {
        let xml = r#"<env:Envelope xmlns:env="http://www.w3.org/2003/05/soap-envelope" xmlns:tds="http://www.onvif.org/ver10/device/wsdl">
  <env:Body>
    <tds:GetCapabilities/>
  </env:Body>
</env:Envelope>"#;
        let parsed = parse_envelope(xml.as_bytes()).unwrap();
        let body_str = std::str::from_utf8(&parsed.body_element).unwrap();
        // Should have tds namespace re-emitted since it was on Envelope
        assert!(
            body_str.contains("xmlns:tds") && body_str.contains("http://www.onvif.org/ver10/device/wsdl"),
            "body_element should contain tds namespace declaration, got: {body_str}"
        );
    }

    #[test]
    fn serialize_envelope_wraps_body_in_soap12() {
        let body = Bytes::from_static(b"<op:Foo/>");
        let envelope = serialize_envelope(body, SoapVersion::Soap12);
        let s = std::str::from_utf8(&envelope).unwrap();
        assert!(s.contains(r#"xmlns:env="http://www.w3.org/2003/05/soap-envelope""#), "got: {s}");
        assert!(s.contains("<env:Body>"), "got: {s}");
        assert!(s.contains("<op:Foo/>"), "got: {s}");
        assert!(s.starts_with("<env:Envelope"), "got: {s}");
    }

    #[test]
    fn parse_envelope_missing_envelope_element_returns_err() {
        let xml = r#"<notsoap:Root xmlns:notsoap="urn:x"><notsoap:Body/></notsoap:Root>"#;
        let result = parse_envelope(xml.as_bytes());
        assert!(result.is_err(), "Expected error for non-SOAP envelope");
    }

    #[test]
    fn parse_envelope_missing_body_returns_err() {
        let xml = r#"<env:Envelope xmlns:env="http://www.w3.org/2003/05/soap-envelope">
  <env:Header/>
</env:Envelope>"#;
        let result = parse_envelope(xml.as_bytes());
        assert!(result.is_err());
        let fault = result.unwrap_err();
        assert!(fault.reason.contains("Missing Body"), "got: {}", fault.reason);
    }

    // SOAP 1.1 tests

    #[test]
    fn parse_envelope_soap11() {
        let xml = r#"<SOAP-ENV:Envelope xmlns:SOAP-ENV="http://schemas.xmlsoap.org/soap/envelope/">
  <SOAP-ENV:Body>
    <op:DoSomething xmlns:op="urn:test"/>
  </SOAP-ENV:Body>
</SOAP-ENV:Envelope>"#;
        let parsed = parse_envelope(xml.as_bytes()).unwrap();
        assert_eq!(parsed.soap_version, SoapVersion::Soap11);
        assert!(parsed.header_children.is_empty());
    }

    #[test]
    fn parse_envelope_soap11_with_header() {
        let xml = r#"<SOAP-ENV:Envelope xmlns:SOAP-ENV="http://schemas.xmlsoap.org/soap/envelope/">
  <SOAP-ENV:Header>
    <wsse:Security xmlns:wsse="urn:wssec"/>
  </SOAP-ENV:Header>
  <SOAP-ENV:Body>
    <op:DoSomething xmlns:op="urn:test"/>
  </SOAP-ENV:Body>
</SOAP-ENV:Envelope>"#;
        let parsed = parse_envelope(xml.as_bytes()).unwrap();
        assert_eq!(parsed.soap_version, SoapVersion::Soap11);
        assert_eq!(parsed.header_children.len(), 1);
    }

    #[test]
    fn parse_envelope_soap11_body_first_child() {
        let xml = r#"<SOAP-ENV:Envelope xmlns:SOAP-ENV="http://schemas.xmlsoap.org/soap/envelope/">
  <SOAP-ENV:Body>
    <op:DoSomething xmlns:op="urn:test"/>
  </SOAP-ENV:Body>
</SOAP-ENV:Envelope>"#;
        let parsed = parse_envelope(xml.as_bytes()).unwrap();
        let body_str = std::str::from_utf8(&parsed.body_element).unwrap();
        assert!(body_str.contains("DoSomething"), "body: {body_str}");
    }

    #[test]
    fn serialize_envelope_soap11() {
        let body = Bytes::from_static(b"<op:Foo/>");
        let envelope = serialize_envelope(body, SoapVersion::Soap11);
        let s = std::str::from_utf8(&envelope).unwrap();
        assert!(
            s.contains("http://schemas.xmlsoap.org/soap/envelope/"),
            "envelope should contain SOAP 1.1 namespace, got: {s}"
        );
        assert!(s.contains("<env:Body>"), "got: {s}");
        assert!(s.contains("<op:Foo/>"), "got: {s}");
        assert!(s.starts_with("<env:Envelope"), "got: {s}");
    }

    #[test]
    fn response_content_type_soap11() {
        assert_eq!(
            response_content_type(&SoapVersion::Soap11),
            "text/xml; charset=utf-8"
        );
    }

    #[test]
    fn response_content_type_soap12() {
        assert_eq!(
            response_content_type(&SoapVersion::Soap12),
            "application/soap+xml; charset=utf-8"
        );
    }
}
