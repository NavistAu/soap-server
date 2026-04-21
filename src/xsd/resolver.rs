// XSD Pass 2 resolver — resolve forward references, flatten extension chains, load imports
use std::collections::{HashMap, HashSet};
use crate::qname::QName;
use crate::xsd::types::{XsdType, ComplexType, ComplexContent, TypeRegistry};
use crate::xsd::elements::{XsdElement, XsdAttribute};
use crate::xsd::parser::{RawSchema, SchemaError};

/// Abstracts file/network I/O for loading imported schemas during resolution.
/// Tests implement MockSchemaLoader; production uses FileSchemaLoader from wsdl::resolver.
pub trait SchemaLoader: Send + Sync {
    fn load(&self, namespace: Option<&str>, location: &str) -> Result<String, SchemaError>;
}

/// A SchemaLoader that always returns Err — for tests that don't need imports.
pub struct NullSchemaLoader;

impl SchemaLoader for NullSchemaLoader {
    fn load(&self, _namespace: Option<&str>, location: &str) -> Result<String, SchemaError> {
        Err(SchemaError::UnknownRef(format!("NullSchemaLoader cannot load: {location}")))
    }
}

/// Flat map of all types collected from root schema + all transitively imported/included schemas.
struct FlatTypeMap {
    types: HashMap<QName, XsdType>,
    attribute_groups: HashMap<QName, crate::xsd::elements::AttributeGroup>,
    groups: HashMap<QName, crate::xsd::elements::Group>,
    elements: HashMap<QName, XsdElement>,
}

/// Entry point: resolve a RawSchema (and its transitive imports/includes) into a TypeRegistry.
///
/// `already_loaded` is keyed by schema location string; it prevents loading the same schema
/// twice even when referenced from multiple locations (diamond import pattern).
pub fn resolve_schema(
    raw: RawSchema,
    loader: &dyn SchemaLoader,
    already_loaded: &mut HashMap<String, ()>,
) -> Result<TypeRegistry, SchemaError> {
    // Step 1: collect all raw schemas (root + imports + includes), deduplicating by location.
    let flat = collect_all_schemas(raw, loader, already_loaded)?;

    // Step 2: resolve all types in the flat map into a TypeRegistry.
    let mut registry = TypeRegistry::new();
    let mut resolved: HashMap<QName, ComplexType> = HashMap::new();

    // Pre-populate registry with simple types and pre-resolved complex types.
    for (qname, xsd_type) in &flat.types {
        if let XsdType::Simple(_) = xsd_type {
            registry.insert(qname.clone(), xsd_type.clone());
        }
    }

    // Resolve each complex type.
    let complex_qnames: Vec<QName> = flat.types.keys()
        .filter(|k| matches!(flat.types[*k], XsdType::Complex(_)))
        .cloned()
        .collect();

    for qname in complex_qnames {
        if !resolved.contains_key(&qname) {
            let mut resolving = HashSet::new();
            let ct = resolve_named_type(&qname, &flat, &mut resolved, &mut resolving)?;
            resolved.insert(qname, ct);
        }
    }

    // Merge resolved complex types into registry.
    for (qname, ct) in resolved {
        registry.insert(qname, XsdType::Complex(Box::new(ct)));
    }

    Ok(registry)
}

/// Recursively collect raw schemas from root + all imports/includes.
fn collect_all_schemas(
    raw: RawSchema,
    loader: &dyn SchemaLoader,
    already_loaded: &mut HashMap<String, ()>,
) -> Result<FlatTypeMap, SchemaError> {
    let mut flat = FlatTypeMap {
        types: HashMap::new(),
        attribute_groups: HashMap::new(),
        groups: HashMap::new(),
        elements: HashMap::new(),
    };

    // Queue of schemas to process.
    let mut queue: Vec<RawSchema> = vec![raw];

    while let Some(schema) = queue.pop() {
        // Merge this schema's types into flat map.
        flat.types.extend(schema.types);
        flat.attribute_groups.extend(schema.attribute_groups);
        flat.groups.extend(schema.groups);
        flat.elements.extend(schema.elements);

        // Process imports.
        for import in schema.imports {
            if let Some(location) = import.schema_location {
                if already_loaded.contains_key(&location) {
                    continue;
                }
                already_loaded.insert(location.clone(), ());
                let xml_text = loader.load(import.namespace.as_deref(), &location)?;
                let doc = roxmltree::Document::parse(&xml_text)
                    .map_err(|e| SchemaError::MalformedXml(e.to_string()))?;
                let imported_raw = crate::xsd::parser::parse_schema(doc.root_element())?;
                queue.push(imported_raw);
            }
        }

        // Process includes.
        for include in schema.includes {
            let location = include.schema_location;
            if already_loaded.contains_key(&location) {
                continue;
            }
            already_loaded.insert(location.clone(), ());
            let xml_text = loader.load(None, &location)?;
            let doc = roxmltree::Document::parse(&xml_text)
                .map_err(|e| SchemaError::MalformedXml(e.to_string()))?;
            let included_raw = crate::xsd::parser::parse_schema(doc.root_element())?;
            queue.push(included_raw);
        }
    }

    Ok(flat)
}

