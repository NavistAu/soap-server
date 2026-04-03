// XSD Pass 1 parser — DOM traversal via roxmltree producing RawSchema
use std::collections::HashMap;
use roxmltree::Node;
use thiserror::Error;
use crate::qname::QName;
use crate::xsd::types::*;
use crate::xsd::elements::*;

/// XS namespace URI constant.
pub const XS_NS: &str = "http://www.w3.org/2001/XMLSchema";

/// Errors produced by XSD parsing.
#[derive(Debug, Error)]
pub enum SchemaError {
    #[error("Malformed XML: {0}")]
    MalformedXml(String),
    #[error("Unknown reference: {0}")]
    UnknownRef(String),
    #[error("Cycle detected: {0}")]
    CycleDetected(String),
}

/// An xs:import declaration.
#[derive(Debug, Clone)]
pub struct SchemaImport {
    pub namespace: Option<String>,
    pub schema_location: Option<String>,
}

/// An xs:include declaration.
#[derive(Debug, Clone)]
pub struct SchemaInclude {
    pub schema_location: String,
}

/// Pass 1 result: all types and elements collected from one XSD schema node.
/// QName references are stored as URIs resolved from the node's namespace context.
/// Resolution of forward-references between schemas happens in pass 2.
pub struct RawSchema {
    pub target_namespace: Option<String>,
    pub types: HashMap<QName, XsdType>,
    pub elements: HashMap<QName, XsdElement>,
    pub attribute_groups: HashMap<QName, AttributeGroup>,
    pub groups: HashMap<QName, Group>,
    pub imports: Vec<SchemaImport>,
    pub includes: Vec<SchemaInclude>,
}

/// Resolve a QName string (e.g. "tns:Foo" or "xs:string") using the namespace
/// context of `node`. Returns a `QName` with namespace URI if the prefix resolves.
fn resolve_qname(value: &str, node: Node<'_, '_>) -> QName {
    if let Some((prefix, local)) = value.split_once(':') {
        let ns_uri = node.lookup_namespace_uri(Some(prefix));
        match ns_uri {
            Some(uri) => QName::new(uri, local),
            None => QName {
                namespace: Some(prefix.to_string()),
                local_name: local.to_string(),
            },
        }
    } else {
        // No prefix — check for default namespace
        let default_ns = node.lookup_namespace_uri(None);
        match default_ns {
            Some(uri) => QName::new(uri, value),
            None => QName::local(value),
        }
    }
}

/// Parse a numeric attribute, returning a default if not present or invalid.
fn parse_u64_attr(node: Node<'_, '_>, name: &str, default: u64) -> u64 {
    node.attribute(name)
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(default)
}

/// Parse min_occurs attribute (default 1).
fn parse_min_occurs(node: Node<'_, '_>) -> u64 {
    parse_u64_attr(node, "minOccurs", 1)
}

/// Parse maxOccurs attribute (default Bounded(1)).
fn parse_max_occurs(node: Node<'_, '_>) -> MaxOccurs {
    match node.attribute("maxOccurs") {
        Some("unbounded") => MaxOccurs::Unbounded,
        Some(v) => MaxOccurs::Bounded(v.parse::<u64>().unwrap_or(1)),
        None => MaxOccurs::Bounded(1),
    }
}

/// Check if a node is an XS-namespace element with the given local name.
fn is_xs(node: Node<'_, '_>, local: &str) -> bool {
    node.is_element()
        && node.tag_name().namespace() == Some(XS_NS)
        && node.tag_name().name() == local
}

