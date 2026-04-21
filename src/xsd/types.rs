// XSD type definitions — ComplexType, SimpleType, Restriction, etc.
use std::collections::HashMap;
use crate::qname::QName;
use crate::xsd::elements::{XsdElement, XsdAttribute};

/// Top-level XSD type — either complex or simple.
#[derive(Debug, Clone)]
pub enum XsdType {
    Complex(Box<ComplexType>),
    Simple(Box<SimpleType>),
}

/// An XSD complexType definition.
#[derive(Debug, Clone)]
pub struct ComplexType {
    pub name: Option<String>,
    pub content: ComplexContent,
    pub attributes: Vec<XsdAttribute>,
}

/// The content model of a complexType.
#[derive(Debug, Clone)]
pub enum ComplexContent {
    Sequence(Vec<XsdElement>),
    All(Vec<XsdElement>),
    Choice(Vec<XsdElement>),
    Empty,
    SimpleContent(SimpleContentDef),
    ComplexExtension {
        base: QName,
        content: Box<ComplexContent>,
    },
    ComplexRestriction {
        base: QName,
        content: Box<ComplexContent>,
    },
}

/// An XSD simpleType definition.
#[derive(Debug, Clone)]
pub struct SimpleType {
    pub name: Option<String>,
    pub restriction: Option<Restriction>,
    pub list: Option<ListDef>,
    pub union: Option<UnionDef>,
}

/// An XSD restriction (facets applied to a base type).
#[derive(Debug, Clone)]
pub struct Restriction {
    pub base: QName,
    pub enumeration: Vec<String>,
    pub min_inclusive: Option<String>,
    pub max_inclusive: Option<String>,
    pub min_exclusive: Option<String>,
    pub max_exclusive: Option<String>,
    pub min_length: Option<u64>,
    pub max_length: Option<u64>,
    pub length: Option<u64>,
    pub pattern: Option<String>,
    pub whitespace: Option<WhitespaceHandling>,
    pub total_digits: Option<u64>,
    pub fraction_digits: Option<u64>,
}

/// XSD whiteSpace facet values.
#[derive(Debug, Clone, PartialEq)]
pub enum WhitespaceHandling {
    Preserve,
    Replace,
    Collapse,
}

/// An XSD list type (values are whitespace-separated).
#[derive(Debug, Clone)]
pub struct ListDef {
    pub item_type: QName,
}

/// An XSD union type (value can match any of the member types).
#[derive(Debug, Clone)]
pub struct UnionDef {
    pub member_types: Vec<QName>,
}

/// A simpleContent definition (complexType with simple base + attributes).
#[derive(Debug, Clone)]
pub struct SimpleContentDef {
    pub base: QName,
    pub attributes: Vec<XsdAttribute>,
}

/// Registry of all parsed XSD types, keyed by QName.
#[derive(Debug, Clone, Default)]
pub struct TypeRegistry {
    types: HashMap<QName, XsdType>,
}

impl TypeRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert(&mut self, name: QName, ty: XsdType) {
        self.types.insert(name, ty);
    }

    pub fn lookup(&self, name: &QName) -> Option<&XsdType> {
        self.types.get(name)
    }

    pub fn len(&self) -> usize {
        self.types.len()
    }

    pub fn is_empty(&self) -> bool {
        self.types.is_empty()
    }
}

impl IntoIterator for TypeRegistry {
    type Item = (QName, XsdType);
    type IntoIter = std::collections::hash_map::IntoIter<QName, XsdType>;

    fn into_iter(self) -> Self::IntoIter {
        self.types.into_iter()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::qname::QName;
    use crate::xsd::elements::{AttributeUse, XsdAttribute};

    #[test]
    fn complex_type_sequence_holds_elements() {
        let ct = ComplexType {
            name: Some("MyType".to_string()),
            content: ComplexContent::Sequence(vec![]),
            attributes: vec![],
        };
        assert!(matches!(ct.content, ComplexContent::Sequence(_)));
    }

    #[test]
    fn complex_extension_holds_base_and_content() {
        let ext = ComplexContent::ComplexExtension {
            base: QName::new("http://example.com", "BaseType"),
            content: Box::new(ComplexContent::Empty),
        };
        if let ComplexContent::ComplexExtension { base, .. } = &ext {
            assert_eq!(base.local_name, "BaseType");
        } else {
            panic!("Expected ComplexExtension");
        }
    }

    #[test]
    fn simple_type_with_restriction() {
        let st = SimpleType {
            name: Some("MyEnum".to_string()),
            restriction: Some(Restriction {
                base: QName::local("string"),
                enumeration: vec!["A".to_string(), "B".to_string()],
                min_inclusive: None,
                max_inclusive: None,
                min_exclusive: None,
                max_exclusive: None,
                min_length: None,
                max_length: None,
                length: None,
                pattern: None,
                whitespace: None,
                total_digits: None,
                fraction_digits: None,
            }),
            list: None,
            union: None,
        };
        let r = st.restriction.unwrap();
        assert_eq!(r.enumeration, vec!["A", "B"]);
    }

    #[test]
    fn type_registry_insert_and_lookup() {
        let mut reg = TypeRegistry::new();
        let qname = QName::local("Foo");
        let ty = XsdType::Simple(Box::new(SimpleType {
            name: Some("Foo".to_string()),
            restriction: None,
            list: None,
            union: None,
        }));
        reg.insert(qname.clone(), ty);
        assert!(reg.lookup(&qname).is_some());
        assert_eq!(reg.len(), 1);
    }

    #[test]
    fn type_registry_lookup_missing_returns_none() {
        let reg = TypeRegistry::new();
        assert!(reg.lookup(&QName::local("NotThere")).is_none());
    }

    #[test]
    fn complex_type_with_simple_content() {
        let ct = ComplexType {
            name: Some("WithSimpleContent".to_string()),
            content: ComplexContent::SimpleContent(SimpleContentDef {
                base: QName::local("string"),
                attributes: vec![XsdAttribute {
                    name: Some("lang".to_string()),
                    type_ref: Some(QName::local("language")),
                    use_attr: AttributeUse::Optional,
                    default: None,
                    fixed: None,
                    ref_attr: None,
                }],
            }),
            attributes: vec![],
        };
        if let ComplexContent::SimpleContent(scd) = &ct.content {
            assert_eq!(scd.attributes.len(), 1);
        } else {
            panic!("Expected SimpleContent");
        }
    }
}
