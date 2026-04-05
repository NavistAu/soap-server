// SOAP dispatch — route body element QName to registered SoapHandler
// Also provides XSD-11 payload validation (validate_request).

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use crate::handler::SoapHandler;
use crate::fault::SoapFault;
use crate::qname::QName;
use crate::xsd::types::{TypeRegistry, ComplexContent};
use crate::wsdl::resolver::ResolvedWsdl;
use crate::wsdl::definitions::BindingStyle;

/// A single routing entry — holds the handler plus metadata needed by the pipeline.
pub struct DispatchEntry {
    pub handler: Arc<dyn SoapHandler>,
    /// Whether this operation requires authentication (controlled by auth_bypass set at startup).
    pub auth_required: bool,
    /// QName of the operation's input type in the TypeRegistry (for XSD-11 validation).
    pub input_type: Option<QName>,
}

impl std::fmt::Debug for DispatchEntry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DispatchEntry")
            .field("auth_required", &self.auth_required)
            .field("input_type", &self.input_type)
            .finish_non_exhaustive()
    }
}

/// The runtime routing table: O(1) by body-element QName, with SOAPAction fallback.
/// Built once at startup from a ResolvedWsdl; never mutated per-request.
pub struct DispatchTable {
    by_element: HashMap<QName, DispatchEntry>,
    by_action: HashMap<String, DispatchEntry>,
    /// Optional catch-all handler for operations not explicitly registered.
    pub default_handler: Option<Arc<dyn SoapHandler>>,
}

impl DispatchTable {
    /// Create an empty dispatch table (no operations, no default handler).
    /// Used internally in multi-service mode as a placeholder.
    pub fn empty() -> Self {
        DispatchTable {
            by_element: HashMap::new(),
            by_action: HashMap::new(),
            default_handler: None,
        }
    }
}

/// Errors that can occur while building the dispatch table (startup-time validation).
#[derive(Debug, thiserror::Error)]
pub enum DispatchError {
    #[error("WSDL operation '{0}' has no registered handler")]
    UnregisteredOperation(String),
    #[error("Registered handler '{0}' has no matching operation in WSDL")]
    UnknownOperation(String),
}

/// Build the dispatch table at startup.
///
/// `handlers` is keyed by operation name (e.g., "GetProfiles").
/// `auth_bypass` is the set of operation names that skip authentication.
/// `default_handler` is an optional catch-all used when no specific handler matches.
///   When `Some`, operations without a registered handler are silently skipped (no error).
///   When `None`, every WSDL operation must have a handler or `UnregisteredOperation` is returned.
///
/// Returns `Err(DispatchError::UnregisteredOperation)` if any WSDL operation has no handler and no default.
/// Returns `Err(DispatchError::UnknownOperation)` if any registered handler has no WSDL operation.
pub fn build_dispatch_table(
    resolved: &ResolvedWsdl,
    handlers: HashMap<String, Arc<dyn SoapHandler>>,
    auth_bypass: &HashSet<String>,
    default_handler: Option<Arc<dyn SoapHandler>>,
) -> Result<DispatchTable, DispatchError> {
    let unique_ops = collect_ops_for_service(None, resolved);
    build_dispatch_table_from_ops(unique_ops, resolved, handlers, auth_bypass, default_handler)
}

/// Build a dispatch table for a single named service.
///
/// Only operations reachable through `service_name`'s ports and bindings are included.
/// Useful when a WSDL defines multiple services and each service mounts on a distinct path.
///
/// Semantics are otherwise identical to `build_dispatch_table`.
pub fn build_dispatch_table_for_service(
    service_name: &str,
    resolved: &ResolvedWsdl,
    handlers: HashMap<String, Arc<dyn SoapHandler>>,
    auth_bypass: &HashSet<String>,
    default_handler: Option<Arc<dyn SoapHandler>>,
) -> Result<DispatchTable, DispatchError> {
    let unique_ops = collect_ops_for_service(Some(service_name), resolved);
    build_dispatch_table_from_ops(unique_ops, resolved, handlers, auth_bypass, default_handler)
}

