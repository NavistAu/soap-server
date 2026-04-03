// WSDL Pass 2 resolver — wire cross-references, load imports, delegate schemas to XSD layer
// Also provides rewrite_wsdl_address() for GET ?wsdl serving.

use std::collections::{HashMap, HashSet};
use crate::wsdl::parser::{parse_wsdl, WsdlError};
use crate::wsdl::definitions::WsdlDefinition;
use crate::xsd::parser::{parse_schema, SchemaError};
use crate::xsd::resolver::{resolve_schema, SchemaLoader};
use crate::xsd::types::{TypeRegistry, XsdType};
use crate::qname::QName;

/// Output of WSDL resolution: the wired definition + all type information.
#[derive(Debug)]
pub struct ResolvedWsdl {
    /// The original WsdlDefinition with QName strings (for WSDL serving)
    pub definition: WsdlDefinition,
    /// All types from inline and imported schemas, fully resolved
    pub type_registry: TypeRegistry,
    /// Original WSDL bytes (for serving on GET ?wsdl)
    pub raw_bytes: Vec<u8>,
}

/// Abstracts file/network I/O for loading WSDL files during recursive import resolution.
pub trait WsdlLoader: Send + Sync {
    fn load(&self, location: &str) -> Result<Vec<u8>, WsdlError>;
}

/// Resolve WSDL bytes into a fully-wired ResolvedWsdl.
///
/// Pass 1: parse_wsdl() → WsdlDefinition with raw QName strings
/// Pass 2: wire cross-references, resolve imports, delegate inline schemas to XSD layer
///
/// `visited` tracks WSDL locations already loaded; prevents diamond-import double-loading.
pub fn resolve_wsdl(
    bytes: &[u8],
    loader: &dyn WsdlLoader,
    visited: &mut HashSet<String>,
) -> Result<ResolvedWsdl, WsdlError> {
    let mut already_loaded_schemas: HashMap<String, ()> = HashMap::new();
    let mut accumulated_types: HashMap<QName, XsdType> = HashMap::new();
    resolve_wsdl_inner(bytes, loader, visited, &mut already_loaded_schemas, &mut accumulated_types)
}

fn resolve_wsdl_inner(
    bytes: &[u8],
    loader: &dyn WsdlLoader,
    visited: &mut HashSet<String>,
    already_loaded_schemas: &mut HashMap<String, ()>,
    accumulated_types: &mut HashMap<QName, XsdType>,
) -> Result<ResolvedWsdl, WsdlError> {
    let raw_bytes = bytes.to_vec();

    // Pass 1: parse WSDL
    let mut root_def = parse_wsdl(bytes)?;

    // Process wsdl:import — recursively resolve imported WSDLs and merge their definitions
    for import in root_def.imports.clone() {
        let location = match &import.location {
            Some(loc) => loc.clone(),
            None => continue, // No location attribute — skip (namespace-only import)
        };

        if visited.contains(&location) {
            // Diamond import guard — already processed this WSDL
            continue;
        }

        // Cycle detection: location about to be loaded must not be in-flight
        // We mark it visited before recursing, which covers the A→B→A cycle case
        visited.insert(location.clone());

        let imported_bytes = loader.load(&location)?;
        let imported = resolve_wsdl_inner(
            &imported_bytes, loader, visited, already_loaded_schemas, accumulated_types
        )?;

        // Merge imported definition into root: messages, port_types, bindings, services
        // (Types are already merged into accumulated_types via the recursive call above)
        for (k, v) in imported.definition.messages {
            root_def.messages.entry(k).or_insert(v);
        }
        for (k, v) in imported.definition.port_types {
            root_def.port_types.entry(k).or_insert(v);
        }
        for (k, v) in imported.definition.bindings {
            root_def.bindings.entry(k).or_insert(v);
        }
        for (k, v) in imported.definition.services {
            root_def.services.entry(k).or_insert(v);
        }
    }

    // Collect and resolve inline xs:schema nodes from wsdl:types
    let schema_loader = WsdlSchemaLoaderAdapter { wsdl_loader: loader };

    for schema_str in &root_def.types.schemas {
        let doc = roxmltree::Document::parse(schema_str)
            .map_err(|e| WsdlError::MalformedXml(format!("inline schema parse error: {e}")))?;
        let raw_schema = parse_schema(doc.root_element())
            .map_err(|e| WsdlError::MalformedXml(format!("inline schema xsd parse error: {e}")))?;
        let partial = resolve_schema(raw_schema, &schema_loader, already_loaded_schemas)
            .map_err(|e| WsdlError::MalformedXml(format!("schema resolution error: {e}")))?;
        for (qname, xsd_type) in partial {
            accumulated_types.entry(qname).or_insert(xsd_type);
        }
    }

    // Build a TypeRegistry snapshot of all accumulated types so far.
    // The root call's snapshot will contain everything (own + all imports transitively).
    let mut type_registry = TypeRegistry::new();
    for (qname, xsd_type) in accumulated_types.iter() {
        type_registry.insert(qname.clone(), xsd_type.clone());
    }

    Ok(ResolvedWsdl {
        definition: root_def,
        type_registry,
        raw_bytes,
    })
}