/// Resolve a named complex type, recursing into base types first (ancestor elements first).
fn resolve_named_type(
    qname: &QName,
    flat: &FlatTypeMap,
    resolved: &mut HashMap<QName, ComplexType>,
    resolving: &mut HashSet<QName>,
) -> Result<ComplexType, SchemaError> {
    // If already resolved, return clone.
    if let Some(ct) = resolved.get(qname) {
        return Ok(ct.clone());
    }

    // Cycle detection.
    if resolving.contains(qname) {
        return Err(SchemaError::CycleDetected(qname.to_string()));
    }

    // Look up the raw type.
    // If the type is from an external/unloaded schema, treat it as opaque (empty complex type).
    // This allows WSDL loading to succeed for partial schemas (e.g., ONVIF with external imports
    // from wsn/b-2, xop/include, etc.) — unknown references are not dispatch-critical.
    let xsd_type = match flat.types.get(qname) {
        Some(t) => t,
        None => {
            return Ok(ComplexType {
                name: None,
                content: ComplexContent::Empty,
                attributes: vec![],
            });
        }
    };

    let raw_ct = match xsd_type {
        XsdType::Complex(ct) => ct.clone(),
        XsdType::Simple(_) => {
            // Simple types don't need complex resolution; return a pass-through.
            return Ok(ComplexType {
                name: None,
                content: ComplexContent::Empty,
                attributes: vec![],
            });
        }
    };

    resolving.insert(qname.clone());
    let result = resolve_complex_type(&raw_ct, flat, resolved, resolving)?;
    resolving.remove(qname);

    resolved.insert(qname.clone(), result.clone());
    Ok(result)
}

/// Resolve a ComplexType, expanding extensions and restrictions.
fn resolve_complex_type(
    raw: &ComplexType,
    flat: &FlatTypeMap,
    resolved: &mut HashMap<QName, ComplexType>,
    resolving: &mut HashSet<QName>,
) -> Result<ComplexType, SchemaError> {
    let content = resolve_content(&raw.content, flat, resolved, resolving)?;
    let attributes = resolve_attributes(&raw.attributes, flat);

    Ok(ComplexType {
        name: raw.name.clone(),
        content,
        attributes,
    })
}

/// Resolve a ComplexContent, handling extension chains, restrictions, and inline refs.
fn resolve_content(
    content: &ComplexContent,
    flat: &FlatTypeMap,
    resolved: &mut HashMap<QName, ComplexType>,
    resolving: &mut HashSet<QName>,
) -> Result<ComplexContent, SchemaError> {
    match content {
        ComplexContent::ComplexExtension { base, content: child_content } => {
            // Resolve base type first (bottom-up: deepest ancestor first).
            let base_ct = resolve_named_type(base, flat, resolved, resolving)?;

            // Get all elements from the base type (ancestor elements first).
            let mut all_elements = extract_elements(&base_ct.content);

            // Resolve child content and append after base elements.
            let child_resolved = resolve_content(child_content, flat, resolved, resolving)?;
            let child_elements = extract_elements(&child_resolved);
            all_elements.extend(child_elements);

            // Note: extension-level attributes are part of the raw ComplexType.attributes field,
            // which is handled at the outer resolve_complex_type level.

            Ok(ComplexContent::Sequence(all_elements))
        }

        ComplexContent::ComplexRestriction { base: _, content: restriction_content } => {
            // Restriction: use only the restriction's own content model (not the base's).
            resolve_content(restriction_content, flat, resolved, resolving)
        }

        ComplexContent::Sequence(elements) => {
            let resolved_elements = resolve_element_list(elements, flat, resolved, resolving)?;
            Ok(ComplexContent::Sequence(resolved_elements))
        }

        ComplexContent::All(elements) => {
            let resolved_elements = resolve_element_list(elements, flat, resolved, resolving)?;
            Ok(ComplexContent::All(resolved_elements))
        }

        ComplexContent::Choice(elements) => {
            let resolved_elements = resolve_element_list(elements, flat, resolved, resolving)?;
            Ok(ComplexContent::Choice(resolved_elements))
        }

        ComplexContent::SimpleContent(scd) => {
            // SimpleContent doesn't need element resolution.
            Ok(ComplexContent::SimpleContent(scd.clone()))
        }

        ComplexContent::Empty => Ok(ComplexContent::Empty),
    }
}