/// Entry point: parse a `<xs:schema>` node.
pub fn parse_schema(node: Node<'_, '_>) -> Result<RawSchema, SchemaError> {
    let target_namespace = node.attribute("targetNamespace").map(|s| s.to_string());

    let mut schema = RawSchema {
        target_namespace: target_namespace.clone(),
        types: HashMap::new(),
        elements: HashMap::new(),
        attribute_groups: HashMap::new(),
        groups: HashMap::new(),
        imports: Vec::new(),
        includes: Vec::new(),
    };

    for child in node.children().filter(|n| n.is_element()) {
        let tag = child.tag_name();
        if tag.namespace() != Some(XS_NS) {
            continue;
        }
        match tag.name() {
            "complexType" => {
                let ct = visit_complex_type(child);
                let name = child
                    .attribute("name")
                    .map(|n| qualify_name(n, target_namespace.as_deref()));
                if let Some(qname) = name {
                    schema.types.insert(qname, XsdType::Complex(ct));
                }
            }
            "simpleType" => {
                let st = visit_simple_type(child, node);
                let name = child
                    .attribute("name")
                    .map(|n| qualify_name(n, target_namespace.as_deref()));
                if let Some(qname) = name {
                    schema.types.insert(qname, XsdType::Simple(st));
                }
            }
            "element" => {
                let el = visit_element(child);
                let name = child
                    .attribute("name")
                    .map(|n| qualify_name(n, target_namespace.as_deref()));
                if let Some(qname) = name {
                    schema.elements.insert(qname, el);
                }
            }
            "attributeGroup" => {
                if let Some(ag) = visit_attribute_group_def(child) {
                    let qname = qualify_name(&ag.name, target_namespace.as_deref());
                    schema.attribute_groups.insert(qname, ag);
                }
            }
            "group" => {
                if let Some(g) = visit_group_def(child) {
                    let qname = qualify_name(&g.name, target_namespace.as_deref());
                    schema.groups.insert(qname, g);
                }
            }
            "import" => {
                schema.imports.push(SchemaImport {
                    namespace: child.attribute("namespace").map(|s| s.to_string()),
                    schema_location: child.attribute("schemaLocation").map(|s| s.to_string()),
                });
            }
            "include" => {
                if let Some(loc) = child.attribute("schemaLocation") {
                    schema.includes.push(SchemaInclude {
                        schema_location: loc.to_string(),
                    });
                }
            }
            // xs:annotation, xs:documentation, xs:appinfo etc. are silently skipped
            _ => {}
        }
    }

    Ok(schema)
}

/// Create a QName from a local name, using the target namespace if provided.
fn qualify_name(local: &str, target_namespace: Option<&str>) -> QName {
    match target_namespace {
        Some(ns) => QName::new(ns, local),
        None => QName::local(local),
    }
}

// ---------------------------------------------------------------------------
// visit_* functions
// ---------------------------------------------------------------------------

/// Visit xs:complexType → ComplexType.
pub fn visit_complex_type(node: Node<'_, '_>) -> ComplexType {
    let name = node.attribute("name").map(|s| s.to_string());
    let mut attributes: Vec<XsdAttribute> = Vec::new();
    let mut any_attribute: Option<AnyAttribute> = None;
    let mut content = ComplexContent::Empty;

    for child in node.children().filter(|n| n.is_element()) {
        if child.tag_name().namespace() != Some(XS_NS) {
            continue;
        }
        match child.tag_name().name() {
            "sequence" => content = ComplexContent::Sequence(visit_sequence(child)),
            "all" => content = ComplexContent::All(visit_all(child)),
            "choice" => content = ComplexContent::Choice(visit_choice(child)),
            "complexContent" => content = visit_complex_content(child),
            "simpleContent" => content = visit_simple_content(child),
            "attribute" => attributes.push(visit_attribute(child)),
            "attributeGroup" => {
                // Inline attributeGroup ref — expand later in pass 2
                // Store as a ref_attr on a synthetic attribute
                if let Some(r) = child.attribute("ref") {
                    let qname = resolve_qname(r, child);
                    attributes.push(XsdAttribute {
                        name: None,
                        type_ref: None,
                        use_attr: AttributeUse::Optional,
                        default: None,
                        fixed: None,
                        ref_attr: Some(qname),
                    });
                }
            }
            "anyAttribute" => {
                any_attribute = Some(visit_any_attribute(child));
            }
            _ => {} // annotation etc. silently skipped
        }
    }

    // Store anyAttribute as a synthetic attribute entry for now
    // (full AnyAttribute support will be in the ComplexType struct in a later pass)
    let _ = any_attribute; // acknowledged, not yet stored in ComplexType

    ComplexType {
        name,
        content,
        attributes,
    }
}

/// Visit xs:complexContent → ComplexContent (ComplexExtension or ComplexRestriction).
fn visit_complex_content(node: Node<'_, '_>) -> ComplexContent {
    for child in node.children().filter(|n| n.is_element()) {
        if child.tag_name().namespace() != Some(XS_NS) {
            continue;
        }
        match child.tag_name().name() {
            "extension" => {
                let (base, inner_content) = visit_extension(child);
                return ComplexContent::ComplexExtension {
                    base,
                    content: Box::new(inner_content),
                };
            }
            "restriction" => {
                if let Some(base_str) = child.attribute("base") {
                    let base = resolve_qname(base_str, child);
                    let inner_content = visit_complex_restriction_content(child);
                    return ComplexContent::ComplexRestriction {
                        base,
                        content: Box::new(inner_content),
                    };
                }
            }
            _ => {}
        }
    }
    ComplexContent::Empty
}

