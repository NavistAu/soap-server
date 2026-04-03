// TODO: SOAP 1.2 fault generation
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
}