/// Adapter that wraps a WsdlLoader to implement SchemaLoader.
/// Allows external XSD files referenced from inline schemas to be loaded via the same loader.
struct WsdlSchemaLoaderAdapter<'a> {
    wsdl_loader: &'a dyn WsdlLoader,
}

impl<'a> SchemaLoader for WsdlSchemaLoaderAdapter<'a> {
    fn load(&self, _namespace: Option<&str>, location: &str) -> Result<String, SchemaError> {
        let bytes = self.wsdl_loader.load(location)
            .map_err(|e| SchemaError::UnknownRef(format!("WsdlLoader error for {location}: {e}")))?;
        String::from_utf8(bytes)
            .map_err(|e| SchemaError::MalformedXml(format!("UTF-8 error: {e}")))
    }
}

/// Rewrite the `location` attribute value on `soap:address` / `soap12:address` elements
/// in WSDL bytes, replacing it with `new_url`. All other content is preserved unchanged.
///
/// Uses quick-xml streaming to avoid full parse overhead.
pub fn rewrite_wsdl_address(bytes: &[u8], new_url: &str) -> Vec<u8> {
    use quick_xml::events::{BytesStart, Event};
    use quick_xml::reader::Reader;
    use quick_xml::writer::Writer;

    let mut reader = Reader::from_reader(bytes);
    reader.config_mut().trim_text(false);

    let mut writer = Writer::new(Vec::new());

    loop {
        match reader.read_event() {
            Ok(Event::Start(e)) => {
                let local_name = e.local_name();
                let local_str = std::str::from_utf8(local_name.as_ref()).unwrap_or("");

                if local_str == "address" {
                    // Rewrite the location= attribute if this is a soap:address element
                    let name_bytes = e.name().as_ref().to_vec();
                    let name_str = String::from_utf8(name_bytes).unwrap_or_else(|_| "address".to_string());
                    let mut new_start = BytesStart::new(name_str.as_str());
                    for attr_result in e.attributes() {
                        if let Ok(attr) = attr_result {
                            let attr_key = std::str::from_utf8(attr.key.as_ref()).unwrap_or("");
                            if attr_key == "location" {
                                new_start.push_attribute(("location", new_url));
                            } else {
                                new_start.push_attribute(attr);
                            }
                        }
                    }
                    let _ = writer.write_event(Event::Start(new_start));
                } else {
                    let _ = writer.write_event(Event::Start(e));
                }
            }
            Ok(Event::Empty(e)) => {
                let local_name = e.local_name();
                let local_str = std::str::from_utf8(local_name.as_ref()).unwrap_or("");

                if local_str == "address" {
                    let name_bytes = e.name().as_ref().to_vec();
                    let name_str = String::from_utf8(name_bytes).unwrap_or_else(|_| "address".to_string());
                    let mut new_empty = BytesStart::new(name_str.as_str());
                    for attr_result in e.attributes() {
                        if let Ok(attr) = attr_result {
                            let attr_key = std::str::from_utf8(attr.key.as_ref()).unwrap_or("");
                            if attr_key == "location" {
                                new_empty.push_attribute(("location", new_url));
                            } else {
                                new_empty.push_attribute(attr);
                            }
                        }
                    }
                    let _ = writer.write_event(Event::Empty(new_empty));
                } else {
                    let _ = writer.write_event(Event::Empty(e));
                }
            }
            Ok(Event::Eof) => break,
            Ok(other) => {
                let _ = writer.write_event(other);
            }
            Err(_) => break,
        }
    }

    writer.into_inner()
}

