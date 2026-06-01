//! SOAP fault types and serialization for SOAP 1.1 and 1.2.
use crate::xml_escape::escape_text;
use thiserror::Error;

#[derive(Debug, Clone, PartialEq)]
pub enum FaultCode {
    VersionMismatch,
    MustUnderstand,
    DataEncodingUnknown,
    Sender,
    Receiver,
}

impl FaultCode {
    /// Returns the SOAP 1.2 fault code string (e.g. `"env:Sender"`, `"env:Receiver"`).
    /// Use this when serializing SOAP 1.2 responses.
    pub fn as_soap12_str(&self) -> &'static str {
        match self {
            FaultCode::VersionMismatch => "env:VersionMismatch",
            FaultCode::MustUnderstand => "env:MustUnderstand",
            FaultCode::DataEncodingUnknown => "env:DataEncodingUnknown",
            FaultCode::Sender => "env:Sender",
            FaultCode::Receiver => "env:Receiver",
        }
    }

    /// Returns the SOAP 1.1 fault code string (e.g. `"env:Client"`, `"env:Server"`).
    /// Use this when serializing SOAP 1.1 responses.
    pub fn as_soap11_str(&self) -> &'static str {
        match self {
            FaultCode::VersionMismatch => "env:VersionMismatch",
            FaultCode::MustUnderstand => "env:MustUnderstand",
            // DataEncodingUnknown has no SOAP 1.1 equivalent — map to Server per Apache CXF
            FaultCode::DataEncodingUnknown => "env:Server",
            FaultCode::Sender => "env:Client",
            FaultCode::Receiver => "env:Server",
        }
    }
}

#[derive(Debug, Clone, Error)]
#[error("SOAP Fault: {code:?} — {reason}")]
pub struct SoapFault {
    pub code: FaultCode,
    pub reason: String,
    pub detail: Option<String>,
}

impl SoapFault {
    pub fn new(code: FaultCode, reason: impl Into<String>, detail: Option<String>) -> Self {
        Self {
            code,
            reason: reason.into(),
            detail,
        }
    }

    pub fn sender(reason: impl Into<String>) -> Self {
        Self::new(FaultCode::Sender, reason, None)
    }

    pub fn receiver(reason: impl Into<String>) -> Self {
        Self::new(FaultCode::Receiver, reason, None)
    }

    pub fn version_mismatch() -> Self {
        Self::new(FaultCode::VersionMismatch, "SOAP version mismatch", None)
    }

    pub fn must_understand(header: &str) -> Self {
        Self::new(
            FaultCode::MustUnderstand,
            format!("Header not understood: {header}"),
            None,
        )
    }

    pub fn action_not_supported(action: &str) -> Self {
        Self::new(
            FaultCode::Sender,
            format!("Action not supported: {action}"),
            None,
        )
    }

    /// Serialize to a complete SOAP envelope. Version determines fault structure and code names.
    /// SOAP 1.2 uses nested Code/Reason (existing to_xml_bytes). SOAP 1.1 uses flat
    /// faultcode/faultstring per W3C SOAP 1.1 spec Section 4.4.
    pub fn to_xml_bytes_versioned(
        &self,
        version: &crate::wsdl::definitions::SoapVersion,
    ) -> Vec<u8> {
        match version {
            crate::wsdl::definitions::SoapVersion::Soap12 => self.to_xml_bytes(),
            crate::wsdl::definitions::SoapVersion::Soap11 => self.to_xml_bytes_v11(),
        }
    }