/// Visit xs:simpleContent → ComplexContent::SimpleContent.
fn visit_simple_content(node: Node<'_, '_>) -> ComplexContent {
    for child in node.children().filter(|n| n.is_element()) {
        if child.tag_name().namespace() != Some(XS_NS) {
            continue;
        }
        if child.tag_name().name() == "extension" {
            if let Some(base_str) = child.attribute("base") {
                let base = resolve_qname(base_str, child);
                let mut attributes = Vec::new();
                for attr_node in child.children().filter(|n| is_xs(*n, "attribute")) {
                    attributes.push(visit_attribute(attr_node));
                }
                return ComplexContent::SimpleContent(SimpleContentDef { base, attributes });
            }
        }
    }
    ComplexContent::Empty
}

/// Visit xs:extension (for complexContent) → (base QName, inner ComplexContent).
pub fn visit_extension(node: Node<'_, '_>) -> (QName, ComplexContent) {
    let base = node
        .attribute("base")
        .map(|b| resolve_qname(b, node))
        .unwrap_or_else(|| QName::local(""));

    let mut inner_content = ComplexContent::Empty;
    let mut attributes: Vec<XsdAttribute> = Vec::new();

    for child in node.children().filter(|n| n.is_element()) {
        if child.tag_name().namespace() != Some(XS_NS) {
            continue;
        }
        match child.tag_name().name() {
            "sequence" => inner_content = ComplexContent::Sequence(visit_sequence(child)),
            "all" => inner_content = ComplexContent::All(visit_all(child)),
            "choice" => inner_content = ComplexContent::Choice(visit_choice(child)),
            "attribute" => attributes.push(visit_attribute(child)),
            _ => {}
        }
    }

    // If extension has attributes, wrap them in the content by returning a ComplexExtension
    // with a Sequence and leave the attributes at this level.
    // The ComplexType visit will pick them up from the extension node's attributes list.
    // For simplicity in pass 1, we return the content only — attributes are handled by caller.
    let _ = attributes;

    (base, inner_content)
}

/// Visit restriction content inside xs:complexContent (structural content, not facets).
fn visit_complex_restriction_content(node: Node<'_, '_>) -> ComplexContent {
    for child in node.children().filter(|n| n.is_element()) {
        if child.tag_name().namespace() != Some(XS_NS) {
            continue;
        }
        match child.tag_name().name() {
            "sequence" => return ComplexContent::Sequence(visit_sequence(child)),
            "all" => return ComplexContent::All(visit_all(child)),
            "choice" => return ComplexContent::Choice(visit_choice(child)),
            _ => {}
        }
    }
    ComplexContent::Empty
}

/// Visit xs:simpleType → SimpleType.
pub fn visit_simple_type(node: Node<'_, '_>, context: Node<'_, '_>) -> SimpleType {
    let name = node.attribute("name").map(|s| s.to_string());
    let mut restriction = None;
    let mut list = None;
    let mut union = None;

    for child in node.children().filter(|n| n.is_element()) {
        if child.tag_name().namespace() != Some(XS_NS) {
            continue;
        }
        match child.tag_name().name() {
            "restriction" => restriction = Some(visit_restriction(child, context)),
            "list" => list = Some(visit_list(child, context)),
            "union" => union = Some(visit_union(child, context)),
            _ => {}
        }
    }

    SimpleType {
        name,
        restriction,
        list,
        union,
    }
}

/// Visit xs:sequence → Vec<XsdElement>.
pub fn visit_sequence(node: Node<'_, '_>) -> Vec<XsdElement> {
    visit_particle_children(node)
}

/// Visit xs:all → Vec<XsdElement>.
pub fn visit_all(node: Node<'_, '_>) -> Vec<XsdElement> {
    visit_particle_children(node)
}

/// Visit xs:choice → Vec<XsdElement>.
pub fn visit_choice(node: Node<'_, '_>) -> Vec<XsdElement> {
    visit_particle_children(node)
}

/// Common particle children visitor (element, group ref, any, sequence, choice, all).
fn visit_particle_children(node: Node<'_, '_>) -> Vec<XsdElement> {
    let mut elements = Vec::new();
    for child in node.children().filter(|n| n.is_element()) {
        if child.tag_name().namespace() != Some(XS_NS) {
            continue;
        }
        match child.tag_name().name() {
            "element" => elements.push(visit_element(child)),
            "any" => {
                // Convert AnyElement to a synthetic XsdElement for storage
                let any = visit_any(child);
                elements.push(XsdElement {
                    name: Some("__any__".to_string()),
                    type_ref: None,
                    inline_type: None,
                    min_occurs: any.min_occurs,
                    max_occurs: any.max_occurs,
                    nillable: false,
                    default: None,
                    fixed: None,
                    ref_attr: None,
                });
            }
            // Nested compositors — recursively collect
            "sequence" => elements.extend(visit_sequence(child)),
            "choice" => elements.extend(visit_choice(child)),
            "all" => elements.extend(visit_all(child)),
            _ => {} // annotation, group ref etc.
        }
    }
    elements
}

