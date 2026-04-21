// XSD element definitions — XsdElement, XsdAttribute, AttributeGroup, Group, Any
use crate::qname::QName;
// Note: XsdType is defined in types.rs; use Box<crate::xsd::types::XsdType> for inline types

/// An XSD element declaration.
#[derive(Debug, Clone)]
pub struct XsdElement {
    pub name: Option<String>,
    pub type_ref: Option<QName>,
    pub inline_type: Option<Box<crate::xsd::types::XsdType>>,
    pub min_occurs: u64,
    pub max_occurs: MaxOccurs,
    pub nillable: bool,
    pub default: Option<String>,
    pub fixed: Option<String>,
    pub ref_attr: Option<QName>,
}

impl Default for XsdElement {
    fn default() -> Self {
        Self {
            name: None,
            type_ref: None,
            inline_type: None,
            min_occurs: 1,
            max_occurs: MaxOccurs::Bounded(1),
            nillable: false,
            default: None,
            fixed: None,
            ref_attr: None,
        }
    }
}

/// The maxOccurs attribute value.
#[derive(Debug, Clone, PartialEq)]
pub enum MaxOccurs {
    Bounded(u64),
    Unbounded,
}

/// An XSD attribute declaration.
#[derive(Debug, Clone)]
pub struct XsdAttribute {
    pub name: Option<String>,
    pub type_ref: Option<QName>,
    pub use_attr: AttributeUse,
    pub default: Option<String>,
    pub fixed: Option<String>,
    pub ref_attr: Option<QName>,
}

/// The use attribute for XSD attributes.
#[derive(Debug, Clone, PartialEq)]
#[derive(Default)]
pub enum AttributeUse {
    Required,
    #[default]
    Optional,
    Prohibited,
}


/// A named group of attributes (xs:attributeGroup).
#[derive(Debug, Clone)]
pub struct AttributeGroup {
    pub name: String,
    pub attributes: Vec<XsdAttribute>,
    pub attribute_groups: Vec<QName>,
}

/// A named model group (xs:group).
#[derive(Debug, Clone)]
pub struct Group {
    pub name: String,
    pub content: GroupContent,
}

/// The content model of a named group.
#[derive(Debug, Clone)]
pub enum GroupContent {
    Sequence(Vec<XsdElement>),
    All(Vec<XsdElement>),
    Choice(Vec<XsdElement>),
}

/// An xs:any wildcard element.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct AnyElement {
    pub namespace: AnyNamespace,
    pub process_contents: ProcessContents,
    pub min_occurs: u64,
    pub max_occurs: MaxOccurs,
}

/// The namespace attribute for xs:any / xs:anyAttribute.
#[derive(Debug, Clone, PartialEq)]
pub enum AnyNamespace {
    Any,
    Other,
    List(Vec<String>),
}

/// The processContents attribute for xs:any / xs:anyAttribute.
#[derive(Debug, Clone, PartialEq)]
pub enum ProcessContents {
    Strict,
    Lax,
    Skip,
}

/// An xs:anyAttribute wildcard attribute.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct AnyAttribute {
    pub namespace: AnyNamespace,
    pub process_contents: ProcessContents,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::qname::QName;

    #[test]
    fn xsd_element_default_min_occurs_is_1() {
        let el = XsdElement::default();
        assert_eq!(el.min_occurs, 1);
    }

    #[test]
    fn xsd_element_default_max_occurs_is_bounded_1() {
        let el = XsdElement::default();
        assert_eq!(el.max_occurs, MaxOccurs::Bounded(1));
    }

    #[test]
    fn max_occurs_unbounded_variant() {
        let mo = MaxOccurs::Unbounded;
        assert_eq!(mo, MaxOccurs::Unbounded);
    }

    #[test]
    fn attribute_use_default_is_optional() {
        assert_eq!(AttributeUse::default(), AttributeUse::Optional);
    }

    #[test]
    fn xsd_attribute_with_ref() {
        let attr = XsdAttribute {
            name: None,
            type_ref: None,
            use_attr: AttributeUse::Required,
            default: None,
            fixed: None,
            ref_attr: Some(QName::new("http://example.com", "myAttr")),
        };
        assert!(attr.ref_attr.is_some());
        assert_eq!(attr.use_attr, AttributeUse::Required);
    }

    #[test]
    fn attribute_group_holds_attributes_and_refs() {
        let ag = AttributeGroup {
            name: "MyGroup".to_string(),
            attributes: vec![XsdAttribute {
                name: Some("id".to_string()),
                type_ref: Some(QName::local("ID")),
                use_attr: AttributeUse::Optional,
                default: None,
                fixed: None,
                ref_attr: None,
            }],
            attribute_groups: vec![QName::local("OtherGroup")],
        };
        assert_eq!(ag.attributes.len(), 1);
        assert_eq!(ag.attribute_groups.len(), 1);
    }

    #[test]
    fn group_content_choice_variant() {
        let g = Group {
            name: "MyGroup".to_string(),
            content: GroupContent::Choice(vec![]),
        };
        assert!(matches!(g.content, GroupContent::Choice(_)));
    }

    #[test]
    fn any_element_any_namespace() {
        let any = AnyElement {
            namespace: AnyNamespace::Any,
            process_contents: ProcessContents::Lax,
            min_occurs: 0,
            max_occurs: MaxOccurs::Unbounded,
        };
        assert_eq!(any.namespace, AnyNamespace::Any);
        assert_eq!(any.process_contents, ProcessContents::Lax);
    }

    #[test]
    fn any_namespace_list_variant() {
        let ns = AnyNamespace::List(vec![
            "http://example.com".to_string(),
            "http://other.com".to_string(),
        ]);
        if let AnyNamespace::List(list) = &ns {
            assert_eq!(list.len(), 2);
        } else {
            panic!("Expected List variant");
        }
    }
}
