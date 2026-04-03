// WSDL 1.1 Pass 1 parser — DOM traversal via roxmltree
// Produces WsdlDefinition with unresolved QName strings.
// Pass 2 (resolver.rs) wires cross-references.

use std::collections::HashMap;
use thiserror::Error;
use crate::qname::QName;
use crate::wsdl::definitions::{
    Binding, BindingMessage, BindingOperation, BindingStyle, Message, MessagePart,
    Operation, OperationFault, OperationMessage, OperationStyle, Port, PortType,
    Service, SoapBinding, SoapBody, SoapHeader, SoapVersion, TypesSection,
    UseStyle, WsdlDefinition, WsdlImport,
};

// WSDL/SOAP namespaces
const WSDL_NS: &str = "http://schemas.xmlsoap.org/wsdl/";
const SOAP11_BINDING_NS: &str = "http://schemas.xmlsoap.org/wsdl/soap/";
const SOAP12_BINDING_NS: &str = "http://schemas.xmlsoap.org/wsdl/soap12/";
const XSD_NS: &str = "http://www.w3.org/2001/XMLSchema";
const SOAP_HTTP_TRANSPORT: &str = "http://schemas.xmlsoap.org/soap/http";

/// Error type for WSDL parsing failures.
#[derive(Debug, Error)]
pub enum WsdlError {
    #[error("Malformed WSDL XML: {0}")]
    MalformedXml(String),
    #[error("Missing required attribute: {0}")]
    MissingAttribute(String),
    #[error("Unknown namespace prefix: {0}")]
    UnknownNamespace(String),
}

impl From<roxmltree::Error> for WsdlError {
    fn from(e: roxmltree::Error) -> Self {
        WsdlError::MalformedXml(e.to_string())
    }
}

/// Pass 1: parse WSDL bytes into WsdlDefinition with unresolved QName strings.
/// Called with raw WSDL bytes (file contents or embedded bytes).
pub fn parse_wsdl(bytes: &[u8]) -> Result<WsdlDefinition, WsdlError> {
    let text = std::str::from_utf8(bytes)
        .map_err(|e| WsdlError::MalformedXml(format!("UTF-8 error: {e}")))?;
    let doc = roxmltree::Document::parse(text)?;
    let root = doc.root_element();

    // Validate root element
    if root.tag_name().name() != "definitions" {
        return Err(WsdlError::MalformedXml(format!(
            "Expected root element 'definitions', got '{}'",
            root.tag_name().name()
        )));
    }

    let target_namespace = root
        .attribute("targetNamespace")
        .unwrap_or("")
        .to_string();

    let mut imports: Vec<WsdlImport> = Vec::new();
    let mut types = TypesSection::default();
    let mut messages: HashMap<String, Message> = HashMap::new();
    let mut port_types: HashMap<String, PortType> = HashMap::new();
    let mut bindings: HashMap<String, Binding> = HashMap::new();
    let mut services: HashMap<String, Service> = HashMap::new();

    for child in root.children().filter(|n| n.is_element()) {
        let local = child.tag_name().name();
        let ns = child.tag_name().namespace().unwrap_or("");

        if ns != WSDL_NS {
            // Skip non-WSDL elements silently
            continue;
        }

        match local {
            "import" => {
                imports.push(parse_import(child)?);
            }
            "types" => {
                types = parse_types(child)?;
            }
            "message" => {
                let msg = parse_message(child)?;
                messages.insert(msg.name.clone(), msg);
            }
            "portType" => {
                let pt = parse_port_type(child)?;
                port_types.insert(pt.name.clone(), pt);
            }
            "binding" => {
                let b = parse_binding(child)?;
                bindings.insert(b.name.clone(), b);
            }
            "service" => {
                let svc = parse_service(child)?;
                services.insert(svc.name.clone(), svc);
            }
            _ => {
                // Silently skip unknown WSDL elements
            }
        }
    }

    Ok(WsdlDefinition {
        target_namespace,
        imports,
        types,
        messages,
        port_types,
        bindings,
        services,
    })
}