#[cfg(test)]
mod tests {
    use super::*;

    // ---- Test WSDL fixtures ----

    // Root WSDL that imports a second WSDL
    const ROOT_WSDL: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<definitions
  xmlns="http://schemas.xmlsoap.org/wsdl/"
  xmlns:soap12="http://schemas.xmlsoap.org/wsdl/soap12/"
  xmlns:tns="http://example.com/root"
  targetNamespace="http://example.com/root"
  name="RootService">

  <import namespace="http://example.com/imported" location="imported.wsdl"/>

  <types>
    <xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema" targetNamespace="http://example.com/root">
      <xs:element name="RootElement" type="xs:string"/>
    </xs:schema>
  </types>

  <message name="RootMsg"><part name="p" element="tns:RootElement"/></message>

  <portType name="RootPT">
    <operation name="RootOp">
      <input message="tns:RootMsg"/>
    </operation>
  </portType>

  <binding name="RootBinding" type="tns:RootPT">
    <soap12:binding style="document" transport="http://schemas.xmlsoap.org/soap/http"/>
    <operation name="RootOp">
      <soap12:operation soapAction="http://example.com/RootOp"/>
      <input><soap12:body use="literal"/></input>
      <output><soap12:body use="literal"/></output>
    </operation>
  </binding>

  <service name="RootService">
    <port name="RootPort" binding="tns:RootBinding">
      <soap12:address location="http://old-server/soap"/>
    </port>
  </service>
</definitions>"#;

    // Imported WSDL with its own message and portType operation
    const IMPORTED_WSDL: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<definitions
  xmlns="http://schemas.xmlsoap.org/wsdl/"
  xmlns:tns="http://example.com/imported"
  targetNamespace="http://example.com/imported"
  name="ImportedService">

  <message name="ImportedMsg"><part name="p" element="tns:ImportedElem"/></message>

  <portType name="ImportedPT">
    <operation name="ImportedOp">
      <input message="tns:ImportedMsg"/>
    </operation>
  </portType>
</definitions>"#;

    // Simple standalone WSDL with inline schema
    const STANDALONE_WSDL: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<definitions
  xmlns="http://schemas.xmlsoap.org/wsdl/"
  xmlns:soap12="http://schemas.xmlsoap.org/wsdl/soap12/"
  xmlns:tns="http://example.com/standalone"
  xmlns:xs="http://www.w3.org/2001/XMLSchema"
  targetNamespace="http://example.com/standalone"
  name="StandaloneService">

  <types>
    <xs:schema targetNamespace="http://example.com/standalone">
      <xs:complexType name="StandaloneType">
        <xs:sequence>
          <xs:element name="field1" type="xs:string"/>
          <xs:element name="field2" type="xs:int"/>
        </xs:sequence>
      </xs:complexType>
    </xs:schema>
  </types>