/// Extract the flat list of elements from any content model variant.
fn extract_elements(content: &ComplexContent) -> Vec<XsdElement> {
    match content {
        ComplexContent::Sequence(els) | ComplexContent::All(els) | ComplexContent::Choice(els) => {
            els.clone()
        }
        ComplexContent::ComplexExtension { .. } | ComplexContent::ComplexRestriction { .. } => {
            // Should not occur after resolution, but return empty to be safe.
            vec![]
        }
        ComplexContent::SimpleContent(_) | ComplexContent::Empty => vec![],
    }
}

/// Resolve a list of elements, inlining xs:group refs and resolving xs:element refs.
#[allow(clippy::only_used_in_recursion)]
fn resolve_element_list(
    elements: &[XsdElement],
    flat: &FlatTypeMap,
    resolved: &mut HashMap<QName, ComplexType>,
    resolving: &mut HashSet<QName>,
) -> Result<Vec<XsdElement>, SchemaError> {
    let mut result = Vec::new();

    for el in elements {
        // Check for xs:any synthetic element (stored as __any__ by parser).
        if el.name.as_deref() == Some("__any__") {
            result.push(el.clone());
            continue;
        }

        // Check for xs:group ref= (stored as element with name __group__:<qname> by convention,
        // OR detected via ref_attr pointing to a Group in flat.groups).
        if let Some(ref group_qname) = el.ref_attr {
            if flat.groups.contains_key(group_qname) {
                // Inline the group's content.
                let group = &flat.groups[group_qname];
                let group_elements = match &group.content {
                    crate::xsd::elements::GroupContent::Sequence(els) => els.clone(),
                    crate::xsd::elements::GroupContent::All(els) => els.clone(),
                    crate::xsd::elements::GroupContent::Choice(els) => els.clone(),
                };
                let inlined = resolve_element_list(&group_elements, flat, resolved, resolving)?;
                result.extend(inlined);
                continue;
            }

            // xs:element ref= — resolve to the global element definition.
            if let Some(global_el) = flat.elements.get(group_qname) {
                result.push(global_el.clone());
                continue;
            }

            // Unknown element ref (e.g., from an external/unloaded schema like xop:Include).
            // Skip it — unresolvable references are treated as optional/opaque for dispatch
            // purposes. The validate_request step will not fail on unknown types.
            continue;
        }

        result.push(el.clone());
    }

    Ok(result)
}

/// Resolve attributes list, expanding xs:attributeGroup refs.
fn resolve_attributes(
    attrs: &[XsdAttribute],
    flat: &FlatTypeMap,
) -> Vec<XsdAttribute> {
    let mut result = Vec::new();
    for attr in attrs {
        if let Some(ref ag_ref) = attr.ref_attr {
            if let Some(ag) = flat.attribute_groups.get(ag_ref) {
                // Inline the attribute group's attributes (recursively expand nested groups).
                let expanded = expand_attribute_group(ag, flat);
                result.extend(expanded);
                continue;
            }
        }
        result.push(attr.clone());
    }
    result
}