/// Parse a <wsdl:import> element.
fn parse_import(node: roxmltree::Node) -> Result<WsdlImport, WsdlError> {
    let namespace = node
        .attribute("namespace")
        .unwrap_or("")
        .to_string();
    let location = node.attribute("location").map(|s| s.to_string());
    Ok(WsdlImport { namespace, location })
}

/// Parse the <wsdl:types> section, collecting inline xs:schema nodes.
fn parse_types(node: roxmltree::Node) -> Result<TypesSection, WsdlError> {
    let mut schemas: Vec<String> = Vec::new();

    for child in node.children().filter(|n| n.is_element()) {
        let ns = child.tag_name().namespace().unwrap_or("");
        let local = child.tag_name().name();
        if ns == XSD_NS && local == "schema" {
            // Serialize the schema node back to string for the resolver
            let schema_str = node_to_string(child);
            schemas.push(schema_str);
        }
    }

    Ok(TypesSection { schemas })
}

/// Parse a <wsdl:message> element.
fn parse_message(node: roxmltree::Node) -> Result<Message, WsdlError> {
    let name = node
        .attribute("name")
        .ok_or_else(|| WsdlError::MissingAttribute("message/@name".to_string()))?
        .to_string();

    let mut parts: Vec<MessagePart> = Vec::new();
    for child in node.children().filter(|n| n.is_element()) {
        if child.tag_name().name() == "part" {
            parts.push(parse_message_part(child, node)?);
        }
    }

    Ok(Message { name, parts })
}

/// Parse a <wsdl:part> element.
fn parse_message_part(node: roxmltree::Node, parent: roxmltree::Node) -> Result<MessagePart, WsdlError> {
    let name = node
        .attribute("name")
        .ok_or_else(|| WsdlError::MissingAttribute("part/@name".to_string()))?
        .to_string();

    let element = node
        .attribute("element")
        .map(|v| resolve_qname_str(v, parent))
        .transpose()?;

    let type_ref = node
        .attribute("type")
        .map(|v| resolve_qname_str(v, parent))
        .transpose()?;

    Ok(MessagePart { name, element, type_ref })
}

/// Parse a <wsdl:portType> element.
fn parse_port_type(node: roxmltree::Node) -> Result<PortType, WsdlError> {
    let name = node
        .attribute("name")
        .ok_or_else(|| WsdlError::MissingAttribute("portType/@name".to_string()))?
        .to_string();

    let mut operations: Vec<Operation> = Vec::new();
    for child in node.children().filter(|n| n.is_element()) {
        if child.tag_name().name() == "operation" {
            operations.push(parse_operation(child)?);
        }
    }

    Ok(PortType { name, operations })
}

/// Parse a <wsdl:operation> within a portType.
fn parse_operation(node: roxmltree::Node) -> Result<Operation, WsdlError> {
    let name = node
        .attribute("name")
        .ok_or_else(|| WsdlError::MissingAttribute("operation/@name".to_string()))?
        .to_string();

    let mut input: Option<OperationMessage> = None;
    let mut output: Option<OperationMessage> = None;
    let mut faults: Vec<OperationFault> = Vec::new();

    for child in node.children().filter(|n| n.is_element()) {
        match child.tag_name().name() {
            "input" => {
                let msg_attr = child.attribute("message")
                    .ok_or_else(|| WsdlError::MissingAttribute("input/@message".to_string()))?;
                let message = resolve_qname_str(msg_attr, child)?;
                let op_name = child.attribute("name").map(|s| s.to_string());
                input = Some(OperationMessage { name: op_name, message });
            }
            "output" => {
                let msg_attr = child.attribute("message")
                    .ok_or_else(|| WsdlError::MissingAttribute("output/@message".to_string()))?;
                let message = resolve_qname_str(msg_attr, child)?;
                let op_name = child.attribute("name").map(|s| s.to_string());
                output = Some(OperationMessage { name: op_name, message });
            }
            "fault" => {
                let fault_name = child.attribute("name")
                    .ok_or_else(|| WsdlError::MissingAttribute("fault/@name".to_string()))?
                    .to_string();
                let msg_attr = child.attribute("message")
                    .ok_or_else(|| WsdlError::MissingAttribute("fault/@message".to_string()))?;
                let message = resolve_qname_str(msg_attr, child)?;
                faults.push(OperationFault { name: fault_name, message });
            }
            _ => {}
        }
    }

    let style = match (input.is_some(), output.is_some()) {
        (true, true) => OperationStyle::RequestResponse,
        (true, false) => OperationStyle::OneWay,
        (false, true) => OperationStyle::Notification,
        (false, false) => OperationStyle::OneWay,
    };

    Ok(Operation { name, input, output, faults, style })
}