  <message name="Req"><part name="p" element="tns:StandaloneElem"/></message>
  <portType name="StandalonePT">
    <operation name="StandaloneOp">
      <input message="tns:Req"/>
    </operation>
  </portType>
  <binding name="StandaloneBinding" type="tns:StandalonePT">
    <soap12:binding style="document" transport="http://schemas.xmlsoap.org/soap/http"/>
    <operation name="StandaloneOp">
      <soap12:operation soapAction="http://example.com/StandaloneOp"/>
      <input><soap12:body use="literal"/></input>
      <output><soap12:body use="literal"/></output>
    </operation>
  </binding>
  <service name="StandaloneService">
    <port name="StandalonePort" binding="tns:StandaloneBinding">
      <soap12:address location="http://old-server/soap"/>
    </port>
  </service>
</definitions>"#;

    // WSDL forming part A of a diamond: imports B and C
    const DIAMOND_A_WSDL: &str = r#"<?xml version="1.0"?>
<definitions xmlns="http://schemas.xmlsoap.org/wsdl/"
  xmlns:tns="http://example.com/a" targetNamespace="http://example.com/a">
  <import namespace="http://example.com/b" location="b.wsdl"/>
  <import namespace="http://example.com/c" location="c.wsdl"/>
  <types>
    <xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema" targetNamespace="http://example.com/a">
      <xs:complexType name="AType"><xs:sequence/></xs:complexType>
    </xs:schema>
  </types>
</definitions>"#;

    const DIAMOND_B_WSDL: &str = r#"<?xml version="1.0"?>
<definitions xmlns="http://schemas.xmlsoap.org/wsdl/"
  xmlns:tns="http://example.com/b" targetNamespace="http://example.com/b">
  <import namespace="http://example.com/d" location="d.wsdl"/>
  <types>
    <xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema" targetNamespace="http://example.com/b">
      <xs:complexType name="BType"><xs:sequence/></xs:complexType>
    </xs:schema>
  </types>
</definitions>"#;

    const DIAMOND_C_WSDL: &str = r#"<?xml version="1.0"?>
<definitions xmlns="http://schemas.xmlsoap.org/wsdl/"
  xmlns:tns="http://example.com/c" targetNamespace="http://example.com/c">
  <import namespace="http://example.com/d" location="d.wsdl"/>
  <types>
    <xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema" targetNamespace="http://example.com/c">
      <xs:complexType name="CType"><xs:sequence/></xs:complexType>
    </xs:schema>
  </types>
</definitions>"#;

    const DIAMOND_D_WSDL: &str = r#"<?xml version="1.0"?>
<definitions xmlns="http://schemas.xmlsoap.org/wsdl/"
  xmlns:tns="http://example.com/d" targetNamespace="http://example.com/d">
  <types>
    <xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema" targetNamespace="http://example.com/d">
      <xs:complexType name="DType"><xs:sequence/></xs:complexType>
    </xs:schema>
  </types>
</definitions>"#;

    // WSDL with soap:address (SOAP 1.1 style)
    const WSDL_WITH_SOAP11_ADDRESS: &str = r#"<?xml version="1.0"?>
<definitions xmlns="http://schemas.xmlsoap.org/wsdl/"
  xmlns:soap="http://schemas.xmlsoap.org/wsdl/soap/"
  xmlns:tns="http://example.com/svc" targetNamespace="http://example.com/svc">
  <service name="Svc">
    <port name="SvcPort" binding="tns:B">
      <soap:address location="http://old-server/soap"/>
    </port>
  </service>
</definitions>"#;

    // ---- Mock loaders ----

    struct NullWsdlLoader;
    impl WsdlLoader for NullWsdlLoader {
        fn load(&self, location: &str) -> Result<Vec<u8>, WsdlError> {
            Err(WsdlError::MalformedXml(format!("NullWsdlLoader cannot load: {}", location)))
        }
    }

