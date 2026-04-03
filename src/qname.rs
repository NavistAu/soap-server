/// A qualified XML name with an optional namespace URI and a local name.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct QName {
    pub namespace: Option<String>,
    pub local_name: String,
}

impl QName {
    /// Create a QName with an explicit namespace URI.
    pub fn new(namespace: impl Into<String>, local_name: impl Into<String>) -> Self {
        Self {
            namespace: Some(namespace.into()),
            local_name: local_name.into(),
        }
    }

    /// Create a QName with no namespace (local-only).
    pub fn local(local_name: impl Into<String>) -> Self {
        Self {
            namespace: None,
            local_name: local_name.into(),
        }
    }
}

impl std::fmt::Display for QName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.namespace {
            Some(ns) => write!(f, "{{{}}}{}", ns, self.local_name),
            None => write!(f, "{}", self.local_name),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn qname_new_sets_namespace_and_local() {
        let q = QName::new("http://example.com", "Foo");
        assert_eq!(q.namespace, Some("http://example.com".to_string()));
        assert_eq!(q.local_name, "Foo");
    }

    #[test]
    fn qname_local_has_no_namespace() {
        let q = QName::local("Bar");
        assert_eq!(q.namespace, None);
        assert_eq!(q.local_name, "Bar");
    }

    #[test]
    fn qname_display_with_namespace() {
        let q = QName::new("http://example.com", "Foo");
        assert_eq!(q.to_string(), "{http://example.com}Foo");
    }

    #[test]
    fn qname_display_without_namespace() {
        let q = QName::local("Bar");
        assert_eq!(q.to_string(), "Bar");
    }

    #[test]
    fn qname_equality_and_hash() {
        let a = QName::new("http://example.com", "X");
        let b = QName::new("http://example.com", "X");
        let c = QName::local("X");
        assert_eq!(a, b);
        assert_ne!(a, c);
    }
}