/// Parse a <wsdl:binding> element.
fn parse_binding(node: roxmltree::Node) -> Result<Binding, WsdlError> {
    let name = node
        .attribute("name")
        .ok_or_else(|| WsdlError::MissingAttribute("binding/@name".to_string()))?
        .to_string();

    let port_type_str = node
        .attribute("type")
        .ok_or_else(|| WsdlError::MissingAttribute("binding/@type".to_string()))?;
    let port_type = resolve_qname_str(port_type_str, node)?;

    // Find the soap:binding element to determine SOAP version and style
    let mut soap_binding = SoapBinding {
        style: BindingStyle::Document,
        transport: SOAP_HTTP_TRANSPORT.to_string(),
        soap_version: SoapVersion::Soap11,
    };

    let mut operations: Vec<BindingOperation> = Vec::new();

    for child in node.children().filter(|n| n.is_element()) {
        let child_ns = child.tag_name().namespace().unwrap_or("");
        let child_local = child.tag_name().name();

        match (child_local, child_ns) {
            ("binding", SOAP11_BINDING_NS) => {
                soap_binding.soap_version = SoapVersion::Soap11;
                soap_binding.style = parse_binding_style(child);
                soap_binding.transport = child
                    .attribute("transport")
                    .unwrap_or(SOAP_HTTP_TRANSPORT)
                    .to_string();
            }
            ("binding", SOAP12_BINDING_NS) => {
                soap_binding.soap_version = SoapVersion::Soap12;
                soap_binding.style = parse_binding_style(child);
                soap_binding.transport = child
                    .attribute("transport")
                    .unwrap_or(SOAP_HTTP_TRANSPORT)
                    .to_string();
            }
            ("operation", WSDL_NS) => {
                operations.push(parse_binding_operation(child, soap_binding.soap_version.clone())?);
            }
            _ => {}
        }
    }

    Ok(Binding {
        name,
        port_type,
        soap_binding,
        operations,
    })
}

/// Parse the style attribute from a soap:binding element.
fn parse_binding_style(node: roxmltree::Node) -> BindingStyle {
    match node.attribute("style") {
        Some("rpc") => BindingStyle::Rpc,
        _ => BindingStyle::Document, // default per spec
    }
}

/// Parse a <wsdl:operation> within a binding.
fn parse_binding_operation(
    node: roxmltree::Node,
    soap_version: SoapVersion,
) -> Result<BindingOperation, WsdlError> {
    let name = node
        .attribute("name")
        .ok_or_else(|| WsdlError::MissingAttribute("binding/operation/@name".to_string()))?
        .to_string();

    let mut soap_action = String::new();
    let mut input = BindingMessage {
        body: SoapBody { use_attr: UseStyle::Literal, namespace: None, encoding_style: None },
        headers: Vec::new(),
    };
    let mut output = BindingMessage {
        body: SoapBody { use_attr: UseStyle::Literal, namespace: None, encoding_style: None },
        headers: Vec::new(),
    };

    let soap_op_ns = match soap_version {
        SoapVersion::Soap11 => SOAP11_BINDING_NS,
        SoapVersion::Soap12 => SOAP12_BINDING_NS,
    };

    for child in node.children().filter(|n| n.is_element()) {
        let child_ns = child.tag_name().namespace().unwrap_or("");
        let child_local = child.tag_name().name();

        match (child_local, child_ns) {
            ("operation", ns) if ns == soap_op_ns => {
                soap_action = child
                    .attribute("soapAction")
                    .unwrap_or("")
                    .to_string();
            }
            ("input", WSDL_NS) => {
                input = parse_binding_message(child, soap_op_ns)?;
            }
            ("output", WSDL_NS) => {
                output = parse_binding_message(child, soap_op_ns)?;
            }
            _ => {}
        }
    }

    Ok(BindingOperation { name, soap_action, input, output })
}