    fn to_xml_bytes_v11(&self) -> Vec<u8> {
        let ns = "http://schemas.xmlsoap.org/soap/envelope/";
        let faultcode = match &self.code {
            FaultCode::Sender => "SOAP-ENV:Client",
            FaultCode::Receiver => "SOAP-ENV:Server",
            FaultCode::VersionMismatch => "SOAP-ENV:VersionMismatch",
            FaultCode::MustUnderstand => "SOAP-ENV:MustUnderstand",
            // DataEncodingUnknown has no SOAP 1.1 equivalent — map to Server per Apache CXF
            FaultCode::DataEncodingUnknown => "SOAP-ENV:Server",
        };
        let reason = escape_text(&self.reason);
        let detail_xml = match &self.detail {
            Some(detail) => format!("<detail>{}</detail>", escape_text(detail)),
            None => String::new(),
        };
        format!(
            r#"<SOAP-ENV:Envelope xmlns:SOAP-ENV="{ns}"><SOAP-ENV:Body><SOAP-ENV:Fault><faultcode>{faultcode}</faultcode><faultstring>{reason}</faultstring>{detail_xml}</SOAP-ENV:Fault></SOAP-ENV:Body></SOAP-ENV:Envelope>"#
        ).into_bytes()
    }

    /// Serialize to a complete SOAP 1.2 envelope XML string.
    /// HTTP status is always 500 per W3C SOAP 1.2 spec Section 7.4.2 (FLT-03).
    pub fn to_xml_bytes(&self) -> Vec<u8> {
        let code = self.code.as_soap12_str();
        let reason = escape_text(&self.reason);

        let detail_xml = match &self.detail {
            Some(detail) => format!("<env:Detail>{}</env:Detail>", escape_text(detail)),
            None => String::new(),
        };

        let xml = format!(
            r#"<env:Envelope xmlns:env="http://www.w3.org/2003/05/soap-envelope"><env:Body><env:Fault><env:Code><env:Value>{code}</env:Value></env:Code><env:Reason><env:Text xml:lang="en">{reason}</env:Text></env:Reason>{detail_xml}</env:Fault></env:Body></env:Envelope>"#
        );

        xml.into_bytes()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fault_code_sender_as_soap12_str() {
        assert_eq!(FaultCode::Sender.as_soap12_str(), "env:Sender");
    }

    #[test]
    fn fault_code_receiver_as_soap12_str() {
        assert_eq!(FaultCode::Receiver.as_soap12_str(), "env:Receiver");
    }

    #[test]
    fn fault_code_version_mismatch_as_soap12_str() {
        assert_eq!(
            FaultCode::VersionMismatch.as_soap12_str(),
            "env:VersionMismatch"
        );
    }

    #[test]
    fn fault_code_must_understand_as_soap12_str() {
        assert_eq!(
            FaultCode::MustUnderstand.as_soap12_str(),
            "env:MustUnderstand"
        );
    }

    #[test]
    fn fault_code_data_encoding_unknown_as_soap12_str() {
        assert_eq!(
            FaultCode::DataEncodingUnknown.as_soap12_str(),
            "env:DataEncodingUnknown"
        );
    }

    #[test]
    fn fault_code_sender_as_soap11_str() {
        assert_eq!(FaultCode::Sender.as_soap11_str(), "env:Client");
    }

    #[test]
    fn fault_code_receiver_as_soap11_str() {
        assert_eq!(FaultCode::Receiver.as_soap11_str(), "env:Server");
    }

    #[test]
    fn fault_code_version_mismatch_as_soap11_str() {
        assert_eq!(
            FaultCode::VersionMismatch.as_soap11_str(),
            "env:VersionMismatch"
        );
    }

    #[test]
    fn fault_code_must_understand_as_soap11_str() {
        assert_eq!(
            FaultCode::MustUnderstand.as_soap11_str(),
            "env:MustUnderstand"
        );
    }

    #[test]
    fn fault_code_data_encoding_unknown_as_soap11_str() {
        // DataEncodingUnknown has no SOAP 1.1 equivalent — maps to env:Server
        assert_eq!(FaultCode::DataEncodingUnknown.as_soap11_str(), "env:Server");
    }

    #[test]
    fn serialize_sender_fault_contains_env_sender() {
        let fault = SoapFault::sender("Bad request");
        let xml = String::from_utf8(fault.to_xml_bytes()).unwrap();
        assert!(
            xml.contains("<env:Value>env:Sender</env:Value>"),
            "Expected env:Sender in XML, got: {xml}"
        );
    }

    #[test]
    fn serialize_receiver_fault_contains_env_receiver() {
        let fault = SoapFault::receiver("Internal error");
        let xml = String::from_utf8(fault.to_xml_bytes()).unwrap();
        assert!(
            xml.contains("<env:Value>env:Receiver</env:Value>"),
            "Expected env:Receiver in XML, got: {xml}"
        );
    }

    #[test]
    fn serialize_fault_wraps_in_soap12_envelope() {
        let fault = SoapFault::sender("test");
        let xml = String::from_utf8(fault.to_xml_bytes()).unwrap();
        assert!(
            xml.contains(r#"xmlns:env="http://www.w3.org/2003/05/soap-envelope""#),
            "Expected SOAP 1.2 namespace in XML, got: {xml}"
        );
        assert!(xml.starts_with("<env:Envelope"), "Expected envelope root");
        assert!(xml.contains("<env:Body>"), "Expected Body element");
        assert!(xml.contains("<env:Fault>"), "Expected Fault element");
    }

    #[test]
    fn serialize_fault_reason_included() {
        let fault = SoapFault::sender("Custom reason text");
        let xml = String::from_utf8(fault.to_xml_bytes()).unwrap();
        assert!(
            xml.contains("Custom reason text"),
            "Expected reason in XML, got: {xml}"
        );
        assert!(
            xml.contains(r#"<env:Text xml:lang="en">"#),
            "Expected Text element with lang"
        );
    }

    #[test]
    fn serialize_fault_no_detail_when_none() {
        let fault = SoapFault::sender("test");
        let xml = String::from_utf8(fault.to_xml_bytes()).unwrap();
        assert!(
            !xml.contains("<env:Detail>"),
            "Expected no Detail element when detail is None, got: {xml}"
        );
    }

    #[test]
    fn serialize_fault_with_detail() {
        let fault = SoapFault::new(
            FaultCode::Receiver,
            "Internal error",
            Some("extra info".to_string()),
        );
        let xml = String::from_utf8(fault.to_xml_bytes()).unwrap();
        assert!(
            xml.contains("<env:Detail>extra info</env:Detail>"),
            "Expected Detail element with content, got: {xml}"
        );
    }

    #[test]
    fn version_mismatch_convenience() {
        let fault = SoapFault::version_mismatch();
        assert_eq!(fault.code, FaultCode::VersionMismatch);
        let xml = String::from_utf8(fault.to_xml_bytes()).unwrap();
        assert!(xml.contains("<env:Value>env:VersionMismatch</env:Value>"));
    }

    #[test]
    fn must_understand_convenience() {
        let fault = SoapFault::must_understand("MyHeader");
        assert_eq!(fault.code, FaultCode::MustUnderstand);
        let xml = String::from_utf8(fault.to_xml_bytes()).unwrap();
        assert!(xml.contains("<env:Value>env:MustUnderstand</env:Value>"));
        assert!(xml.contains("MyHeader"));
    }

    #[test]
    fn action_not_supported_is_sender_fault() {
        let fault = SoapFault::action_not_supported("urn:SomeAction");
        assert_eq!(fault.code, FaultCode::Sender);
        let xml = String::from_utf8(fault.to_xml_bytes()).unwrap();
        assert!(xml.contains("<env:Value>env:Sender</env:Value>"));
        assert!(xml.contains("urn:SomeAction"));
    }

    // ── SOAP 1.1 fault tests (TDD RED) ───────────────────────────────────────

    #[test]
    fn fault_soap11_structure() {
        let fault = SoapFault::sender("bad");
        let xml = String::from_utf8(fault.to_xml_bytes_v11()).unwrap();
        assert!(
            xml.contains("<faultcode>"),
            "Expected <faultcode>, got: {xml}"
        );
        assert!(
            xml.contains("<faultstring>"),
            "Expected <faultstring>, got: {xml}"
        );
        assert!(
            xml.contains("SOAP-ENV:Fault"),
            "Expected SOAP-ENV:Fault, got: {xml}"
        );
        assert!(
            !xml.contains("<env:Code>"),
            "Should NOT contain <env:Code>, got: {xml}"
        );
        assert!(
            !xml.contains("<env:Reason>"),
            "Should NOT contain <env:Reason>, got: {xml}"
        );
    }

    #[test]
    fn fault_soap11_namespace() {
        let fault = SoapFault::sender("bad");
        let xml = String::from_utf8(fault.to_xml_bytes_v11()).unwrap();
        assert!(
            xml.contains(r#"xmlns:SOAP-ENV="http://schemas.xmlsoap.org/soap/envelope/""#),
            "Expected SOAP 1.1 namespace on Envelope, got: {xml}"
        );
    }

    #[test]
    fn fault_soap11_wraps_in_envelope() {
        let fault = SoapFault::sender("bad");
        let xml = String::from_utf8(fault.to_xml_bytes_v11()).unwrap();
        assert!(
            xml.starts_with("<SOAP-ENV:Envelope"),
            "Expected SOAP-ENV:Envelope root, got: {xml}"
        );
        assert!(
            xml.contains("<SOAP-ENV:Body>"),
            "Expected <SOAP-ENV:Body>, got: {xml}"
        );
    }

    #[test]
    fn fault_code_sender_maps_to_client() {
        let fault = SoapFault::sender("bad");
        let xml = String::from_utf8(fault.to_xml_bytes_v11()).unwrap();
        assert!(
            xml.contains("<faultcode>SOAP-ENV:Client</faultcode>"),
            "Expected SOAP-ENV:Client, got: {xml}"
        );
    }

    #[test]
    fn fault_code_receiver_maps_to_server() {
        let fault = SoapFault::receiver("internal");
        let xml = String::from_utf8(fault.to_xml_bytes_v11()).unwrap();
        assert!(
            xml.contains("<faultcode>SOAP-ENV:Server</faultcode>"),
            "Expected SOAP-ENV:Server, got: {xml}"
        );
    }

    #[test]
    fn fault_code_version_mismatch_soap11() {
        let fault = SoapFault::version_mismatch();
        let xml = String::from_utf8(fault.to_xml_bytes_v11()).unwrap();
        assert!(
            xml.contains("<faultcode>SOAP-ENV:VersionMismatch</faultcode>"),
            "Expected SOAP-ENV:VersionMismatch, got: {xml}"
        );
    }

    #[test]
    fn fault_code_must_understand_soap11() {
        let fault = SoapFault::must_understand("MyHeader");
        let xml = String::from_utf8(fault.to_xml_bytes_v11()).unwrap();
        assert!(
            xml.contains("<faultcode>SOAP-ENV:MustUnderstand</faultcode>"),
            "Expected SOAP-ENV:MustUnderstand, got: {xml}"
        );
    }

    #[test]
    fn fault_code_data_encoding_unknown_soap11() {
        let fault = SoapFault::new(FaultCode::DataEncodingUnknown, "enc error", None);
        let xml = String::from_utf8(fault.to_xml_bytes_v11()).unwrap();
        assert!(
            xml.contains("<faultcode>SOAP-ENV:Server</faultcode>"),
            "Expected SOAP-ENV:Server (no 1.1 equivalent for DataEncodingUnknown), got: {xml}"
        );
    }

    #[test]
    fn fault_soap11_with_detail() {
        let fault = SoapFault::new(
            FaultCode::Receiver,
            "Internal error",
            Some("extra info".to_string()),
        );
        let xml = String::from_utf8(fault.to_xml_bytes_v11()).unwrap();
        assert!(
            xml.contains("<detail>extra info</detail>"),
            "Expected <detail> element with content, got: {xml}"
        );
    }

    #[test]
    fn fault_soap11_no_detail_when_none() {
        let fault = SoapFault::sender("test");
        let xml = String::from_utf8(fault.to_xml_bytes_v11()).unwrap();
        assert!(
            !xml.contains("<detail>"),
            "Expected no <detail> when detail is None, got: {xml}"
        );
    }

    #[test]
    fn to_xml_bytes_versioned_soap12_calls_existing() {
        use crate::wsdl::definitions::SoapVersion;
        let fault = SoapFault::sender("test");
        let v12 = fault.to_xml_bytes_versioned(&SoapVersion::Soap12);
        let existing = fault.to_xml_bytes();
        assert_eq!(
            v12, existing,
            "Soap12 path should produce identical output to to_xml_bytes()"
        );
    }

    #[test]
    fn to_xml_bytes_versioned_soap11_calls_v11() {
        use crate::wsdl::definitions::SoapVersion;
        let fault = SoapFault::sender("test");
        let v11_versioned = fault.to_xml_bytes_versioned(&SoapVersion::Soap11);
        let v11_direct = fault.to_xml_bytes_v11();
        assert_eq!(
            v11_versioned, v11_direct,
            "Soap11 path should produce identical output to to_xml_bytes_v11()"
        );
    }

    // ── XML escaping tests (Finding #1) ──────────────────────────────────────

    /// Verify that reason and detail containing all five XML special characters
    /// (`& < > " '`) are properly escaped and the resulting document parses as
    /// well-formed XML.  Uses quick_xml to parse the envelope so any unescaped
    /// entity or unbound prefix would surface as a parse error.
    #[test]
    fn soap12_fault_special_chars_in_reason_and_detail_are_escaped() {
        use quick_xml::events::Event;
        use quick_xml::Reader;

        let special = r#"& < > " '"#;
        let fault = SoapFault::new(FaultCode::Sender, special, Some(special.to_string()));
        let xml_bytes = fault.to_xml_bytes();
        let xml_str = String::from_utf8(xml_bytes.clone()).unwrap();

        // The raw ampersand must NOT appear unescaped in the output.
        // We check that the literal "& " (ampersand-space) is absent — the namespace
        // URI http://... does not contain "& ", so any match is from the dynamic value.
        assert!(
            !xml_str.contains("& "),
            "Unescaped '& ' found in SOAP 1.2 fault XML: {xml_str}"
        );
        // The escaped forms must be present.
        assert!(xml_str.contains("&amp;"), "Expected &amp;: {xml_str}");
        assert!(xml_str.contains("&lt;"), "Expected &lt;: {xml_str}");
        assert!(xml_str.contains("&gt;"), "Expected &gt;: {xml_str}");
        assert!(xml_str.contains("&quot;"), "Expected &quot;: {xml_str}");
        assert!(xml_str.contains("&apos;"), "Expected &apos;: {xml_str}");

        // Parse with quick_xml to confirm the document is well-formed XML.
        // Any unescaped `<` or bare `&` would cause a parse error here (the
        // expect() would panic with an XML error rather than the assertions below).
        // Parse with quick_xml to confirm the document is well-formed XML.
        // A bare `&` or unescaped `<` would cause a parse error on the expect().
        let mut reader = Reader::from_str(&xml_str);
        reader.config_mut().trim_text(true);
        let mut buf = Vec::new();
        let mut event_count = 0usize;
        loop {
            let is_eof = {
                let ev = reader
                    .read_event_into(&mut buf)
                    .expect("XML must parse as well-formed");
                matches!(ev, Event::Eof)
            };
            buf.clear();
            if is_eof {
                break;
            }
            event_count += 1;
        }
        assert!(event_count > 0, "Expected at least one XML event");
    }

    #[test]
    fn soap11_fault_special_chars_in_reason_and_detail_are_escaped() {
        use quick_xml::events::Event;
        use quick_xml::Reader;

        let special = r#"& < > " '"#;
        let fault = SoapFault::new(FaultCode::Sender, special, Some(special.to_string()));
        let xml_bytes = fault.to_xml_bytes_v11();
        let xml_str = String::from_utf8(xml_bytes).unwrap();

        assert!(
            !xml_str.contains("& "),
            "Unescaped '& ' found in SOAP 1.1 fault XML: {xml_str}"
        );
        assert!(xml_str.contains("&amp;"), "Expected &amp;: {xml_str}");
        assert!(xml_str.contains("&lt;"), "Expected &lt;: {xml_str}");

        // Parse to confirm well-formed.
        let mut reader = Reader::from_str(&xml_str);
        reader.config_mut().trim_text(true);
        let mut buf = Vec::new();
        loop {
            let is_eof = {
                let ev = reader.read_event_into(&mut buf).expect("XML must parse");
                matches!(ev, Event::Eof)
            };
            buf.clear();
            if is_eof {
                break;
            }
        }
    }
}