/// Collect unique operations for all services (service_name = None) or a specific service.
/// Returns Vec of (op_name, soap_action, binding_style, rpc_namespace).
fn collect_ops_for_service(
    service_name: Option<&str>,
    resolved: &ResolvedWsdl,
) -> Vec<(String, String, BindingStyle, Option<String>)> {
    let mut all_binding_ops: Vec<(String, String, BindingStyle, Option<String>)> = Vec::new();

    let service_iter: Vec<&crate::wsdl::definitions::Service> = match service_name {
        Some(name) => resolved.definition.services.get(name).into_iter().collect(),
        None => resolved.definition.services.values().collect(),
    };

    for service in service_iter {
        for port in &service.ports {
            let binding_local = &port.binding.local_name;
            if let Some(binding) = resolved.definition.bindings.get(binding_local) {
                let style = binding.soap_binding.style.clone();
                for binding_op in &binding.operations {
                    let rpc_ns = if style == BindingStyle::Rpc {
                        binding_op.input.body.namespace.clone()
                            .or_else(|| Some(resolved.definition.target_namespace.clone()))
                    } else {
                        None
                    };
                    all_binding_ops.push((
                        binding_op.name.clone(),
                        binding_op.soap_action.clone(),
                        style.clone(),
                        rpc_ns,
                    ));
                }
            }
        }
    }

    // If no services defined (and collecting for all), fall back to all bindings directly.
    if all_binding_ops.is_empty() && service_name.is_none() {
        for (_binding_name, binding) in &resolved.definition.bindings {
            let style = binding.soap_binding.style.clone();
            for binding_op in &binding.operations {
                let rpc_ns = if style == BindingStyle::Rpc {
                    binding_op.input.body.namespace.clone()
                        .or_else(|| Some(resolved.definition.target_namespace.clone()))
                } else {
                    None
                };
                all_binding_ops.push((
                    binding_op.name.clone(),
                    binding_op.soap_action.clone(),
                    style.clone(),
                    rpc_ns,
                ));
            }
        }
    }

    // Deduplicate (same operation may appear in multiple ports/bindings).
    let mut seen_ops: HashSet<String> = HashSet::new();
    let mut unique_ops: Vec<(String, String, BindingStyle, Option<String>)> = Vec::new();
    for (op_name, soap_action, style, rpc_ns) in all_binding_ops {
        if seen_ops.insert(op_name.clone()) {
            unique_ops.push((op_name, soap_action, style, rpc_ns));
        }
    }

    unique_ops
}

/// Internal helper: build a DispatchTable from a pre-collected list of unique operations.
fn build_dispatch_table_from_ops(
    unique_ops: Vec<(String, String, BindingStyle, Option<String>)>,
    resolved: &ResolvedWsdl,
    handlers: HashMap<String, Arc<dyn SoapHandler>>,
    auth_bypass: &HashSet<String>,
    default_handler: Option<Arc<dyn SoapHandler>>,
) -> Result<DispatchTable, DispatchError> {
    let mut by_element: HashMap<QName, DispatchEntry> = HashMap::new();
    let mut by_action: HashMap<String, DispatchEntry> = HashMap::new();

    // Track which handler names are consumed (to detect handlers with no WSDL operation).
    let mut consumed_handlers: HashSet<String> = HashSet::new();

    for (op_name, soap_action, style, rpc_ns) in unique_ops {
        let handler = match handlers.get(&op_name) {
            Some(h) => h.clone(),
            None => match &default_handler {
                Some(d) => d.clone(),
                None => return Err(DispatchError::UnregisteredOperation(op_name.clone())),
            },
        };

        consumed_handlers.insert(op_name.clone());

        let auth_required = !auth_bypass.contains(&op_name);

        // Determine the input dispatch QName.
        // RPC style: synthesize QName from (soap:body namespace or targetNamespace, operation name).
        // Document style: resolve from message part element reference.
        let input_type = if style == BindingStyle::Rpc {
            let ns = rpc_ns.as_deref().unwrap_or(&resolved.definition.target_namespace);
            Some(QName::new(ns, &op_name))
        } else {
            resolve_input_element(resolved, &op_name)
        };

        let entry_for_element = DispatchEntry {
            handler: handler.clone(),
            auth_required,
            input_type: input_type.clone(),
        };
        let entry_for_action = DispatchEntry {
            handler,
            auth_required,
            input_type,
        };

        // Index by input element QName.
        if let Some(ref qn) = entry_for_element.input_type {
            by_element.insert(qn.clone(), entry_for_element);
        } else {
            // No element reference — still insert by soap action so we can route by SOAPAction.
            drop(entry_for_element);
        }

        // Index by SOAPAction (non-empty actions only).
        if !soap_action.is_empty() {
            by_action.insert(soap_action, entry_for_action);
        }
    }

    // Verify no extra handlers were registered that have no WSDL operation.
    for handler_name in handlers.keys() {
        if !consumed_handlers.contains(handler_name) {
            return Err(DispatchError::UnknownOperation(handler_name.clone()));
        }
    }

    Ok(DispatchTable { by_element, by_action, default_handler })
}

/// Resolve the input element QName for a named operation.
/// Searches through port_types for an operation by name, then resolves its input message's
/// first part element reference.
fn resolve_input_element(resolved: &ResolvedWsdl, op_name: &str) -> Option<QName> {
    // Search all port_types for this operation.
    for (_pt_name, port_type) in &resolved.definition.port_types {
        for op in &port_type.operations {
            if op.name != op_name {
                continue;
            }
            let input_msg_ref = op.input.as_ref()?;
            // Resolve the message QName (local_name is the key in the messages map).
            let msg_local = &input_msg_ref.message.local_name;
            let message = resolved.definition.messages.get(msg_local)?;
            // Take the element attribute of the first part (document/literal style).
            let first_part = message.parts.first()?;
            return first_part.element.clone();
        }
    }
    None
}