/// Parse a binding input/output message element.
fn parse_binding_message(
    node: roxmltree::Node,
    soap_ns: &str,
) -> Result<BindingMessage, WsdlError> {
    let mut body = SoapBody {
        use_attr: UseStyle::Literal,
        namespace: None,
        encoding_style: None,
    };
    let mut headers: Vec<SoapHeader> = Vec::new();

    for child in node.children().filter(|n| n.is_element()) {
        let child_ns = child.tag_name().namespace().unwrap_or("");
        let child_local = child.tag_name().name();

        if child_ns == soap_ns {
            match child_local {
                "body" => {
                    body.use_attr = match child.attribute("use") {
                        Some("encoded") => UseStyle::Encoded,
                        _ => UseStyle::Literal,
                    };
                    body.namespace = child.attribute("namespace").map(|s| s.to_string());
                    body.encoding_style = child.attribute("encodingStyle").map(|s| s.to_string());
                }
                "header" => {
                    if let (Some(msg_str), Some(part)) = (
                        child.attribute("message"),
                        child.attribute("part"),
                    ) {
                        let message = resolve_qname_str(msg_str, child)?;
                        headers.push(SoapHeader {
                            message,
                            part: part.to_string(),
                        });
                    }
                }
                _ => {}
            }
        }
    }

    Ok(BindingMessage { body, headers })
}

/// Parse a <wsdl:service> element.
fn parse_service(node: roxmltree::Node) -> Result<Service, WsdlError> {
    let name = node
        .attribute("name")
        .ok_or_else(|| WsdlError::MissingAttribute("service/@name".to_string()))?
        .to_string();

    let mut ports: Vec<Port> = Vec::new();
    for child in node.children().filter(|n| n.is_element()) {
        if child.tag_name().name() == "port" {
            ports.push(parse_port(child)?);
        }
    }

    Ok(Service { name, ports })
}

/// Parse a <wsdl:port> element.
fn parse_port(node: roxmltree::Node) -> Result<Port, WsdlError> {
    let name = node
        .attribute("name")
        .ok_or_else(|| WsdlError::MissingAttribute("port/@name".to_string()))?
        .to_string();

    let binding_str = node
        .attribute("binding")
        .ok_or_else(|| WsdlError::MissingAttribute("port/@binding".to_string()))?;
    let binding = resolve_qname_str(binding_str, node)?;

    // Find the soap:address child
    let address = node
        .children()
        .filter(|n| n.is_element() && n.tag_name().name() == "address")
        .find_map(|n| n.attribute("location"))
        .unwrap_or("")
        .to_string();

    Ok(Port { name, binding, address })
}

/// Resolve a "prefix:local" QName string using in-scope namespace bindings from `context_node`.
fn resolve_qname_str(qname_str: &str, context_node: roxmltree::Node) -> Result<QName, WsdlError> {
    match qname_str.split_once(':') {
        Some((prefix, local)) => {
            let ns = context_node
                .lookup_namespace_uri(Some(prefix))
                .ok_or_else(|| WsdlError::UnknownNamespace(prefix.to_string()))?;
            Ok(QName::new(ns, local))
        }
        None => {
            // No prefix — check for default namespace
            match context_node.lookup_namespace_uri(None) {
                Some(ns) => Ok(QName::new(ns, qname_str)),
                None => Ok(QName::local(qname_str)),
            }
        }
    }
}

/// Serialize a roxmltree::Node back to an XML string (best-effort, for schema extraction).
fn node_to_string(node: roxmltree::Node) -> String {
    let mut out = String::new();
    serialize_node(node, &mut out);
    out
}