/// Visit xs:element → XsdElement.
pub fn visit_element(node: Node<'_, '_>) -> XsdElement {
    let name = node.attribute("name").map(|s| s.to_string());
    let type_ref = node.attribute("type").map(|t| resolve_qname(t, node));
    let ref_attr = node.attribute("ref").map(|r| resolve_qname(r, node));
    let min_occurs = parse_min_occurs(node);
    let max_occurs = parse_max_occurs(node);
    let nillable = node
        .attribute("nillable")
        .map(|v| v == "true")
        .unwrap_or(false);
    let default = node.attribute("default").map(|s| s.to_string());
    let fixed = node.attribute("fixed").map(|s| s.to_string());

    // Inline type definitions (anonymous complexType / simpleType inside element)
    let inline_type = node
        .children()
        .filter(|n| n.is_element() && n.tag_name().namespace() == Some(XS_NS))
        .find_map(|child| match child.tag_name().name() {
            "complexType" => Some(Box::new(XsdType::Complex(visit_complex_type(child)))),
            "simpleType" => {
                Some(Box::new(XsdType::Simple(visit_simple_type(child, node))))
            }
            _ => None,
        });

    XsdElement {
        name,
        type_ref,
        inline_type,
        min_occurs,
        max_occurs,
        nillable,
        default,
        fixed,
        ref_attr,
    }
}

/// Visit xs:attribute → XsdAttribute.
pub fn visit_attribute(node: Node<'_, '_>) -> XsdAttribute {
    let name = node.attribute("name").map(|s| s.to_string());
    let type_ref = node.attribute("type").map(|t| resolve_qname(t, node));
    let ref_attr = node.attribute("ref").map(|r| resolve_qname(r, node));
    let use_attr = match node.attribute("use") {
        Some("required") => AttributeUse::Required,
        Some("prohibited") => AttributeUse::Prohibited,
        _ => AttributeUse::Optional,
    };
    let default = node.attribute("default").map(|s| s.to_string());
    let fixed = node.attribute("fixed").map(|s| s.to_string());

    XsdAttribute {
        name,
        type_ref,
        use_attr,
        default,
        fixed,
        ref_attr,
    }
}

/// Visit xs:attributeGroup (definition) → Option<AttributeGroup>.
pub fn visit_attribute_group_def(node: Node<'_, '_>) -> Option<AttributeGroup> {
    let name = node.attribute("name")?.to_string();
    let mut attributes = Vec::new();
    let mut attribute_groups = Vec::new();

    for child in node.children().filter(|n| n.is_element()) {
        if child.tag_name().namespace() != Some(XS_NS) {
            continue;
        }
        match child.tag_name().name() {
            "attribute" => attributes.push(visit_attribute(child)),
            "attributeGroup" => {
                if let Some(r) = child.attribute("ref") {
                    attribute_groups.push(resolve_qname(r, child));
                }
            }
            "anyAttribute" => {} // handle in pass 2
            _ => {}
        }
    }

    Some(AttributeGroup {
        name,
        attributes,
        attribute_groups,
    })
}

/// Visit xs:group (definition) → Option<Group>.
pub fn visit_group_def(node: Node<'_, '_>) -> Option<Group> {
    let name = node.attribute("name")?.to_string();
    let mut content = None;

    for child in node.children().filter(|n| n.is_element()) {
        if child.tag_name().namespace() != Some(XS_NS) {
            continue;
        }
        match child.tag_name().name() {
            "sequence" => content = Some(GroupContent::Sequence(visit_sequence(child))),
            "all" => content = Some(GroupContent::All(visit_all(child))),
            "choice" => content = Some(GroupContent::Choice(visit_choice(child))),
            _ => {}
        }
    }

    Some(Group {
        name,
        content: content.unwrap_or(GroupContent::Sequence(Vec::new())),
    })
}

/// Visit xs:any → AnyElement.
pub fn visit_any(node: Node<'_, '_>) -> AnyElement {
    let namespace = match node.attribute("namespace") {
        Some("##any") | None => AnyNamespace::Any,
        Some("##other") => AnyNamespace::Other,
        Some(v) => AnyNamespace::List(v.split_whitespace().map(|s| s.to_string()).collect()),
    };
    let process_contents = match node.attribute("processContents") {
        Some("lax") => ProcessContents::Lax,
        Some("skip") => ProcessContents::Skip,
        _ => ProcessContents::Strict,
    };
    let min_occurs = parse_min_occurs(node);
    let max_occurs = parse_max_occurs(node);

    AnyElement {
        namespace,
        process_contents,
        min_occurs,
        max_occurs,
    }
}