/// Route an incoming request to its handler.
///
/// Tries `by_element` lookup first (document/literal dispatch by body first-child QName).
/// Falls back to `by_action` if `soap_action` is provided and body-QName lookup fails.
/// Returns `Err(SoapFault)` with `FaultCode::Sender` if no match is found.
pub fn route<'a>(
    table: &'a DispatchTable,
    body_first_child_qname: &QName,
    soap_action: Option<&str>,
) -> Result<&'a DispatchEntry, SoapFault> {
    // Primary: by element QName.
    if let Some(entry) = table.by_element.get(body_first_child_qname) {
        return Ok(entry);
    }

    // Fallback: by SOAPAction header.
    if let Some(action) = soap_action {
        if let Some(entry) = table.by_action.get(action) {
            return Ok(entry);
        }
    }

    Err(SoapFault::action_not_supported(&body_first_child_qname.to_string()))
    // Note: default_handler is not reachable via route() — it is used at build_dispatch_table()
    // time to fill entries for unregistered operations, so by dispatch time all entries exist.
}

/// XSD-11: Structural validation of the request body against the operation's input type.
///
/// Parses `body_bytes` as XML and checks that all elements with `min_occurs > 0` in the
/// resolved ComplexType are present as children of the root element.
///
/// Returns `Ok(())` if:
/// - `input_type` is `None` (no type information — skip validation)
/// - `input_type` is `Some(qname)` but `qname` is not in the registry (unknown type — skip)
/// - All required elements are present
///
/// Returns `Err(SoapFault::sender(...))` if a required child element is missing.
pub fn validate_request(
    body_bytes: &[u8],
    type_registry: &TypeRegistry,
    input_type: Option<&QName>,
) -> Result<(), SoapFault> {
    let Some(qname) = input_type else {
        return Ok(());
    };

    let Some(xsd_type) = type_registry.lookup(qname) else {
        return Ok(()); // Unknown type — skip validation (not an error)
    };

    // Extract required element names from the ComplexType.
    let required_names = collect_required_element_names(xsd_type);
    if required_names.is_empty() {
        return Ok(());
    }

    // Parse body_bytes and collect the local names of direct children of the root element.
    let present_children = parse_child_element_names(body_bytes)
        .map_err(|e| SoapFault::sender(format!("Schema validation failed: malformed XML: {e}")))?;

    for required in &required_names {
        if !present_children.contains(required) {
            return Err(SoapFault::sender(format!(
                "Schema validation failed: required element '{}' is missing",
                required
            )));
        }
    }

    Ok(())
}

/// Collect local names of elements with min_occurs > 0 from the top-level content model.
fn collect_required_element_names(xsd_type: &crate::xsd::types::XsdType) -> Vec<String> {
    use crate::xsd::types::XsdType;
    match xsd_type {
        XsdType::Complex(ct) => collect_required_from_content(&ct.content),
        XsdType::Simple(_) => vec![],
    }
}

fn collect_required_from_content(content: &ComplexContent) -> Vec<String> {
    let elements = match content {
        ComplexContent::Sequence(els) | ComplexContent::All(els) | ComplexContent::Choice(els) => els,
        ComplexContent::ComplexExtension { content, .. } => return collect_required_from_content(content),
        ComplexContent::ComplexRestriction { content, .. } => return collect_required_from_content(content),
        ComplexContent::Empty | ComplexContent::SimpleContent(_) => return vec![],
    };

    elements.iter()
        .filter_map(|el| {
            if el.min_occurs > 0 {
                el.name.clone()
            } else {
                None
            }
        })
        .collect()
}