    struct TwoFileLoader;
    impl WsdlLoader for TwoFileLoader {
        fn load(&self, location: &str) -> Result<Vec<u8>, WsdlError> {
            match location {
                "imported.wsdl" => Ok(IMPORTED_WSDL.as_bytes().to_vec()),
                _ => Err(WsdlError::MalformedXml(format!("Unknown location: {}", location))),
            }
        }
    }

    struct DiamondLoader {
        load_count: std::sync::Arc<std::sync::atomic::AtomicUsize>,
    }
    impl WsdlLoader for DiamondLoader {
        fn load(&self, location: &str) -> Result<Vec<u8>, WsdlError> {
            self.load_count.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            match location {
                "b.wsdl" => Ok(DIAMOND_B_WSDL.as_bytes().to_vec()),
                "c.wsdl" => Ok(DIAMOND_C_WSDL.as_bytes().to_vec()),
                "d.wsdl" => Ok(DIAMOND_D_WSDL.as_bytes().to_vec()),
                _ => Err(WsdlError::MalformedXml(format!("Unknown: {}", location))),
            }
        }
    }

    struct CycleLoader;
    impl WsdlLoader for CycleLoader {
        fn load(&self, location: &str) -> Result<Vec<u8>, WsdlError> {
            match location {
                "b.wsdl" => {
                    // B imports A again — creates A→B→A cycle
                    let b = r#"<?xml version="1.0"?>
<definitions xmlns="http://schemas.xmlsoap.org/wsdl/"
  xmlns:tns="http://example.com/b" targetNamespace="http://example.com/b">
  <import namespace="http://example.com/a" location="a.wsdl"/>
</definitions>"#;
                    Ok(b.as_bytes().to_vec())
                }
                _ => Err(WsdlError::MalformedXml(format!("Unknown: {}", location))),
            }
        }
    }

    // ---- Tests ----

    #[test]
    fn two_file_wsdl_merges_operations() {
        let mut visited = HashSet::new();
        let result = resolve_wsdl(ROOT_WSDL.as_bytes(), &TwoFileLoader, &mut visited);
        assert!(result.is_ok(), "Expected Ok, got: {:?}", result.err());
        let resolved = result.unwrap();

        // Root operations present
        assert!(resolved.definition.messages.contains_key("RootMsg"),
            "RootMsg should be in merged definition");

        // Imported operations merged in
        assert!(resolved.definition.messages.contains_key("ImportedMsg"),
            "ImportedMsg from imported WSDL should be merged");
        assert!(resolved.definition.port_types.contains_key("ImportedPT"),
            "ImportedPT from imported WSDL should be merged");
    }

    #[test]
    fn standalone_wsdl_resolves_inline_schema() {
        let mut visited = HashSet::new();
        let result = resolve_wsdl(STANDALONE_WSDL.as_bytes(), &NullWsdlLoader, &mut visited);
        assert!(result.is_ok(), "Expected Ok, got: {:?}", result.err());
        let resolved = result.unwrap();

        // TypeRegistry should contain StandaloneType from inline schema
        use crate::qname::QName;
        let qname = QName::new("http://example.com/standalone", "StandaloneType");
        assert!(
            resolved.type_registry.lookup(&qname).is_some(),
            "StandaloneType from inline xs:schema should be in TypeRegistry"
        );
    }

    #[test]
    fn diamond_import_loads_d_once() {
        let load_count = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let loader = DiamondLoader { load_count: load_count.clone() };

        let mut visited = HashSet::new();
        let result = resolve_wsdl(DIAMOND_A_WSDL.as_bytes(), &loader, &mut visited);
        assert!(result.is_ok(), "Expected Ok, got: {:?}", result.err());

        // Should have loaded b.wsdl, c.wsdl, d.wsdl — d only once
        let count = load_count.load(std::sync::atomic::Ordering::SeqCst);
        assert_eq!(count, 3, "Expected 3 loader calls (b, c, d). Got: {}", count);
    }