fn serialize_node(node: roxmltree::Node, out: &mut String) {
    if node.is_element() {
        let tag_name = node.tag_name();
        let local = tag_name.name();
        let ns_uri = tag_name.namespace().unwrap_or("");

        // Find the prefix for this element's namespace so we emit a qualified name.
        // This is needed because the serialized fragment must be parseable standalone —
        // the parent document's default namespace (e.g. WSDL namespace) must not bleed in.
        let prefix = find_prefix_for_ns(node, ns_uri);
        let qualified_name = match &prefix {
            Some(p) if !p.is_empty() => format!("{}:{}", p, local),
            _ => local.to_string(),
        };

        out.push('<');
        out.push_str(&qualified_name);

        // Emit namespace declarations (only those declared on THIS node to avoid duplication)
        for ns in node.namespaces() {
            match ns.name() {
                Some(p) => {
                    out.push_str(&format!(" xmlns:{}=\"{}\"", p, ns.uri()));
                }
                None => {
                    out.push_str(&format!(" xmlns=\"{}\"", ns.uri()));
                }
            }
        }

        // Emit attributes
        for attr in node.attributes() {
            out.push(' ');
            out.push_str(attr.name());
            out.push_str("=\"");
            out.push_str(attr.value());
            out.push('"');
        }

        if node.has_children() {
            out.push('>');
            for child in node.children() {
                serialize_node(child, out);
            }
            out.push_str("</");
            out.push_str(&qualified_name);
            out.push('>');
        } else {
            out.push_str("/>");
        }
    } else if node.is_text() {
        if let Some(text) = node.text() {
            out.push_str(text);
        }
    }
}

