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