/// Recursively expand an attribute group into a flat list of attributes.
fn expand_attribute_group(
    ag: &crate::xsd::elements::AttributeGroup,
    flat: &FlatTypeMap,
) -> Vec<XsdAttribute> {
    let mut result = ag.attributes.clone();
    for nested_ref in &ag.attribute_groups {
        if let Some(nested_ag) = flat.attribute_groups.get(nested_ref) {
            result.extend(expand_attribute_group(nested_ag, flat));
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use roxmltree::Document;
    use crate::xsd::parser::parse_schema;
    use crate::xsd::types::{ComplexContent, XsdType};

    /// Fixture: 3-level inheritance schema (BaseType → MiddleType → LeafType).
    const THREE_LEVEL_SCHEMA: &str = r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema"
        targetNamespace="urn:test" xmlns:tns="urn:test">
  <xs:complexType name="BaseType">
    <xs:sequence>
      <xs:element name="id" type="xs:string"/>
    </xs:sequence>
  </xs:complexType>
  <xs:complexType name="MiddleType">
    <xs:complexContent>
      <xs:extension base="tns:BaseType">
        <xs:sequence>
          <xs:element name="name" type="xs:string"/>
        </xs:sequence>
      </xs:extension>
    </xs:complexContent>
  </xs:complexType>
  <xs:complexType name="LeafType">
    <xs:complexContent>
      <xs:extension base="tns:MiddleType">
        <xs:sequence>
          <xs:element name="value" type="xs:string"/>
        </xs:sequence>
      </xs:extension>
    </xs:complexContent>
  </xs:complexType>
</xs:schema>"#;

    fn parse_and_resolve(xml: &str) -> Result<TypeRegistry, SchemaError> {
        let doc = Document::parse(xml).expect("valid XML");
        let raw = parse_schema(doc.root_element()).expect("valid schema");
        let mut already_loaded = HashMap::new();
        resolve_schema(raw, &NullSchemaLoader, &mut already_loaded)
    }

    fn get_elements(registry: &TypeRegistry, ns: &str, name: &str) -> Vec<String> {
        let qname = QName::new(ns, name);
        match registry.lookup(&qname) {
            Some(XsdType::Complex(ct)) => {
                match &ct.content {
                    ComplexContent::Sequence(els) => {
                        els.iter()
                           .filter_map(|e| e.name.clone())
                           .collect()
                    }
                    _ => vec![],
                }
            }
            _ => vec![],
        }
    }

    /// THREE-LEVEL FIXTURE TEST: LeafType must have [id, name, value] in order.
    #[test]
    fn three_level_extension_chain_resolves_in_order() {
        let registry = parse_and_resolve(THREE_LEVEL_SCHEMA).expect("resolve ok");

        let leaf_elements = get_elements(&registry, "urn:test", "LeafType");
        assert_eq!(
            leaf_elements,
            vec!["id", "name", "value"],
            "LeafType must have [id, name, value] in ancestor-first order"
        );
    }

    #[test]
    fn base_type_resolves_to_single_element() {
        let registry = parse_and_resolve(THREE_LEVEL_SCHEMA).expect("resolve ok");
        let base_elements = get_elements(&registry, "urn:test", "BaseType");
        assert_eq!(base_elements, vec!["id"]);
    }

    #[test]
    fn middle_type_resolves_to_two_elements() {
        let registry = parse_and_resolve(THREE_LEVEL_SCHEMA).expect("resolve ok");
        let mid_elements = get_elements(&registry, "urn:test", "MiddleType");
        assert_eq!(mid_elements, vec!["id", "name"]);
    }

    /// CYCLE DETECTION: A type that directly extends itself should return CycleDetected.
    #[test]
    fn cycle_detection_returns_err() {
        let xml = r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema"
            targetNamespace="urn:test" xmlns:tns="urn:test">
  <xs:complexType name="SelfRef">
    <xs:complexContent>
      <xs:extension base="tns:SelfRef">
        <xs:sequence/>
      </xs:extension>
    </xs:complexContent>
  </xs:complexType>
</xs:schema>"#;
        let result = parse_and_resolve(xml);
        assert!(
            matches!(result, Err(SchemaError::CycleDetected(_))),
            "Self-referencing type must produce CycleDetected, got: {result:?}"
        );
    }

    /// RESTRICTION: Restricted type uses only its own content model (not base's).
    #[test]
    fn restriction_uses_own_content_not_base() {
        let xml = r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema"
            targetNamespace="urn:test" xmlns:tns="urn:test">
  <xs:complexType name="BaseType">
    <xs:sequence>
      <xs:element name="id" type="xs:string"/>
      <xs:element name="extra" type="xs:string"/>
    </xs:sequence>
  </xs:complexType>
  <xs:complexType name="RestrictedType">
    <xs:complexContent>
      <xs:restriction base="tns:BaseType">
        <xs:sequence>
          <xs:element name="id" type="xs:string"/>
        </xs:sequence>
      </xs:restriction>
    </xs:complexContent>
  </xs:complexType>
</xs:schema>"#;
        let registry = parse_and_resolve(xml).expect("resolve ok");
        let elements = get_elements(&registry, "urn:test", "RestrictedType");
        assert_eq!(elements, vec!["id"], "Restriction should only have 'id', not 'extra'");
    }

    /// DIAMOND IMPORT: A imports B and C; B and C both import D. D loaded once.
    #[test]
    fn diamond_import_loads_schema_once() {
        struct DiamondLoader {
            load_count: std::sync::atomic::AtomicUsize,
        }
        impl SchemaLoader for DiamondLoader {
            fn load(&self, _ns: Option<&str>, location: &str) -> Result<String, SchemaError> {
                self.load_count.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                match location {
                    "B.xsd" => Ok(r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema"
                        targetNamespace="urn:b" xmlns:tns="urn:b">
                      <xs:import namespace="urn:d" schemaLocation="D.xsd"/>
                      <xs:complexType name="BType"><xs:sequence/></xs:complexType>
                    </xs:schema>"#.to_string()),
                    "C.xsd" => Ok(r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema"
                        targetNamespace="urn:c" xmlns:tns="urn:c">
                      <xs:import namespace="urn:d" schemaLocation="D.xsd"/>
                      <xs:complexType name="CType"><xs:sequence/></xs:complexType>
                    </xs:schema>"#.to_string()),
                    "D.xsd" => Ok(r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema"
                        targetNamespace="urn:d">
                      <xs:complexType name="DType"><xs:sequence/></xs:complexType>
                    </xs:schema>"#.to_string()),
                    _ => Err(SchemaError::UnknownRef(location.to_string())),
                }
            }
        }

        let root_xml = r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema"
            targetNamespace="urn:root">
          <xs:import namespace="urn:b" schemaLocation="B.xsd"/>
          <xs:import namespace="urn:c" schemaLocation="C.xsd"/>
        </xs:schema>"#;

        let loader = DiamondLoader { load_count: std::sync::atomic::AtomicUsize::new(0) };
        let doc = Document::parse(root_xml).expect("valid XML");
        let raw = parse_schema(doc.root_element()).expect("valid schema");
        let mut already_loaded = HashMap::new();
        let registry = resolve_schema(raw, &loader, &mut already_loaded).expect("resolve ok");

        // D.xsd should have been loaded exactly once.
        let count = loader.load_count.load(std::sync::atomic::Ordering::SeqCst);
        assert_eq!(count, 3, "Should load B.xsd, C.xsd, D.xsd — D only once. Got: {count}");

        // All types from B, C, D should be in registry.
        assert!(registry.lookup(&QName::new("urn:b", "BType")).is_some());
        assert!(registry.lookup(&QName::new("urn:c", "CType")).is_some());
        assert!(registry.lookup(&QName::new("urn:d", "DType")).is_some());
    }

    /// UNKNOWN REF: Element with type_ref pointing to unknown type should error.
    /// (This is detected at resolution time via UnknownRef.)
    #[test]
    fn empty_schema_produces_empty_registry() {
        let xml = r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema"
            targetNamespace="urn:test">
        </xs:schema>"#;
        let registry = parse_and_resolve(xml).expect("resolve ok");
        assert_eq!(registry.len(), 0);
    }

    /// SIMPLE TYPE: Simple types pass through without modification.
    #[test]
    fn simple_types_pass_through_resolution() {
        let xml = r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema"
            targetNamespace="urn:test">
          <xs:simpleType name="MyEnum">
            <xs:restriction base="xs:string">
              <xs:enumeration value="A"/>
              <xs:enumeration value="B"/>
            </xs:restriction>
          </xs:simpleType>
        </xs:schema>"#;
        let registry = parse_and_resolve(xml).expect("resolve ok");
        let qname = QName::new("urn:test", "MyEnum");
        assert!(
            matches!(registry.lookup(&qname), Some(XsdType::Simple(_))),
            "SimpleType should pass through resolution unchanged"
        );
    }
}
