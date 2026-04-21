// WSDL definition types — Service, Port, Binding, PortType, Operation, Message
use crate::qname::QName;
use std::collections::HashMap;

/// Top-level WSDL 1.1 document.
#[derive(Debug, Clone)]
pub struct WsdlDefinition {
    pub target_namespace: String,
    pub imports: Vec<WsdlImport>,
    pub types: TypesSection,
    pub messages: HashMap<String, Message>,
    pub port_types: HashMap<String, PortType>,
    pub bindings: HashMap<String, Binding>,
    pub services: HashMap<String, Service>,
}

/// The <wsdl:types> section containing embedded XSD schemas.
#[derive(Debug, Clone, Default)]
pub struct TypesSection {
    /// Raw XML strings of embedded <xs:schema> elements, one per schema block.
    pub schemas: Vec<String>,
}

/// A <wsdl:message> element.
#[derive(Debug, Clone)]
pub struct Message {
    pub name: String,
    pub parts: Vec<MessagePart>,
}

/// A <wsdl:part> element within a message.
#[derive(Debug, Clone)]
pub struct MessagePart {
    pub name: String,
    /// Document/literal style: element reference.
    pub element: Option<QName>,
    /// RPC style: type reference.
    pub type_ref: Option<QName>,
}

/// A <wsdl:portType> element (abstract interface).
#[derive(Debug, Clone)]
pub struct PortType {
    pub name: String,
    pub operations: Vec<Operation>,
}

/// A <wsdl:operation> within a portType.
#[derive(Debug, Clone)]
pub struct Operation {
    pub name: String,
    pub input: Option<OperationMessage>,
    pub output: Option<OperationMessage>,
    pub faults: Vec<OperationFault>,
    pub style: OperationStyle,
}

/// The MEP (Message Exchange Pattern) for an operation.
#[derive(Debug, Clone, PartialEq)]
pub enum OperationStyle {
    OneWay,
    RequestResponse,
    SolicitResponse,
    Notification,
}

/// Input or output message reference in an operation.
#[derive(Debug, Clone)]
pub struct OperationMessage {
    pub name: Option<String>,
    pub message: QName,
}

/// A fault message reference in an operation.
#[derive(Debug, Clone)]
pub struct OperationFault {
    pub name: String,
    pub message: QName,
}

/// A <wsdl:binding> element (concrete protocol binding for a portType).
#[derive(Debug, Clone)]
pub struct Binding {
    pub name: String,
    pub port_type: QName,
    pub soap_binding: SoapBinding,
    pub operations: Vec<BindingOperation>,
}

/// SOAP-specific binding information (style and transport).
#[derive(Debug, Clone)]
pub struct SoapBinding {
    pub style: BindingStyle,
    pub transport: String,
    pub soap_version: SoapVersion,
}

/// WSDL binding style.
#[derive(Debug, Clone, PartialEq)]
pub enum BindingStyle {
    Document,
    Rpc,
}

/// SOAP version for a binding.
#[derive(Debug, Clone, PartialEq)]
pub enum SoapVersion {
    Soap11,
    Soap12,
}

/// A bound operation within a <wsdl:binding>.
#[derive(Debug, Clone)]
pub struct BindingOperation {
    pub name: String,
    pub soap_action: String,
    pub input: BindingMessage,
    pub output: BindingMessage,
}

/// SOAP message binding (body + optional headers).
#[derive(Debug, Clone)]
pub struct BindingMessage {
    pub body: SoapBody,
    pub headers: Vec<SoapHeader>,
}

/// The <soap:body> element in a binding.
#[derive(Debug, Clone)]
pub struct SoapBody {
    pub use_attr: UseStyle,
    pub namespace: Option<String>,
    pub encoding_style: Option<String>,
}

/// The use attribute for soap:body and soap:header.
#[derive(Debug, Clone, PartialEq)]
pub enum UseStyle {
    Literal,
    Encoded,
}

/// A <soap:header> element in a binding message.
#[derive(Debug, Clone)]
pub struct SoapHeader {
    pub message: QName,
    pub part: String,
}

/// A <wsdl:service> element.
#[derive(Debug, Clone)]
pub struct Service {
    pub name: String,
    pub ports: Vec<Port>,
}

/// A <wsdl:port> element within a service.
#[derive(Debug, Clone)]
pub struct Port {
    pub name: String,
    pub binding: QName,
    pub address: String,
}