/// Visit xs:anyAttribute → AnyAttribute.
pub fn visit_any_attribute(node: Node<'_, '_>) -> AnyAttribute {
    let namespace = match node.attribute("namespace") {
        Some("##any") | None => AnyNamespace::Any,
        Some("##other") => AnyNamespace::Other,
        Some(v) => AnyNamespace::List(v.split_whitespace().map(|s| s.to_string()).collect()),
    };
    let process_contents = match node.attribute("processContents") {
        Some("lax") => ProcessContents::Lax,
        Some("skip") => ProcessContents::Skip,
        _ => ProcessContents::Strict,
    };

    AnyAttribute {
        namespace,
        process_contents,
    }
}

/// Visit xs:restriction (in simpleType context) → Restriction.
pub fn visit_restriction(node: Node<'_, '_>, context: Node<'_, '_>) -> Restriction {
    let base = node
        .attribute("base")
        .map(|b| resolve_qname(b, node))
        .unwrap_or_else(|| QName::local(""));

    let mut enumeration = Vec::new();
    let mut min_inclusive = None;
    let mut max_inclusive = None;
    let mut min_exclusive = None;
    let mut max_exclusive = None;
    let mut min_length = None;
    let mut max_length = None;
    let mut length = None;
    let mut pattern = None;
    let mut whitespace = None;
    let mut total_digits = None;
    let mut fraction_digits = None;

    for child in node.children().filter(|n| n.is_element()) {
        if child.tag_name().namespace() != Some(XS_NS) {
            continue;
        }
        match child.tag_name().name() {
            "enumeration" => {
                if let Some(v) = child.attribute("value") {
                    enumeration.push(v.to_string());
                }
            }
            "minInclusive" => min_inclusive = child.attribute("value").map(|s| s.to_string()),
            "maxInclusive" => max_inclusive = child.attribute("value").map(|s| s.to_string()),
            "minExclusive" => min_exclusive = child.attribute("value").map(|s| s.to_string()),
            "maxExclusive" => max_exclusive = child.attribute("value").map(|s| s.to_string()),
            "minLength" => {
                min_length = child.attribute("value").and_then(|v| v.parse().ok())
            }
            "maxLength" => {
                max_length = child.attribute("value").and_then(|v| v.parse().ok())
            }
            "length" => length = child.attribute("value").and_then(|v| v.parse().ok()),
            "pattern" => pattern = child.attribute("value").map(|s| s.to_string()),
            "whiteSpace" => {
                whitespace = match child.attribute("value") {
                    Some("preserve") => Some(WhitespaceHandling::Preserve),
                    Some("replace") => Some(WhitespaceHandling::Replace),
                    Some("collapse") => Some(WhitespaceHandling::Collapse),
                    _ => None,
                }
            }
            "totalDigits" => {
                total_digits = child.attribute("value").and_then(|v| v.parse().ok())
            }
            "fractionDigits" => {
                fraction_digits = child.attribute("value").and_then(|v| v.parse().ok())
            }
            _ => {}
        }
    }

    let _ = context; // used by callers for namespace resolution context

    Restriction {
        base,
        enumeration,
        min_inclusive,
        max_inclusive,
        min_exclusive,
        max_exclusive,
        min_length,
        max_length,
        length,
        pattern,
        whitespace,
        total_digits,
        fraction_digits,
    }
}

/// Visit xs:list → ListDef.
pub fn visit_list(node: Node<'_, '_>, _context: Node<'_, '_>) -> ListDef {
    let item_type = node
        .attribute("itemType")
        .map(|t| resolve_qname(t, node))
        .unwrap_or_else(|| QName::local(""));
    ListDef { item_type }
}