/// Find the prefix bound to the given namespace URI in scope at `node`.
/// Returns Some(prefix) where prefix may be empty string for default namespace.
fn find_prefix_for_ns(node: roxmltree::Node, ns_uri: &str) -> Option<String> {
    if ns_uri.is_empty() {
        return None;
    }
    // Walk the in-scope namespaces to find one matching this URI
    for ns in node.namespaces() {
        if ns.uri() == ns_uri {
            return Some(ns.name().unwrap_or("").to_string());
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wsdl::definitions::{BindingStyle, SoapVersion, UseStyle};

    // Minimal WSDL with SOAP 1.2 binding, one service, one port, one binding, one portType, one operation
    const MINIMAL_WSDL_SOAP12: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<definitions
  xmlns="http://schemas.xmlsoap.org/wsdl/"
  xmlns:soap12="http://schemas.xmlsoap.org/wsdl/soap12/"
  xmlns:tns="http://example.com/device"
  xmlns:xs="http://www.w3.org/2001/XMLSchema"
  targetNamespace="http://example.com/device"
  name="DeviceService">

  <types>
    <xs:schema targetNamespace="http://example.com/device">
      <xs:element name="GetStatus"/>
      <xs:element name="GetStatusResponse"/>
    </xs:schema>
  </types>

  <message name="GetStatusRequest">
    <part name="parameters" element="tns:GetStatus"/>
  </message>
  <message name="GetStatusResponse">
    <part name="parameters" element="tns:GetStatusResponse"/>
  </message>

  <portType name="DevicePortType">
    <operation name="GetStatus">
      <input message="tns:GetStatusRequest"/>
      <output message="tns:GetStatusResponse"/>
    </operation>
  </portType>

  <binding name="DeviceBinding" type="tns:DevicePortType">
    <soap12:binding style="document" transport="http://schemas.xmlsoap.org/soap/http"/>
    <operation name="GetStatus">
      <soap12:operation soapAction="http://example.com/device/GetStatus"/>
      <input>
        <soap12:body use="literal"/>
      </input>
      <output>
        <soap12:body use="literal"/>
      </output>
    </operation>
  </binding>

  <service name="DeviceService">
    <port name="DevicePort" binding="tns:DeviceBinding">
      <soap12:address location="http://192.168.1.1/onvif/device_service"/>
    </port>
  </service>
</definitions>"#;

    const MINIMAL_WSDL_SOAP11: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<definitions
  xmlns="http://schemas.xmlsoap.org/wsdl/"
  xmlns:soap="http://schemas.xmlsoap.org/wsdl/soap/"
  xmlns:tns="http://example.com/service"
  targetNamespace="http://example.com/service"
  name="TestService">

  <message name="EchoRequest">
    <part name="body" element="tns:Echo"/>
  </message>
  <message name="EchoResponse">
    <part name="body" element="tns:EchoResponse"/>
  </message>

  <portType name="TestPortType">
    <operation name="Echo">
      <input message="tns:EchoRequest"/>
      <output message="tns:EchoResponse"/>
    </operation>
  </portType>

  <binding name="TestBinding" type="tns:TestPortType">
    <soap:binding style="document" transport="http://schemas.xmlsoap.org/soap/http"/>
    <operation name="Echo">
      <soap:operation soapAction="http://example.com/Echo"/>
      <input><soap:body use="literal"/></input>
      <output><soap:body use="literal"/></output>
    </operation>
  </binding>

  <service name="TestService">
    <port name="TestPort" binding="tns:TestBinding">
      <soap:address location="http://localhost/test"/>
    </port>
  </service>
</definitions>"#;

    const WSDL_WITH_IMPORT: &str = r#"<?xml version="1.0"?>
<definitions
  xmlns="http://schemas.xmlsoap.org/wsdl/"
  xmlns:tns="http://example.com/svc"
  targetNamespace="http://example.com/svc">
  <import namespace="http://example.com/types" location="types.xsd"/>
  <message name="Req"><part name="p" element="tns:ReqElem"/></message>
  <portType name="PT"><operation name="Op"><input message="tns:Req"/></operation></portType>
  <binding name="B" type="tns:PT">
    <soap12:binding xmlns:soap12="http://schemas.xmlsoap.org/wsdl/soap12/" style="document" transport="http://schemas.xmlsoap.org/soap/http"/>
    <operation name="Op">
      <soap12:operation xmlns:soap12="http://schemas.xmlsoap.org/wsdl/soap12/" soapAction="http://example.com/Op"/>
      <input><soap12:body xmlns:soap12="http://schemas.xmlsoap.org/wsdl/soap12/" use="literal"/></input>
      <output><soap12:body xmlns:soap12="http://schemas.xmlsoap.org/wsdl/soap12/" use="literal"/></output>
    </operation>
  </binding>
  <service name="Svc">
    <port name="SvcPort" binding="tns:B">
      <soap12:address xmlns:soap12="http://schemas.xmlsoap.org/wsdl/soap12/" location="http://localhost/svc"/>
    </port>
  </service>
</definitions>"#;

    const WSDL_MULTIPLE_OPERATIONS: &str = r#"<?xml version="1.0"?>
<definitions
  xmlns="http://schemas.xmlsoap.org/wsdl/"
  xmlns:soap12="http://schemas.xmlsoap.org/wsdl/soap12/"
  xmlns:tns="http://example.com/multi"
  targetNamespace="http://example.com/multi">
  <message name="OpAReq"><part name="p" element="tns:OpAReqElem"/></message>
  <message name="OpARes"><part name="p" element="tns:OpAResElem"/></message>
  <message name="OpBReq"><part name="p" element="tns:OpBReqElem"/></message>
  <message name="OpBRes"><part name="p" element="tns:OpBResElem"/></message>
  <portType name="MultiPT">
    <operation name="OpA">
      <input message="tns:OpAReq"/>
      <output message="tns:OpARes"/>
    </operation>
    <operation name="OpB">
      <input message="tns:OpBReq"/>
      <output message="tns:OpBRes"/>
    </operation>
  </portType>
  <binding name="MultiB" type="tns:MultiPT">
    <soap12:binding style="document" transport="http://schemas.xmlsoap.org/soap/http"/>
    <operation name="OpA">
      <soap12:operation soapAction="http://example.com/OpA"/>
      <input><soap12:body use="literal"/></input>
      <output><soap12:body use="literal"/></output>
    </operation>
    <operation name="OpB">
      <soap12:operation soapAction="http://example.com/OpB"/>
      <input><soap12:body use="literal"/></input>
      <output><soap12:body use="literal"/></output>
    </operation>
  </binding>
  <service name="MultiSvc">
    <port name="MultiPort" binding="tns:MultiB">
      <soap12:address location="http://localhost/multi"/>
    </port>
  </service>
</definitions>"#;

    // ---- Basic parsing tests ----

    #[test]
    fn parse_minimal_wsdl_produces_definition() {
        let def = parse_wsdl(MINIMAL_WSDL_SOAP12.as_bytes()).unwrap();
        assert_eq!(def.target_namespace, "http://example.com/device");
        assert!(def.services.contains_key("DeviceService"));
        assert!(def.port_types.contains_key("DevicePortType"));
        assert!(def.bindings.contains_key("DeviceBinding"));
        assert!(def.messages.contains_key("GetStatusRequest"));
        assert!(def.messages.contains_key("GetStatusResponse"));
    }

    #[test]
    fn service_port_contains_address() {
        let def = parse_wsdl(MINIMAL_WSDL_SOAP12.as_bytes()).unwrap();
        let svc = def.services.get("DeviceService").unwrap();
        assert_eq!(svc.ports.len(), 1);
        let port = &svc.ports[0];
        assert_eq!(port.name, "DevicePort");
        assert_eq!(port.address, "http://192.168.1.1/onvif/device_service");
    }

    #[test]
    fn port_binding_qname_is_resolved() {
        let def = parse_wsdl(MINIMAL_WSDL_SOAP12.as_bytes()).unwrap();
        let port = &def.services["DeviceService"].ports[0];
        assert_eq!(
            port.binding.namespace.as_deref(),
            Some("http://example.com/device")
        );
        assert_eq!(port.binding.local_name, "DeviceBinding");
    }

    // ---- SOAP version detection tests ----

    #[test]
    fn soap12_binding_detected() {
        let def = parse_wsdl(MINIMAL_WSDL_SOAP12.as_bytes()).unwrap();
        let binding = def.bindings.get("DeviceBinding").unwrap();
        assert_eq!(binding.soap_binding.soap_version, SoapVersion::Soap12);
    }

    #[test]
    fn soap11_binding_detected() {
        let def = parse_wsdl(MINIMAL_WSDL_SOAP11.as_bytes()).unwrap();
        let binding = def.bindings.get("TestBinding").unwrap();
        assert_eq!(binding.soap_binding.soap_version, SoapVersion::Soap11);
    }

    #[test]
    fn binding_style_document() {
        let def = parse_wsdl(MINIMAL_WSDL_SOAP12.as_bytes()).unwrap();
        let binding = def.bindings.get("DeviceBinding").unwrap();
        assert_eq!(binding.soap_binding.style, BindingStyle::Document);
    }

    // ---- PortType operation tests ----

    #[test]
    fn port_type_has_operation_with_input_and_output() {
        let def = parse_wsdl(MINIMAL_WSDL_SOAP12.as_bytes()).unwrap();
        let pt = def.port_types.get("DevicePortType").unwrap();
        assert_eq!(pt.operations.len(), 1);
        let op = &pt.operations[0];
        assert_eq!(op.name, "GetStatus");
        assert!(op.input.is_some());
        assert!(op.output.is_some());
    }

    #[test]
    fn operation_input_message_qname_resolved() {
        let def = parse_wsdl(MINIMAL_WSDL_SOAP12.as_bytes()).unwrap();
        let op = &def.port_types["DevicePortType"].operations[0];
        let input = op.input.as_ref().unwrap();
        assert_eq!(
            input.message.namespace.as_deref(),
            Some("http://example.com/device")
        );
        assert_eq!(input.message.local_name, "GetStatusRequest");
    }

    #[test]
    fn operation_output_message_qname_resolved() {
        let def = parse_wsdl(MINIMAL_WSDL_SOAP12.as_bytes()).unwrap();
        let op = &def.port_types["DevicePortType"].operations[0];
        let output = op.output.as_ref().unwrap();
        assert_eq!(output.message.local_name, "GetStatusResponse");
    }

    // ---- Binding operation tests ----

    #[test]
    fn binding_operation_soap_action_extracted() {
        let def = parse_wsdl(MINIMAL_WSDL_SOAP12.as_bytes()).unwrap();
        let binding = def.bindings.get("DeviceBinding").unwrap();
        assert_eq!(binding.operations.len(), 1);
        assert_eq!(
            binding.operations[0].soap_action,
            "http://example.com/device/GetStatus"
        );
    }

    #[test]
    fn binding_operation_use_literal() {
        let def = parse_wsdl(MINIMAL_WSDL_SOAP12.as_bytes()).unwrap();
        let op = &def.bindings["DeviceBinding"].operations[0];
        assert_eq!(op.input.body.use_attr, UseStyle::Literal);
        assert_eq!(op.output.body.use_attr, UseStyle::Literal);
    }

    // ---- Message part tests ----

    #[test]
    fn message_part_element_qname_resolved() {
        let def = parse_wsdl(MINIMAL_WSDL_SOAP12.as_bytes()).unwrap();
        let msg = def.messages.get("GetStatusRequest").unwrap();
        assert_eq!(msg.parts.len(), 1);
        let part = &msg.parts[0];
        assert_eq!(part.name, "parameters");
        let elem = part.element.as_ref().unwrap();
        assert_eq!(elem.namespace.as_deref(), Some("http://example.com/device"));
        assert_eq!(elem.local_name, "GetStatus");
    }

    // ---- Import tests ----

    #[test]
    fn wsdl_import_with_location_recorded() {
        let def = parse_wsdl(WSDL_WITH_IMPORT.as_bytes()).unwrap();
        assert_eq!(def.imports.len(), 1);
        assert_eq!(def.imports[0].namespace, "http://example.com/types");
        assert_eq!(def.imports[0].location.as_deref(), Some("types.xsd"));
    }

    // ---- Inline schema tests ----

    #[test]
    fn inline_schema_nodes_collected() {
        let def = parse_wsdl(MINIMAL_WSDL_SOAP12.as_bytes()).unwrap();
        assert!(!def.types.schemas.is_empty(), "Expected inline schema to be collected");
        assert!(def.types.schemas[0].contains("xs:schema") || def.types.schemas[0].contains("schema"));
    }

    // ---- Multiple operations test ----

    #[test]
    fn multiple_operations_all_present() {
        let def = parse_wsdl(WSDL_MULTIPLE_OPERATIONS.as_bytes()).unwrap();
        let pt = def.port_types.get("MultiPT").unwrap();
        assert_eq!(pt.operations.len(), 2);
        let names: Vec<&str> = pt.operations.iter().map(|o| o.name.as_str()).collect();
        assert!(names.contains(&"OpA"));
        assert!(names.contains(&"OpB"));
    }

    #[test]
    fn multiple_binding_operations_all_present() {
        let def = parse_wsdl(WSDL_MULTIPLE_OPERATIONS.as_bytes()).unwrap();
        let b = def.bindings.get("MultiB").unwrap();
        assert_eq!(b.operations.len(), 2);
    }

    // ---- Unknown element silently skipped ----

    #[test]
    fn unknown_wsdl_elements_silently_skipped() {
        let wsdl = r#"<?xml version="1.0"?>
<definitions xmlns="http://schemas.xmlsoap.org/wsdl/" xmlns:tns="http://example.com" targetNamespace="http://example.com">
  <documentation>This is a doc</documentation>
  <message name="M1"><part name="p" element="tns:E1"/></message>
</definitions>"#;
        let result = parse_wsdl(wsdl.as_bytes());
        assert!(result.is_ok(), "Should not error on unknown elements: {:?}", result);
        let def = result.unwrap();
        assert!(def.messages.contains_key("M1"));
    }

    // ---- Error cases ----

    #[test]
    fn malformed_xml_returns_error() {
        let result = parse_wsdl(b"<not valid xml");
        assert!(result.is_err());
    }

    #[test]
    fn wrong_root_element_returns_error() {
        let result = parse_wsdl(b"<schema xmlns=\"http://www.w3.org/2001/XMLSchema\"/>");
        assert!(result.is_err());
        match result.unwrap_err() {
            WsdlError::MalformedXml(_) => {}
            e => panic!("Expected MalformedXml, got: {e:?}"),
        }
    }
}