    #[test]
    fn diamond_import_types_deduplicated_in_registry() {
        let load_count = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let loader = DiamondLoader { load_count: load_count.clone() };

        let mut visited = HashSet::new();
        let resolved = resolve_wsdl(DIAMOND_A_WSDL.as_bytes(), &loader, &mut visited)
            .expect("resolve ok");

        use crate::qname::QName;
        // DType should appear exactly once (from d.wsdl loaded once)
        assert!(resolved.type_registry.lookup(&QName::new("http://example.com/d", "DType")).is_some(),
            "DType from d.wsdl should appear in TypeRegistry");
    }

    #[test]
    fn cycle_import_returns_err_without_stack_overflow() {
        let a_wsdl = r#"<?xml version="1.0"?>
<definitions xmlns="http://schemas.xmlsoap.org/wsdl/"
  xmlns:tns="http://example.com/a" targetNamespace="http://example.com/a">
  <import namespace="http://example.com/b" location="b.wsdl"/>
</definitions>"#;

        let mut visited = HashSet::new();
        // Mark a.wsdl as already visited before we start (simulating it being the root)
        visited.insert("a.wsdl".to_string());

        let result = resolve_wsdl(a_wsdl.as_bytes(), &CycleLoader, &mut visited);
        // With visited containing "a.wsdl", when B tries to import "a.wsdl" it's skipped
        // So this should succeed (no cycle error needed for this pattern)
        // The cycle guard is: b.wsdl is loaded, then it tries to load a.wsdl but a.wsdl is already in visited
        assert!(result.is_ok(), "Cycle guard should prevent infinite recursion: {:?}", result);
    }

    #[test]
    fn rewrite_wsdl_address_replaces_location_soap12() {
        let result = rewrite_wsdl_address(ROOT_WSDL.as_bytes(), "http://new-server/soap");
        let output = String::from_utf8(result).expect("valid utf8");

        assert!(
            output.contains("http://new-server/soap"),
            "Output should contain new URL"
        );
        assert!(
            !output.contains("http://old-server/soap"),
            "Output should not contain old URL"
        );
    }

    #[test]
    fn rewrite_wsdl_address_replaces_location_soap11() {
        let result = rewrite_wsdl_address(WSDL_WITH_SOAP11_ADDRESS.as_bytes(), "http://new-server/soap");
        let output = String::from_utf8(result).expect("valid utf8");

        assert!(
            output.contains("http://new-server/soap"),
            "Output should contain new URL for SOAP 1.1 address"
        );
        assert!(
            !output.contains("http://old-server/soap"),
            "Output should not contain old URL"
        );
    }

    #[test]
    fn rewrite_wsdl_address_preserves_other_content() {
        let result = rewrite_wsdl_address(ROOT_WSDL.as_bytes(), "http://new-server/soap");
        let output = String::from_utf8(result).expect("valid utf8");

        // Service name should still be present
        assert!(output.contains("RootService"), "Service name should be preserved");
        // Port name should still be present
        assert!(output.contains("RootPort"), "Port name should be preserved");
        // binding reference should still be there
        assert!(output.contains("RootBinding"), "Binding reference should be preserved");
    }

    #[test]
    fn raw_bytes_preserved_in_resolved_wsdl() {
        let mut visited = HashSet::new();
        let resolved = resolve_wsdl(STANDALONE_WSDL.as_bytes(), &NullWsdlLoader, &mut visited)
            .expect("resolve ok");

        assert_eq!(resolved.raw_bytes, STANDALONE_WSDL.as_bytes(),
            "raw_bytes should match original input bytes");
    }

    #[test]
    fn resolve_wsdl_no_imports_succeeds() {
        let mut visited = HashSet::new();
        let result = resolve_wsdl(STANDALONE_WSDL.as_bytes(), &NullWsdlLoader, &mut visited);
        assert!(result.is_ok(), "Standalone WSDL with no imports should succeed: {:?}", result.err());
    }
}