/// A <wsdl:import> element.
#[derive(Debug, Clone)]
pub struct WsdlImport {
    pub namespace: String,
    pub location: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::qname::QName;

    fn make_minimal_definition() -> WsdlDefinition {
        WsdlDefinition {
            target_namespace: "http://example.com/service".to_string(),
            imports: vec![],
            types: TypesSection::default(),
            messages: HashMap::new(),
            port_types: HashMap::new(),
            bindings: HashMap::new(),
            services: HashMap::new(),
        }
    }

    #[test]
    fn wsdl_definition_can_be_constructed() {
        let def = make_minimal_definition();
        assert_eq!(def.target_namespace, "http://example.com/service");
        assert!(def.messages.is_empty());
    }

    #[test]
    fn message_with_parts() {
        let msg = Message {
            name: "GetStatusRequest".to_string(),
            parts: vec![MessagePart {
                name: "parameters".to_string(),
                element: Some(QName::new("http://example.com", "GetStatus")),
                type_ref: None,
            }],
        };
        assert_eq!(msg.parts.len(), 1);
        assert!(msg.parts[0].element.is_some());
    }

    #[test]
    fn port_type_with_request_response_operation() {
        let pt = PortType {
            name: "DeviceService".to_string(),
            operations: vec![Operation {
                name: "GetStatus".to_string(),
                input: Some(OperationMessage {
                    name: None,
                    message: QName::local("GetStatusRequest"),
                }),
                output: Some(OperationMessage {
                    name: None,
                    message: QName::local("GetStatusResponse"),
                }),
                faults: vec![],
                style: OperationStyle::RequestResponse,
            }],
        };
        assert_eq!(pt.operations.len(), 1);
        assert_eq!(pt.operations[0].style, OperationStyle::RequestResponse);
    }

    #[test]
    fn one_way_operation_has_no_output() {
        let op = Operation {
            name: "Notify".to_string(),
            input: Some(OperationMessage {
                name: None,
                message: QName::local("NotifyRequest"),
            }),
            output: None,
            faults: vec![],
            style: OperationStyle::OneWay,
        };
        assert!(op.output.is_none());
        assert_eq!(op.style, OperationStyle::OneWay);
    }

    #[test]
    fn binding_soap12_document_literal() {
        let binding = Binding {
            name: "DeviceServiceBinding".to_string(),
            port_type: QName::local("DeviceService"),
            soap_binding: SoapBinding {
                style: BindingStyle::Document,
                transport: "http://schemas.xmlsoap.org/soap/http".to_string(),
                soap_version: SoapVersion::Soap12,
            },
            operations: vec![],
        };
        assert_eq!(binding.soap_binding.soap_version, SoapVersion::Soap12);
        assert_eq!(binding.soap_binding.style, BindingStyle::Document);
    }

    #[test]
    fn service_with_port() {
        let svc = Service {
            name: "DeviceService".to_string(),
            ports: vec![Port {
                name: "DeviceServicePort".to_string(),
                binding: QName::local("DeviceServiceBinding"),
                address: "http://192.168.1.1/onvif/device_service".to_string(),
            }],
        };
        assert_eq!(svc.ports.len(), 1);
        assert!(svc.ports[0].address.contains("onvif"));
    }

    #[test]
    fn wsdl_import_with_optional_location() {
        let imp_with_loc = WsdlImport {
            namespace: "http://example.com/types".to_string(),
            location: Some("types.xsd".to_string()),
        };
        let imp_no_loc = WsdlImport {
            namespace: "http://example.com/types".to_string(),
            location: None,
        };
        assert!(imp_with_loc.location.is_some());
        assert!(imp_no_loc.location.is_none());
    }

    #[test]
    fn soap_header_references_message_and_part() {
        let header = SoapHeader {
            message: QName::new("http://docs.oasis-open.org/wss/2004", "Security"),
            part: "token".to_string(),
        };
        assert_eq!(header.part, "token");
        assert!(header.message.namespace.is_some());
    }

    #[test]
    fn operation_fault_holds_message_ref() {
        let fault = OperationFault {
            name: "UnauthorizedFault".to_string(),
            message: QName::local("UnauthorizedFaultMessage"),
        };
        assert_eq!(fault.name, "UnauthorizedFault");
    }
}