/// Use quick-xml to parse body_bytes and return the local names of direct children
/// of the root element (one level deep).
fn parse_child_element_names(body_bytes: &[u8]) -> Result<HashSet<String>, String> {
    use quick_xml::Reader;
    use quick_xml::events::Event;

    let mut reader = Reader::from_reader(body_bytes);
    reader.config_mut().trim_text(true);

    let mut depth: u32 = 0;
    let mut children: HashSet<String> = HashSet::new();

    loop {
        match reader.read_event() {
            Ok(Event::Start(e)) => {
                depth += 1;
                if depth == 2 {
                    // Direct child of root element
                    let local_name = e.local_name();
                    let name = std::str::from_utf8(local_name.as_ref())
                        .map_err(|e| e.to_string())?
                        .to_string();
                    children.insert(name);
                }
            }
            Ok(Event::Empty(e)) => {
                if depth == 1 {
                    // Self-closing direct child of root
                    let local_name = e.local_name();
                    let name = std::str::from_utf8(local_name.as_ref())
                        .map_err(|e| e.to_string())?
                        .to_string();
                    children.insert(name);
                }
            }
            Ok(Event::End(_)) => {
                if depth == 0 {
                    break;
                }
                depth -= 1;
            }
            Ok(Event::Eof) => break,
            Err(e) => return Err(e.to_string()),
            _ => {}
        }
    }

    Ok(children)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use bytes::Bytes;
    use crate::fault::{FaultCode, SoapFault};
    use crate::handler::SoapHandler;
    use crate::qname::QName;
    use crate::wsdl::resolver::ResolvedWsdl;
    use crate::wsdl::definitions::*;
    use crate::xsd::types::{TypeRegistry, XsdType, ComplexType, ComplexContent};
    use crate::xsd::elements::XsdElement;
    use async_trait::async_trait;

    // ── Test handler ──────────────────────────────────────────────────────────

    struct MockHandler {
        name: &'static str,
    }

    #[async_trait]
    impl SoapHandler for MockHandler {
        async fn handle(&self, _body: Bytes) -> Result<Bytes, SoapFault> {
            Ok(Bytes::from(format!("<response>{}</response>", self.name)))
        }
    }

    fn mock_handler(name: &'static str) -> Arc<dyn SoapHandler> {
        Arc::new(MockHandler { name })
    }

    // ── Minimal ResolvedWsdl builder ──────────────────────────────────────────

    /// Build a minimal ResolvedWsdl with one service, one port, one binding, one operation.
    fn make_resolved_wsdl(
        op_name: &str,
        soap_action: &str,
        input_element_qname: Option<QName>,
    ) -> ResolvedWsdl {
        let target_ns = "http://example.com/service".to_string();
        let msg_name = format!("{}Request", op_name);

        // Message: one part with the element reference
        let part = MessagePart {
            name: "parameters".to_string(),
            element: input_element_qname,
            type_ref: None,
        };
        let message = Message {
            name: msg_name.clone(),
            parts: vec![part],
        };

        // PortType operation
        let pt_op = Operation {
            name: op_name.to_string(),
            input: Some(OperationMessage {
                name: None,
                message: QName::local(&msg_name),
            }),
            output: None,
            faults: vec![],
            style: OperationStyle::RequestResponse,
        };
        let port_type = PortType {
            name: "TestPortType".to_string(),
            operations: vec![pt_op],
        };

        // Binding operation
        let binding_op = BindingOperation {
            name: op_name.to_string(),
            soap_action: soap_action.to_string(),
            input: BindingMessage {
                body: SoapBody {
                    use_attr: UseStyle::Literal,
                    namespace: None,
                    encoding_style: None,
                },
                headers: vec![],
            },
            output: BindingMessage {
                body: SoapBody {
                    use_attr: UseStyle::Literal,
                    namespace: None,
                    encoding_style: None,
                },
                headers: vec![],
            },
        };
        let binding = Binding {
            name: "TestBinding".to_string(),
            port_type: QName::local("TestPortType"),
            soap_binding: SoapBinding {
                style: BindingStyle::Document,
                transport: "http://schemas.xmlsoap.org/soap/http".to_string(),
                soap_version: SoapVersion::Soap12,
            },
            operations: vec![binding_op],
        };

        // Service + Port
        let port = Port {
            name: "TestPort".to_string(),
            binding: QName::local("TestBinding"),
            address: "http://localhost/service".to_string(),
        };
        let service = Service {
            name: "TestService".to_string(),
            ports: vec![port],
        };

        let mut messages = HashMap::new();
        messages.insert(msg_name, message);
        let mut port_types = HashMap::new();
        port_types.insert("TestPortType".to_string(), port_type);
        let mut bindings = HashMap::new();
        bindings.insert("TestBinding".to_string(), binding);
        let mut services = HashMap::new();
        services.insert("TestService".to_string(), service);

        let definition = WsdlDefinition {
            target_namespace: target_ns,
            imports: vec![],
            types: TypesSection::default(),
            messages,
            port_types,
            bindings,
            services,
        };

        ResolvedWsdl {
            definition,
            type_registry: TypeRegistry::new(),
            raw_bytes: vec![],
        }
    }

    // ── build_dispatch_table tests ────────────────────────────────────────────

    #[test]
    fn dispatch_table_routes_by_element_qname() {
        let elem_qname = QName::new("http://example.com", "GetProfiles");
        let resolved = make_resolved_wsdl("GetProfiles", "urn:GetProfiles", Some(elem_qname.clone()));

        let mut handlers = HashMap::new();
        handlers.insert("GetProfiles".to_string(), mock_handler("GetProfiles"));

        let table = build_dispatch_table(&resolved, handlers, &HashSet::new(), None).unwrap();
        let entry = route(&table, &elem_qname, None).unwrap();

        // Handler is present (we can verify by running it synchronously via block_on equivalent)
        assert!(entry.auth_required); // default: auth required
    }

    #[test]
    fn dispatch_table_unregistered_operation_fails_at_build() {
        let elem_qname = QName::new("http://example.com", "GetProfiles");
        let resolved = make_resolved_wsdl("GetProfiles", "urn:GetProfiles", Some(elem_qname));

        // No handlers provided at all — should fail at startup
        let handlers: HashMap<String, Arc<dyn SoapHandler>> = HashMap::new();
        let result = build_dispatch_table(&resolved, handlers, &HashSet::new(), None);

        assert!(matches!(result, Err(DispatchError::UnregisteredOperation(ref name)) if name == "GetProfiles"));
    }

    #[test]
    fn dispatch_table_unknown_handler_name_fails_at_build() {
        let elem_qname = QName::new("http://example.com", "GetProfiles");
        let resolved = make_resolved_wsdl("GetProfiles", "urn:GetProfiles", Some(elem_qname));

        let mut handlers = HashMap::new();
        handlers.insert("GetProfiles".to_string(), mock_handler("GetProfiles"));
        handlers.insert("NonExistentOp".to_string(), mock_handler("ghost")); // no WSDL op

        let result = build_dispatch_table(&resolved, handlers, &HashSet::new(), None);
        assert!(matches!(result, Err(DispatchError::UnknownOperation(ref name)) if name == "NonExistentOp"));
    }

    #[test]
    fn route_unknown_qname_no_soap_action_returns_sender_fault() {
        let elem_qname = QName::new("http://example.com", "GetProfiles");
        let resolved = make_resolved_wsdl("GetProfiles", "urn:GetProfiles", Some(elem_qname.clone()));

        let mut handlers = HashMap::new();
        handlers.insert("GetProfiles".to_string(), mock_handler("GetProfiles"));
        let table = build_dispatch_table(&resolved, handlers, &HashSet::new(), None).unwrap();

        let unknown = QName::new("http://example.com", "UnknownOperation");
        let result = route(&table, &unknown, None);

        assert!(result.is_err());
        let fault = result.unwrap_err();
        assert_eq!(fault.code, FaultCode::Sender);
        assert!(fault.reason.to_lowercase().contains("action") || fault.reason.to_lowercase().contains("supported"));
    }

    #[test]
    fn route_falls_back_to_soap_action_on_unknown_qname() {
        let elem_qname = QName::new("http://example.com", "GetProfiles");
        let resolved = make_resolved_wsdl("GetProfiles", "urn:GetProfiles", Some(elem_qname.clone()));

        let mut handlers = HashMap::new();
        handlers.insert("GetProfiles".to_string(), mock_handler("GetProfiles"));
        let table = build_dispatch_table(&resolved, handlers, &HashSet::new(), None).unwrap();

        // Unknown body QName but known SOAPAction
        let unknown = QName::local("SomethingElse");
        let result = route(&table, &unknown, Some("urn:GetProfiles"));
        assert!(result.is_ok(), "Expected fallback to SOAPAction to succeed");
    }

    #[test]
    fn route_unknown_qname_unknown_soap_action_returns_fault() {
        let elem_qname = QName::new("http://example.com", "GetProfiles");
        let resolved = make_resolved_wsdl("GetProfiles", "urn:GetProfiles", Some(elem_qname.clone()));

        let mut handlers = HashMap::new();
        handlers.insert("GetProfiles".to_string(), mock_handler("GetProfiles"));
        let table = build_dispatch_table(&resolved, handlers, &HashSet::new(), None).unwrap();

        let unknown = QName::local("SomethingElse");
        let result = route(&table, &unknown, Some("urn:UnknownAction"));
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().code, FaultCode::Sender);
    }

    #[test]
    fn auth_bypass_marks_entry_auth_not_required() {
        let elem_qname = QName::new("http://example.com", "GetProfiles");
        let resolved = make_resolved_wsdl("GetProfiles", "urn:GetProfiles", Some(elem_qname.clone()));

        let mut handlers = HashMap::new();
        handlers.insert("GetProfiles".to_string(), mock_handler("GetProfiles"));

        let mut bypass = HashSet::new();
        bypass.insert("GetProfiles".to_string());

        let table = build_dispatch_table(&resolved, handlers, &bypass, None).unwrap();
        let entry = route(&table, &elem_qname, None).unwrap();
        assert!(!entry.auth_required); // bypassed — no auth required
    }

    #[test]
    fn auth_required_by_default_for_non_bypass_op() {
        let elem_qname = QName::new("http://example.com", "SetSomething");
        let resolved = make_resolved_wsdl("SetSomething", "urn:SetSomething", Some(elem_qname.clone()));

        let mut handlers = HashMap::new();
        handlers.insert("SetSomething".to_string(), mock_handler("SetSomething"));

        let table = build_dispatch_table(&resolved, handlers, &HashSet::new(), None).unwrap();
        let entry = route(&table, &elem_qname, None).unwrap();
        assert!(entry.auth_required);
    }

    // ── validate_request tests ────────────────────────────────────────────────

    fn make_type_registry_with_required_field(type_qname: &QName, required_field: &str) -> TypeRegistry {
        let element = XsdElement {
            name: Some(required_field.to_string()),
            min_occurs: 1,
            ..Default::default()
        };
        let ct = ComplexType {
            name: Some(type_qname.local_name.clone()),
            content: ComplexContent::Sequence(vec![element]),
            attributes: vec![],
        };
        let mut reg = TypeRegistry::new();
        reg.insert(type_qname.clone(), XsdType::Complex(ct));
        reg
    }

    #[test]
    fn validate_request_no_input_type_returns_ok() {
        let reg = TypeRegistry::new();
        let result = validate_request(b"<GetProfiles/>", &reg, None);
        assert!(result.is_ok());
    }

    #[test]
    fn validate_request_unknown_type_in_registry_returns_ok() {
        let reg = TypeRegistry::new(); // empty — unknown type
        let qname = QName::new("http://example.com", "GetProfilesRequest");
        let result = validate_request(b"<GetProfiles/>", &reg, Some(&qname));
        assert!(result.is_ok());
    }

    #[test]
    fn validate_request_valid_xml_with_required_element_returns_ok() {
        let qname = QName::new("http://example.com", "GetProfilesRequest");
        let reg = make_type_registry_with_required_field(&qname, "Token");

        let body = b"<GetProfiles xmlns=\"http://example.com\"><Token>1234</Token></GetProfiles>";
        let result = validate_request(body, &reg, Some(&qname));
        assert!(result.is_ok(), "Expected Ok, got: {:?}", result);
    }

    #[test]
    fn validate_request_missing_required_element_returns_sender_fault() {
        let qname = QName::new("http://example.com", "GetProfilesRequest");
        let reg = make_type_registry_with_required_field(&qname, "Token");

        // Body has no <Token> child
        let body = b"<GetProfiles xmlns=\"http://example.com\"></GetProfiles>";
        let result = validate_request(body, &reg, Some(&qname));
        assert!(result.is_err());
        let fault = result.unwrap_err();
        assert_eq!(fault.code, FaultCode::Sender);
        assert!(fault.reason.contains("Schema validation failed"));
        assert!(fault.reason.contains("Token"));
    }

    #[test]
    fn validate_request_optional_element_missing_returns_ok() {
        let qname = QName::new("http://example.com", "GetProfilesRequest");
        let optional_el = XsdElement {
            name: Some("OptionalField".to_string()),
            min_occurs: 0,
            ..Default::default()
        };
        let ct = ComplexType {
            name: Some("GetProfilesRequest".to_string()),
            content: ComplexContent::Sequence(vec![optional_el]),
            attributes: vec![],
        };
        let mut reg = TypeRegistry::new();
        reg.insert(qname.clone(), XsdType::Complex(ct));

        let body = b"<GetProfiles xmlns=\"http://example.com\"></GetProfiles>";
        let result = validate_request(body, &reg, Some(&qname));
        assert!(result.is_ok());
    }

    #[test]
    fn handler_name_matches_wsdl_operation_name_get_profiles() {
        // Regression: "GetProfiles" key in handlers must match "GetProfiles" operation in WSDL
        let elem_qname = QName::new("http://onvif.org/ver10/media/wsdl", "GetProfiles");
        let resolved = make_resolved_wsdl("GetProfiles", "http://www.onvif.org/ver10/media/wsdl/GetProfiles", Some(elem_qname.clone()));

        let mut handlers = HashMap::new();
        handlers.insert("GetProfiles".to_string(), mock_handler("GetProfiles"));

        let table = build_dispatch_table(&resolved, handlers, &HashSet::new(), None).unwrap();
        let entry = route(&table, &elem_qname, None).unwrap();
        assert!(entry.auth_required);
    }

    // ── RPC binding tests ─────────────────────────────────────────────────────

    /// Build a minimal ResolvedWsdl with RPC-style binding.
    fn make_resolved_wsdl_rpc(
        op_name: &str,
        soap_action: &str,
        rpc_namespace: Option<&str>,
    ) -> ResolvedWsdl {
        let target_ns = "http://example.com/service".to_string();
        let msg_name = format!("{}Request", op_name);

        // Message: one part with type_ref (RPC style — no element ref)
        let part = MessagePart {
            name: "parameters".to_string(),
            element: None,
            type_ref: Some(QName::new("http://www.w3.org/2001/XMLSchema", "string")),
        };
        let message = Message {
            name: msg_name.clone(),
            parts: vec![part],
        };

        // PortType operation
        let pt_op = Operation {
            name: op_name.to_string(),
            input: Some(OperationMessage {
                name: None,
                message: QName::local(&msg_name),
            }),
            output: None,
            faults: vec![],
            style: OperationStyle::RequestResponse,
        };
        let port_type = PortType {
            name: "TestPortType".to_string(),
            operations: vec![pt_op],
        };

        // Binding operation with RPC-specific namespace
        let binding_op = BindingOperation {
            name: op_name.to_string(),
            soap_action: soap_action.to_string(),
            input: BindingMessage {
                body: SoapBody {
                    use_attr: UseStyle::Encoded,
                    namespace: rpc_namespace.map(|s| s.to_string()),
                    encoding_style: None,
                },
                headers: vec![],
            },
            output: BindingMessage {
                body: SoapBody {
                    use_attr: UseStyle::Encoded,
                    namespace: rpc_namespace.map(|s| s.to_string()),
                    encoding_style: None,
                },
                headers: vec![],
            },
        };
        let binding = Binding {
            name: "TestBinding".to_string(),
            port_type: QName::local("TestPortType"),
            soap_binding: SoapBinding {
                style: BindingStyle::Rpc,
                transport: "http://schemas.xmlsoap.org/soap/http".to_string(),
                soap_version: SoapVersion::Soap12,
            },
            operations: vec![binding_op],
        };

        // Service + Port
        let port = Port {
            name: "TestPort".to_string(),
            binding: QName::local("TestBinding"),
            address: "http://localhost/service".to_string(),
        };
        let service = Service {
            name: "TestService".to_string(),
            ports: vec![port],
        };

        let mut messages = HashMap::new();
        messages.insert(msg_name, message);
        let mut port_types = HashMap::new();
        port_types.insert("TestPortType".to_string(), port_type);
        let mut bindings = HashMap::new();
        bindings.insert("TestBinding".to_string(), binding);
        let mut services = HashMap::new();
        services.insert("TestService".to_string(), service);

        let definition = WsdlDefinition {
            target_namespace: target_ns,
            imports: vec![],
            types: TypesSection::default(),
            messages,
            port_types,
            bindings,
            services,
        };

        ResolvedWsdl {
            definition,
            type_registry: crate::xsd::types::TypeRegistry::new(),
            raw_bytes: vec![],
        }
    }

    /// Build a ResolvedWsdl with two services (ServiceA and ServiceB), each with their own
    /// binding and operations.
    fn make_resolved_wsdl_two_services() -> ResolvedWsdl {
        let target_ns = "http://example.com/multi".to_string();

        // Messages
        let msg_a = Message {
            name: "OpARequest".to_string(),
            parts: vec![MessagePart {
                name: "parameters".to_string(),
                element: Some(QName::new("http://example.com/multi", "OpA")),
                type_ref: None,
            }],
        };
        let msg_b = Message {
            name: "OpBRequest".to_string(),
            parts: vec![MessagePart {
                name: "parameters".to_string(),
                element: Some(QName::new("http://example.com/multi", "OpB")),
                type_ref: None,
            }],
        };

        // PortTypes
        let pt_a = PortType {
            name: "PortTypeA".to_string(),
            operations: vec![Operation {
                name: "OpA".to_string(),
                input: Some(OperationMessage {
                    name: None,
                    message: QName::local("OpARequest"),
                }),
                output: None,
                faults: vec![],
                style: OperationStyle::RequestResponse,
            }],
        };
        let pt_b = PortType {
            name: "PortTypeB".to_string(),
            operations: vec![Operation {
                name: "OpB".to_string(),
                input: Some(OperationMessage {
                    name: None,
                    message: QName::local("OpBRequest"),
                }),
                output: None,
                faults: vec![],
                style: OperationStyle::RequestResponse,
            }],
        };

        // Bindings
        let binding_a = Binding {
            name: "BindingA".to_string(),
            port_type: QName::local("PortTypeA"),
            soap_binding: SoapBinding {
                style: BindingStyle::Document,
                transport: "http://schemas.xmlsoap.org/soap/http".to_string(),
                soap_version: SoapVersion::Soap12,
            },
            operations: vec![BindingOperation {
                name: "OpA".to_string(),
                soap_action: "urn:OpA".to_string(),
                input: BindingMessage {
                    body: SoapBody {
                        use_attr: UseStyle::Literal,
                        namespace: None,
                        encoding_style: None,
                    },
                    headers: vec![],
                },
                output: BindingMessage {
                    body: SoapBody {
                        use_attr: UseStyle::Literal,
                        namespace: None,
                        encoding_style: None,
                    },
                    headers: vec![],
                },
            }],
        };
        let binding_b = Binding {
            name: "BindingB".to_string(),
            port_type: QName::local("PortTypeB"),
            soap_binding: SoapBinding {
                style: BindingStyle::Document,
                transport: "http://schemas.xmlsoap.org/soap/http".to_string(),
                soap_version: SoapVersion::Soap12,
            },
            operations: vec![BindingOperation {
                name: "OpB".to_string(),
                soap_action: "urn:OpB".to_string(),
                input: BindingMessage {
                    body: SoapBody {
                        use_attr: UseStyle::Literal,
                        namespace: None,
                        encoding_style: None,
                    },
                    headers: vec![],
                },
                output: BindingMessage {
                    body: SoapBody {
                        use_attr: UseStyle::Literal,
                        namespace: None,
                        encoding_style: None,
                    },
                    headers: vec![],
                },
            }],
        };

        // Services
        let service_a = Service {
            name: "ServiceA".to_string(),
            ports: vec![Port {
                name: "PortA".to_string(),
                binding: QName::local("BindingA"),
                address: "http://localhost/soap/a".to_string(),
            }],
        };
        let service_b = Service {
            name: "ServiceB".to_string(),
            ports: vec![Port {
                name: "PortB".to_string(),
                binding: QName::local("BindingB"),
                address: "http://localhost/soap/b".to_string(),
            }],
        };

        let mut messages = HashMap::new();
        messages.insert("OpARequest".to_string(), msg_a);
        messages.insert("OpBRequest".to_string(), msg_b);
        let mut port_types = HashMap::new();
        port_types.insert("PortTypeA".to_string(), pt_a);
        port_types.insert("PortTypeB".to_string(), pt_b);
        let mut bindings = HashMap::new();
        bindings.insert("BindingA".to_string(), binding_a);
        bindings.insert("BindingB".to_string(), binding_b);
        let mut services = HashMap::new();
        services.insert("ServiceA".to_string(), service_a);
        services.insert("ServiceB".to_string(), service_b);

        let definition = WsdlDefinition {
            target_namespace: target_ns,
            imports: vec![],
            types: TypesSection::default(),
            messages,
            port_types,
            bindings,
            services,
        };

        ResolvedWsdl {
            definition,
            type_registry: crate::xsd::types::TypeRegistry::new(),
            raw_bytes: vec![],
        }
    }

    #[test]
    fn build_dispatch_table_rpc_binding() {
        let resolved = make_resolved_wsdl_rpc("GetOp", "urn:GetOp", Some("http://example.com/svc"));

        let mut handlers = HashMap::new();
        handlers.insert("GetOp".to_string(), mock_handler("GetOp"));

        let table = build_dispatch_table(&resolved, handlers, &HashSet::new(), None).unwrap();

        // RPC dispatch key: (soap:body namespace, operation name)
        let rpc_qname = QName::new("http://example.com/svc", "GetOp");
        let result = route(&table, &rpc_qname, None);
        assert!(result.is_ok(), "Expected RPC QName to route successfully, got: {:?}", result);
    }

    #[test]
    fn rpc_dispatch_by_wrapper_element() {
        let resolved = make_resolved_wsdl_rpc("GetOp", "urn:GetOp", Some("http://example.com/svc"));

        let mut handlers = HashMap::new();
        handlers.insert("GetOp".to_string(), mock_handler("GetOp"));

        let table = build_dispatch_table(&resolved, handlers, &HashSet::new(), None).unwrap();

        // Confirm the entry is in by_element keyed on the synthesized QName
        let rpc_qname = QName::new("http://example.com/svc", "GetOp");
        let result = route(&table, &rpc_qname, None);
        assert!(result.is_ok(), "route by synthesized RPC QName should return Ok");
    }

    #[test]
    fn build_dispatch_table_rpc_missing_namespace_falls_back_to_target_ns() {
        // When soap:body.namespace is None for an RPC binding, fall back to WSDL targetNamespace
        let resolved = make_resolved_wsdl_rpc("GetOp", "urn:GetOp", None);
        let target_ns = resolved.definition.target_namespace.clone();

        let mut handlers = HashMap::new();
        handlers.insert("GetOp".to_string(), mock_handler("GetOp"));

        let table = build_dispatch_table(&resolved, handlers, &HashSet::new(), None).unwrap();

        // Should be keyed by (targetNamespace, opName)
        let fallback_qname = QName::new(&target_ns, "GetOp");
        let result = route(&table, &fallback_qname, None);
        assert!(result.is_ok(), "RPC without namespace should fall back to targetNamespace, got: {:?}", result);
    }

    #[test]
    fn build_dispatch_table_for_service_isolates_operations() {
        let resolved = make_resolved_wsdl_two_services();

        let op_a_qname = QName::new("http://example.com/multi", "OpA");
        let op_b_qname = QName::new("http://example.com/multi", "OpB");

        let mut handlers_a = HashMap::new();
        handlers_a.insert("OpA".to_string(), mock_handler("OpA"));

        let mut handlers_b = HashMap::new();
        handlers_b.insert("OpB".to_string(), mock_handler("OpB"));

        let table_a = build_dispatch_table_for_service("ServiceA", &resolved, handlers_a, &HashSet::new(), None).unwrap();
        let table_b = build_dispatch_table_for_service("ServiceB", &resolved, handlers_b, &HashSet::new(), None).unwrap();

        // ServiceA table contains OpA
        assert!(route(&table_a, &op_a_qname, None).is_ok(), "ServiceA should route OpA");
        // ServiceB table contains OpB
        assert!(route(&table_b, &op_b_qname, None).is_ok(), "ServiceB should route OpB");
        // ServiceA table does NOT contain OpB
        assert!(route(&table_a, &op_b_qname, None).is_err(), "ServiceA should NOT route OpB");
    }
}