/// Visit xs:union → UnionDef.
pub fn visit_union(node: Node<'_, '_>, _context: Node<'_, '_>) -> UnionDef {
    let member_types = node
        .attribute("memberTypes")
        .map(|mt| {
            mt.split_whitespace()
                .map(|t| resolve_qname(t, node))
                .collect()
        })
        .unwrap_or_default();
    UnionDef { member_types }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(xml: &str) -> RawSchema {
        let doc = roxmltree::Document::parse(xml).expect("XML parse failed");
        parse_schema(doc.root_element()).expect("schema parse failed")
    }

    #[test]
    fn parse_complex_type_with_sequence_element() {
        let schema = parse(
            r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema" targetNamespace="urn:t">
                 <xs:complexType name="Foo">
                   <xs:sequence>
                     <xs:element name="bar" type="xs:string"/>
                   </xs:sequence>
                 </xs:complexType>
               </xs:schema>"#,
        );
        let qname = QName::new("urn:t", "Foo");
        let ty = schema.types.get(&qname).expect("Foo not found");
        assert!(matches!(ty, XsdType::Complex(_)));
        if let XsdType::Complex(ct) = ty {
            assert!(matches!(ct.content, ComplexContent::Sequence(_)));
            if let ComplexContent::Sequence(elems) = &ct.content {
                assert_eq!(elems.len(), 1);
                assert_eq!(elems[0].name.as_deref(), Some("bar"));
            }
        }
    }

    #[test]
    fn parse_element_min_occurs_zero() {
        let schema = parse(
            r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema" targetNamespace="urn:t">
                 <xs:complexType name="CT">
                   <xs:sequence>
                     <xs:element name="opt" type="xs:string" minOccurs="0"/>
                   </xs:sequence>
                 </xs:complexType>
               </xs:schema>"#,
        );
        let qname = QName::new("urn:t", "CT");
        let ty = schema.types.get(&qname).unwrap();
        if let XsdType::Complex(ct) = ty {
            if let ComplexContent::Sequence(elems) = &ct.content {
                assert_eq!(elems[0].min_occurs, 0);
            }
        }
    }

    #[test]
    fn parse_element_max_occurs_unbounded() {
        let schema = parse(
            r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema" targetNamespace="urn:t">
                 <xs:complexType name="CT">
                   <xs:sequence>
                     <xs:element name="items" type="xs:string" maxOccurs="unbounded"/>
                   </xs:sequence>
                 </xs:complexType>
               </xs:schema>"#,
        );
        let qname = QName::new("urn:t", "CT");
        let ty = schema.types.get(&qname).unwrap();
        if let XsdType::Complex(ct) = ty {
            if let ComplexContent::Sequence(elems) = &ct.content {
                assert_eq!(elems[0].max_occurs, MaxOccurs::Unbounded);
            }
        }
    }

    #[test]
    fn parse_element_nillable_true() {
        let schema = parse(
            r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema" targetNamespace="urn:t">
                 <xs:complexType name="CT">
                   <xs:sequence>
                     <xs:element name="val" type="xs:string" nillable="true"/>
                   </xs:sequence>
                 </xs:complexType>
               </xs:schema>"#,
        );
        let ty = schema.types.get(&QName::new("urn:t", "CT")).unwrap();
        if let XsdType::Complex(ct) = ty {
            if let ComplexContent::Sequence(elems) = &ct.content {
                assert!(elems[0].nillable);
            }
        }
    }

    #[test]
    fn parse_element_ref_attribute() {
        let schema = parse(
            r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema"
                          xmlns:tns="urn:t" targetNamespace="urn:t">
                 <xs:complexType name="CT">
                   <xs:sequence>
                     <xs:element ref="tns:Foo"/>
                   </xs:sequence>
                 </xs:complexType>
               </xs:schema>"#,
        );
        let ty = schema.types.get(&QName::new("urn:t", "CT")).unwrap();
        if let XsdType::Complex(ct) = ty {
            if let ComplexContent::Sequence(elems) = &ct.content {
                let ref_attr = elems[0].ref_attr.as_ref().expect("ref_attr missing");
                assert_eq!(ref_attr.local_name, "Foo");
                assert_eq!(ref_attr.namespace.as_deref(), Some("urn:t"));
            }
        }
    }

    #[test]
    fn parse_complex_extension() {
        let schema = parse(
            r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema"
                          xmlns:tns="urn:t" targetNamespace="urn:t">
                 <xs:complexType name="Child">
                   <xs:complexContent>
                     <xs:extension base="tns:Base">
                       <xs:sequence>
                         <xs:element name="extra" type="xs:string"/>
                       </xs:sequence>
                     </xs:extension>
                   </xs:complexContent>
                 </xs:complexType>
               </xs:schema>"#,
        );
        let ty = schema.types.get(&QName::new("urn:t", "Child")).unwrap();
        if let XsdType::Complex(ct) = ty {
            if let ComplexContent::ComplexExtension { base, .. } = &ct.content {
                assert_eq!(base.local_name, "Base");
                assert_eq!(base.namespace.as_deref(), Some("urn:t"));
            } else {
                panic!("Expected ComplexExtension, got {:?}", ct.content);
            }
        }
    }

    #[test]
    fn parse_complex_restriction() {
        let schema = parse(
            r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema"
                          xmlns:tns="urn:t" targetNamespace="urn:t">
                 <xs:complexType name="Restricted">
                   <xs:complexContent>
                     <xs:restriction base="tns:Base">
                       <xs:sequence/>
                     </xs:restriction>
                   </xs:complexContent>
                 </xs:complexType>
               </xs:schema>"#,
        );
        let ty = schema.types.get(&QName::new("urn:t", "Restricted")).unwrap();
        if let XsdType::Complex(ct) = ty {
            assert!(matches!(ct.content, ComplexContent::ComplexRestriction { .. }));
        }
    }

    #[test]
    fn parse_simple_type_enumeration() {
        let schema = parse(
            r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema" targetNamespace="urn:t">
                 <xs:simpleType name="ColorEnum">
                   <xs:restriction base="xs:string">
                     <xs:enumeration value="JPEG"/>
                     <xs:enumeration value="PNG"/>
                   </xs:restriction>
                 </xs:simpleType>
               </xs:schema>"#,
        );
        let ty = schema.types.get(&QName::new("urn:t", "ColorEnum")).unwrap();
        if let XsdType::Simple(st) = ty {
            let r = st.restriction.as_ref().unwrap();
            assert!(r.enumeration.contains(&"JPEG".to_string()));
            assert!(r.enumeration.contains(&"PNG".to_string()));
        } else {
            panic!("Expected Simple type");
        }
    }

    #[test]
    fn parse_simple_type_list() {
        let schema = parse(
            r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema" targetNamespace="urn:t">
                 <xs:simpleType name="StringList">
                   <xs:list itemType="xs:string"/>
                 </xs:simpleType>
               </xs:schema>"#,
        );
        let ty = schema.types.get(&QName::new("urn:t", "StringList")).unwrap();
        if let XsdType::Simple(st) = ty {
            assert!(st.list.is_some());
            assert_eq!(st.list.as_ref().unwrap().item_type.local_name, "string");
        }
    }

    #[test]
    fn parse_simple_type_union() {
        let schema = parse(
            r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema" targetNamespace="urn:t">
                 <xs:simpleType name="StringOrInt">
                   <xs:union memberTypes="xs:string xs:integer"/>
                 </xs:simpleType>
               </xs:schema>"#,
        );
        let ty = schema.types.get(&QName::new("urn:t", "StringOrInt")).unwrap();
        if let XsdType::Simple(st) = ty {
            let u = st.union.as_ref().unwrap();
            assert_eq!(u.member_types.len(), 2);
        }
    }

    #[test]
    fn parse_import_recorded() {
        let schema = parse(
            r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema" targetNamespace="urn:t">
                 <xs:import namespace="urn:foo" schemaLocation="foo.xsd"/>
               </xs:schema>"#,
        );
        assert_eq!(schema.imports.len(), 1);
        assert_eq!(schema.imports[0].namespace.as_deref(), Some("urn:foo"));
        assert_eq!(
            schema.imports[0].schema_location.as_deref(),
            Some("foo.xsd")
        );
    }

    #[test]
    fn parse_attribute_use_required() {
        let schema = parse(
            r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema"
                          xmlns:tt="urn:tt" targetNamespace="urn:t">
                 <xs:complexType name="CT">
                   <xs:attribute name="token" type="tt:ReferenceToken" use="required"/>
                 </xs:complexType>
               </xs:schema>"#,
        );
        let ty = schema.types.get(&QName::new("urn:t", "CT")).unwrap();
        if let XsdType::Complex(ct) = ty {
            assert_eq!(ct.attributes.len(), 1);
            assert_eq!(ct.attributes[0].use_attr, AttributeUse::Required);
            assert_eq!(ct.attributes[0].name.as_deref(), Some("token"));
        }
    }

    #[test]
    fn parse_attribute_group_definition() {
        let schema = parse(
            r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema" targetNamespace="urn:t">
                 <xs:attributeGroup name="AG">
                   <xs:attribute name="id" type="xs:ID" use="optional"/>
                 </xs:attributeGroup>
               </xs:schema>"#,
        );
        let qname = QName::new("urn:t", "AG");
        let ag = schema.attribute_groups.get(&qname).expect("AG not found");
        assert_eq!(ag.name, "AG");
        assert_eq!(ag.attributes.len(), 1);
    }

    #[test]
    fn parse_group_definition_sequence() {
        let schema = parse(
            r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema" targetNamespace="urn:t">
                 <xs:group name="G">
                   <xs:sequence>
                     <xs:element name="item" type="xs:string"/>
                   </xs:sequence>
                 </xs:group>
               </xs:schema>"#,
        );
        let qname = QName::new("urn:t", "G");
        let g = schema.groups.get(&qname).expect("G not found");
        assert!(matches!(g.content, GroupContent::Sequence(_)));
    }

    #[test]
    fn parse_any_element() {
        let schema = parse(
            r###"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema" targetNamespace="urn:t">
                 <xs:complexType name="CT">
                   <xs:sequence>
                     <xs:any namespace="##any" processContents="lax"/>
                   </xs:sequence>
                 </xs:complexType>
               </xs:schema>"###,
        );
        // The xs:any is stored as a synthetic element named __any__
        let ty = schema.types.get(&QName::new("urn:t", "CT")).unwrap();
        if let XsdType::Complex(ct) = ty {
            if let ComplexContent::Sequence(elems) = &ct.content {
                assert_eq!(elems.len(), 1);
                assert_eq!(elems[0].name.as_deref(), Some("__any__"));
            }
        }
    }

    #[test]
    fn parse_unknown_elements_silently_skipped() {
        // xs:annotation and xs:documentation should not cause errors
        let schema = parse(
            r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema" targetNamespace="urn:t">
                 <xs:annotation>
                   <xs:documentation>Hello</xs:documentation>
                 </xs:annotation>
                 <xs:complexType name="Foo">
                   <xs:sequence/>
                 </xs:complexType>
               </xs:schema>"#,
        );
        assert!(schema.types.get(&QName::new("urn:t", "Foo")).is_some());
    }

    #[test]
    fn parse_choice_compositor() {
        let schema = parse(
            r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema" targetNamespace="urn:t">
                 <xs:complexType name="CT">
                   <xs:choice>
                     <xs:element name="a" type="xs:string"/>
                     <xs:element name="b" type="xs:integer"/>
                   </xs:choice>
                 </xs:complexType>
               </xs:schema>"#,
        );
        let ty = schema.types.get(&QName::new("urn:t", "CT")).unwrap();
        if let XsdType::Complex(ct) = ty {
            assert!(matches!(ct.content, ComplexContent::Choice(_)));
            if let ComplexContent::Choice(elems) = &ct.content {
                assert_eq!(elems.len(), 2);
            }
        }
    }

    #[test]
    fn parse_all_compositor() {
        let schema = parse(
            r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema" targetNamespace="urn:t">
                 <xs:complexType name="CT">
                   <xs:all>
                     <xs:element name="x" type="xs:string"/>
                   </xs:all>
                 </xs:complexType>
               </xs:schema>"#,
        );
        let ty = schema.types.get(&QName::new("urn:t", "CT")).unwrap();
        if let XsdType::Complex(ct) = ty {
            assert!(matches!(ct.content, ComplexContent::All(_)));
        }
    }

    #[test]
    fn parse_top_level_element() {
        let schema = parse(
            r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema" targetNamespace="urn:t">
                 <xs:element name="Root" type="xs:string"/>
               </xs:schema>"#,
        );
        let qname = QName::new("urn:t", "Root");
        assert!(schema.elements.contains_key(&qname));
    }

    #[test]
    fn parse_schema_include() {
        let schema = parse(
            r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema" targetNamespace="urn:t">
                 <xs:include schemaLocation="common.xsd"/>
               </xs:schema>"#,
        );
        assert_eq!(schema.includes.len(), 1);
        assert_eq!(schema.includes[0].schema_location, "common.xsd");
    }

    #[test]
    fn parse_element_default_and_fixed() {
        let schema = parse(
            r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema" targetNamespace="urn:t">
                 <xs:complexType name="CT">
                   <xs:sequence>
                     <xs:element name="d" type="xs:string" default="hello"/>
                     <xs:element name="f" type="xs:string" fixed="world"/>
                   </xs:sequence>
                 </xs:complexType>
               </xs:schema>"#,
        );
        let ty = schema.types.get(&QName::new("urn:t", "CT")).unwrap();
        if let XsdType::Complex(ct) = ty {
            if let ComplexContent::Sequence(elems) = &ct.content {
                assert_eq!(elems[0].default.as_deref(), Some("hello"));
                assert_eq!(elems[1].fixed.as_deref(), Some("world"));
            }
        }
    }

    #[test]
    fn parse_restriction_facets() {
        let schema = parse(
            r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema" targetNamespace="urn:t">
                 <xs:simpleType name="Range">
                   <xs:restriction base="xs:integer">
                     <xs:minInclusive value="1"/>
                     <xs:maxInclusive value="100"/>
                     <xs:pattern value="\d+"/>
                   </xs:restriction>
                 </xs:simpleType>
               </xs:schema>"#,
        );
        let ty = schema.types.get(&QName::new("urn:t", "Range")).unwrap();
        if let XsdType::Simple(st) = ty {
            let r = st.restriction.as_ref().unwrap();
            assert_eq!(r.min_inclusive.as_deref(), Some("1"));
            assert_eq!(r.max_inclusive.as_deref(), Some("100"));
            assert_eq!(r.pattern.as_deref(), Some(r"\d+"));
        }
    }
}
