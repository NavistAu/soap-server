//! XML character escaping helpers.
//!
//! `escape_text` and `escape_attr` are intentionally separate even though
//! both currently delegate to `quick_xml::escape::escape` (which handles all
//! five XML special characters).  Keeping them distinct lets callers
//! communicate intent, and leaves room to differentiate behaviour in the
//! future (e.g. skipping `"` / `'` in text-only contexts for smaller output).
//!
//! # Decision: `detail` escaping
//! `SoapFault::detail` is always treated as **plain text** in this codebase.
//! All call sites in soap-server construct detail from internal error messages
//! or static strings; none embed pre-formed XML markup.  Therefore `detail`
//! content is escaped with `escape_text` just like `reason`.  If a future
//! caller needs to embed literal XML in a fault detail element it must
//! construct a `SoapFault` whose `detail` field already contains the correct
//! escaped representation, or it must supply the literal XML via a wrapper
//! element and bypass this helper.

/// Escape a string for use in XML text content (`&`, `<`, `>`, `"`, `'`).
///
/// Uses [`quick_xml::escape::escape`] which covers all five XML special chars,
/// producing `&amp;`, `&lt;`, `&gt;`, `&quot;`, and `&apos;` as needed.
pub fn escape_text(s: &str) -> String {
    quick_xml::escape::escape(s).into_owned()
}

/// Escape a string for use in an XML attribute value (`&`, `<`, `>`, `"`, `'`).
///
/// Identical to [`escape_text`] — both `"` and `'` are escaped so the result
/// is safe inside either single- or double-quoted attribute values.
pub fn escape_attr(s: &str) -> String {
    quick_xml::escape::escape(s).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escape_text_all_special_chars() {
        let input = r#"& < > " '"#;
        let out = escape_text(input);
        assert_eq!(out, "&amp; &lt; &gt; &quot; &apos;");
    }

    #[test]
    fn escape_attr_all_special_chars() {
        let input = r#"& < > " '"#;
        let out = escape_attr(input);
        assert_eq!(out, "&amp; &lt; &gt; &quot; &apos;");
    }

    #[test]
    fn escape_text_plain_string_unchanged() {
        let s = "hello world";
        assert_eq!(escape_text(s), s);
    }

    #[test]
    fn escape_attr_plain_string_unchanged() {
        let s = "hello world";
        assert_eq!(escape_attr(s), s);
    }

    #[test]
    fn escape_text_ampersand_only() {
        assert_eq!(escape_text("Acme & Sons"), "Acme &amp; Sons");
    }

    #[test]
    fn escape_text_angle_brackets() {
        assert_eq!(escape_text("<tag>"), "&lt;tag&gt;");
    }

    #[test]
    fn escape_attr_double_quote() {
        assert_eq!(escape_attr(r#"say "hi""#), "say &quot;hi&quot;");
    }

    #[test]
    fn escape_attr_single_quote() {
        assert_eq!(escape_attr("it's"), "it&apos;s");
    }
}
