//! SOAP fault types and serialization for SOAP 1.1 and 1.2.
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
    pub fn as_str(&self) -> &'static str {
        match self {
            FaultCode::VersionMismatch => "env:VersionMismatch",
            FaultCode::MustUnderstand => "env:MustUnderstand",
            FaultCode::DataEncodingUnknown => "env:DataEncodingUnknown",
            FaultCode::Sender => "env:Sender",
            FaultCode::Receiver => "env:Receiver",
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
        Self::new(
            FaultCode::VersionMismatch,
            "SOAP version mismatch",
            None,
        )
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
    pub fn to_xml_bytes_versioned(&self, version: &crate::wsdl::definitions::SoapVersion) -> Vec<u8> {
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
        let reason = &self.reason;
        let detail_xml = match &self.detail {
            Some(detail) => format!("<detail>{detail}</detail>"),
            None => String::new(),
        };
        format!(
            r#"<SOAP-ENV:Envelope xmlns:SOAP-ENV="{ns}"><SOAP-ENV:Body><SOAP-ENV:Fault><faultcode>{faultcode}</faultcode><faultstring>{reason}</faultstring>{detail_xml}</SOAP-ENV:Fault></SOAP-ENV:Body></SOAP-ENV:Envelope>"#
        ).into_bytes()
    }

    /// Serialize to a complete SOAP 1.2 envelope XML string.
    /// HTTP status is always 500 per W3C SOAP 1.2 spec Section 7.4.2 (FLT-03).
    pub fn to_xml_bytes(&self) -> Vec<u8> {
        let code = self.code.as_str();
        let reason = &self.reason;

        let detail_xml = match &self.detail {
            Some(detail) => format!("<env:Detail>{detail}</env:Detail>"),
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
    fn fault_code_sender_as_str() {
        assert_eq!(FaultCode::Sender.as_str(), "env:Sender");
    }

    #[test]
    fn fault_code_receiver_as_str() {
        assert_eq!(FaultCode::Receiver.as_str(), "env:Receiver");
    }

    #[test]
    fn fault_code_version_mismatch_as_str() {
        assert_eq!(FaultCode::VersionMismatch.as_str(), "env:VersionMismatch");
    }

    #[test]
    fn fault_code_must_understand_as_str() {
        assert_eq!(FaultCode::MustUnderstand.as_str(), "env:MustUnderstand");
    }

    #[test]
    fn fault_code_data_encoding_unknown_as_str() {
        assert_eq!(
            FaultCode::DataEncodingUnknown.as_str(),
            "env:DataEncodingUnknown"
        );
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
            Some("<extra>info</extra>".to_string()),
        );
        let xml = String::from_utf8(fault.to_xml_bytes()).unwrap();
        assert!(
            xml.contains("<env:Detail><extra>info</extra></env:Detail>"),
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
        assert!(xml.contains("<faultcode>"), "Expected <faultcode>, got: {xml}");
        assert!(xml.contains("<faultstring>"), "Expected <faultstring>, got: {xml}");
        assert!(xml.contains("SOAP-ENV:Fault"), "Expected SOAP-ENV:Fault, got: {xml}");
        assert!(!xml.contains("<env:Code>"), "Should NOT contain <env:Code>, got: {xml}");
        assert!(!xml.contains("<env:Reason>"), "Should NOT contain <env:Reason>, got: {xml}");
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
        assert!(xml.starts_with("<SOAP-ENV:Envelope"), "Expected SOAP-ENV:Envelope root, got: {xml}");
        assert!(xml.contains("<SOAP-ENV:Body>"), "Expected <SOAP-ENV:Body>, got: {xml}");
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
            Some("<extra>info</extra>".to_string()),
        );
        let xml = String::from_utf8(fault.to_xml_bytes_v11()).unwrap();
        assert!(
            xml.contains("<detail><extra>info</extra></detail>"),
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
        assert_eq!(v12, existing, "Soap12 path should produce identical output to to_xml_bytes()");
    }

    #[test]
    fn to_xml_bytes_versioned_soap11_calls_v11() {
        use crate::wsdl::definitions::SoapVersion;
        let fault = SoapFault::sender("test");
        let v11_versioned = fault.to_xml_bytes_versioned(&SoapVersion::Soap11);
        let v11_direct = fault.to_xml_bytes_v11();
        assert_eq!(v11_versioned, v11_direct, "Soap11 path should produce identical output to to_xml_bytes_v11()");
    }
}
