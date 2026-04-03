// WS-Security UsernameToken parsing and verification
use sha1::{Sha1, Digest};
use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine;
use quick_xml::events::Event;
use quick_xml::Reader;
use chrono::{DateTime, Utc};
use crate::fault::SoapFault;
use crate::wssec::nonce_cache::RotatingNonceCache;
use crate::wssec::timestamp::{parse_created, check_freshness};

// WS-Security namespaces
const WSSE_NS: &str = "http://docs.oasis-open.org/wss/2004/01/oasis-200401-wss-wssecurity-secext-1.0.xsd";
const WSU_NS: &str = "http://docs.oasis-open.org/wss/2004/01/oasis-200401-wss-wssecurity-utility-1.0.xsd";
const PASSWORD_DIGEST_TYPE: &str = "http://docs.oasis-open.org/wss/2004/01/oasis-200401-wss-username-token-profile-1.0#PasswordDigest";
const PASSWORD_TEXT_TYPE: &str = "http://docs.oasis-open.org/wss/2004/01/oasis-200401-wss-username-token-profile-1.0#PasswordText";

/// The type of password in a UsernameToken.
#[derive(Debug, Clone, PartialEq)]
pub enum PasswordType {
    Digest,
    Text,
}

/// A parsed WS-Security UsernameToken.
#[derive(Debug, Clone)]
pub struct UsernameToken {
    pub username: String,
    pub password: String,
    pub password_type: PasswordType,
    pub nonce: Option<String>,
    pub created: Option<String>,
}

/// Compute PasswordDigest per OASIS WS-Security UsernameToken Profile 1.1 spec:
/// digest = Base64(SHA-1(Base64Decode(Nonce) ++ Created_UTF8 ++ Password_UTF8))
pub fn compute_digest(nonce_b64: &str, created: &str, password: &str) -> Result<String, SoapFault> {
    let nonce_bytes = BASE64.decode(nonce_b64)
        .map_err(|_| SoapFault::sender("Invalid nonce encoding"))?;
    let mut hasher = Sha1::new();
    hasher.update(&nonce_bytes);
    hasher.update(created.as_bytes());
    hasher.update(password.as_bytes());
    Ok(BASE64.encode(hasher.finalize()))
}

/// Parse a WS-Security UsernameToken from XML bytes (the wsse:Security header content).
pub fn parse_username_token(xml_bytes: &[u8]) -> Result<UsernameToken, SoapFault> {
    let mut reader = Reader::from_reader(xml_bytes);
    reader.config_mut().trim_text(true);

    let mut username: Option<String> = None;
    let mut password: Option<String> = None;
    let mut password_type = PasswordType::Digest;
    let mut nonce: Option<String> = None;
    let mut created: Option<String> = None;

    // Track whether we're inside wsse:UsernameToken
    let mut in_username_token = false;
    let mut in_username_elem = false;
    let mut in_password_elem = false;
    let mut in_nonce_elem = false;
    let mut in_created_elem = false;
    let mut found_token = false;

    let mut buf = Vec::new();
    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) => {
                let name = e.name();
                let local = local_name(name.as_ref());
                let ns = find_ns(&e, xml_bytes);

                match (local, ns.as_deref()) {
                    ("UsernameToken", Some(WSSE_NS)) => {
                        in_username_token = true;
                        found_token = true;
                    }
                    ("Username", Some(WSSE_NS)) if in_username_token => {
                        in_username_elem = true;
                    }
                    ("Password", Some(WSSE_NS)) if in_username_token => {
                        in_password_elem = true;
                        // Read the Type attribute
                        for attr in e.attributes().flatten() {
                            if attr.key.as_ref() == b"Type" {
                                let val = String::from_utf8_lossy(&attr.value).to_string();
                                password_type = if val == PASSWORD_DIGEST_TYPE {
                                    PasswordType::Digest
                                } else if val == PASSWORD_TEXT_TYPE {
                                    PasswordType::Text
                                } else {
                                    PasswordType::Digest
                                };
                            }
                        }
                    }
                    ("Nonce", Some(WSSE_NS)) if in_username_token => {
                        in_nonce_elem = true;
                    }
                    ("Created", Some(WSU_NS)) if in_username_token => {
                        in_created_elem = true;
                    }
                    _ => {}
                }
            }
            Ok(Event::Empty(e)) => {
                // Handle self-closing elements if needed
                let name = e.name();
                let local = local_name(name.as_ref());
                let ns = find_ns_from_empty(&e, xml_bytes);
                if local == "UsernameToken" && ns.as_deref() == Some(WSSE_NS) {
                    found_token = true;
                }
            }
            Ok(Event::Text(e)) => {
                let text = e.decode()
                    .map(|s| s.to_string())
                    .unwrap_or_default();
                if in_username_elem {
                    username = Some(text);
                    in_username_elem = false;
                } else if in_password_elem {
                    password = Some(text);
                    in_password_elem = false;
                } else if in_nonce_elem {
                    nonce = Some(text);
                    in_nonce_elem = false;
                } else if in_created_elem {
                    created = Some(text);
                    in_created_elem = false;
                }
            }
            Ok(Event::End(e)) => {
                let name = e.name();
                let local = local_name(name.as_ref());
                match local {
                    "UsernameToken" => {
                        in_username_token = false;
                    }
                    "Username" => { in_username_elem = false; }
                    "Password" => { in_password_elem = false; }
                    "Nonce" => { in_nonce_elem = false; }
                    "Created" => { in_created_elem = false; }
                    _ => {}
                }
            }
            Ok(Event::Eof) => break,
            Err(e) => {
                return Err(SoapFault::sender(format!("WS-Security XML parse error: {e}")));
            }
            _ => {}
        }
        buf.clear();
    }

    if !found_token {
        return Err(SoapFault::sender("Missing UsernameToken"));
    }

    let username = username.ok_or_else(|| SoapFault::sender("Missing Username in UsernameToken"))?;
    let password = password.ok_or_else(|| SoapFault::sender("Missing Password in UsernameToken"))?;

    Ok(UsernameToken {
        username,
        password,
        password_type,
        nonce,
        created,
    })
}

/// Validate a WS-Security UsernameToken.
///
/// Returns the authenticated username on success, or a SoapFault on failure.
pub fn validate_username_token(
    security_bytes: &[u8],
    get_password: &dyn Fn(&str) -> Option<String>,
    nonce_cache: &mut RotatingNonceCache,
    tolerance_secs: i64,
    now: DateTime<Utc>,
) -> Result<String, SoapFault> {
    let token = parse_username_token(security_bytes)?;

    let stored_password = get_password(&token.username)
        .ok_or_else(|| SoapFault::sender("Unknown user"))?;

    // Verify password
    match token.password_type {
        PasswordType::Digest => {
            let nonce = token.nonce.as_deref()
                .ok_or_else(|| SoapFault::sender("Missing Nonce for PasswordDigest"))?;
            let created = token.created.as_deref()
                .ok_or_else(|| SoapFault::sender("Missing Created for PasswordDigest"))?;
            let expected = compute_digest(nonce, created, &stored_password)?;
            if expected != token.password {
                return Err(SoapFault::sender("Authentication failed"));
            }
        }
        PasswordType::Text => {
            if token.password != stored_password {
                return Err(SoapFault::sender("Authentication failed"));
            }
        }
    }

    // Check timestamp freshness
    if let Some(created_str) = &token.created {
        let created_dt = parse_created(created_str)?;
        check_freshness(now, created_dt, tolerance_secs)?;
    }

    // Check nonce for replay (for digest tokens)
    if let Some(nonce) = &token.nonce {
        nonce_cache.check_and_insert(nonce)?;
    }

    Ok(token.username)
}

/// Extract local name from a qualified XML element name.
fn local_name(name: &[u8]) -> &str {
    let s = std::str::from_utf8(name).unwrap_or("");
    match s.rfind(':') {
        Some(pos) => &s[pos + 1..],
        None => s,
    }
}

/// Find the namespace for a Start element by checking its prefix against namespace declarations.
fn find_ns(e: &quick_xml::events::BytesStart, _xml_bytes: &[u8]) -> Option<String> {
    let name = e.name();
    let name_str = std::str::from_utf8(name.as_ref()).unwrap_or("");
    let prefix = if let Some(pos) = name_str.find(':') {
        Some(&name_str[..pos])
    } else {
        None
    };

    // Look for xmlns: declarations in this element's attributes
    for attr in e.attributes().flatten() {
        let key = std::str::from_utf8(attr.key.as_ref()).unwrap_or("");
        let val = std::str::from_utf8(&attr.value).unwrap_or("");
        match prefix {
            Some(p) => {
                if key == format!("xmlns:{p}") {
                    return Some(val.to_string());
                }
            }
            None => {
                if key == "xmlns" {
                    return Some(val.to_string());
                }
            }
        }
    }
    None
}

/// Find the namespace for an Empty element.
fn find_ns_from_empty(e: &quick_xml::events::BytesStart, xml_bytes: &[u8]) -> Option<String> {
    find_ns(e, xml_bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;
    use crate::wssec::nonce_cache::RotatingNonceCache;

    // Known test vector from OASIS WS-Security spec / rpos reference implementation
    const TEST_NONCE: &str = "d36e316282959a9d7aF9e8";
    const TEST_CREATED: &str = "2010-09-09T14:18:30.000Z";
    const TEST_PASSWORD: &str = "userpassword";
    const TEST_EXPECTED_DIGEST: &str = "RL1yQQEFpFWFbOPjU9I6+c5p4r0=";

    fn test_now() -> DateTime<Utc> {
        // 1 second after the created timestamp — within tolerance
        Utc.with_ymd_and_hms(2010, 9, 9, 14, 18, 31).unwrap()
    }

    fn make_nonce_cache() -> RotatingNonceCache {
        RotatingNonceCache::new(150)
    }

    fn security_xml_digest(nonce: &str, created: &str, digest: &str) -> Vec<u8> {
        format!(r#"<wsse:Security xmlns:wsse="http://docs.oasis-open.org/wss/2004/01/oasis-200401-wss-wssecurity-secext-1.0.xsd" xmlns:wsu="http://docs.oasis-open.org/wss/2004/01/oasis-200401-wss-wssecurity-utility-1.0.xsd">
  <wsse:UsernameToken>
    <wsse:Username>admin</wsse:Username>
    <wsse:Password Type="http://docs.oasis-open.org/wss/2004/01/oasis-200401-wss-username-token-profile-1.0#PasswordDigest">{digest}</wsse:Password>
    <wsse:Nonce>{nonce}</wsse:Nonce>
    <wsu:Created>{created}</wsu:Created>
  </wsse:UsernameToken>
</wsse:Security>"#).into_bytes()
    }

    fn security_xml_text(password: &str) -> Vec<u8> {
        format!(r#"<wsse:Security xmlns:wsse="http://docs.oasis-open.org/wss/2004/01/oasis-200401-wss-wssecurity-secext-1.0.xsd">
  <wsse:UsernameToken>
    <wsse:Username>admin</wsse:Username>
    <wsse:Password Type="http://docs.oasis-open.org/wss/2004/01/oasis-200401-wss-username-token-profile-1.0#PasswordText">{password}</wsse:Password>
  </wsse:UsernameToken>
</wsse:Security>"#).into_bytes()
    }

    fn security_xml_no_token() -> Vec<u8> {
        br#"<wsse:Security xmlns:wsse="http://docs.oasis-open.org/wss/2004/01/oasis-200401-wss-wssecurity-secext-1.0.xsd">
</wsse:Security>"#.to_vec()
    }

    fn security_xml_no_password() -> Vec<u8> {
        br#"<wsse:Security xmlns:wsse="http://docs.oasis-open.org/wss/2004/01/oasis-200401-wss-wssecurity-secext-1.0.xsd">
  <wsse:UsernameToken>
    <wsse:Username>admin</wsse:Username>
  </wsse:UsernameToken>
</wsse:Security>"#.to_vec()
    }

    // ---- compute_digest tests ----

    #[test]
    fn known_vector_digest_matches_expected() {
        let result = compute_digest(TEST_NONCE, TEST_CREATED, TEST_PASSWORD).unwrap();
        assert_eq!(
            result,
            TEST_EXPECTED_DIGEST,
            "PasswordDigest known vector failed: got {result}, expected {TEST_EXPECTED_DIGEST}"
        );
    }

    #[test]
    fn compute_digest_invalid_base64_nonce_returns_err() {
        let result = compute_digest("not!!!valid_base64!!!", TEST_CREATED, TEST_PASSWORD);
        assert!(result.is_err());
    }

    // ---- parse_username_token tests ----

    #[test]
    fn parse_digest_token_extracts_all_fields() {
        let xml = security_xml_digest(TEST_NONCE, TEST_CREATED, TEST_EXPECTED_DIGEST);
        let token = parse_username_token(&xml).unwrap();
        assert_eq!(token.username, "admin");
        assert_eq!(token.password, TEST_EXPECTED_DIGEST);
        assert_eq!(token.password_type, PasswordType::Digest);
        assert_eq!(token.nonce.as_deref(), Some(TEST_NONCE));
        assert_eq!(token.created.as_deref(), Some(TEST_CREATED));
    }

    #[test]
    fn parse_text_token_extracts_fields() {
        let xml = security_xml_text("secret");
        let token = parse_username_token(&xml).unwrap();
        assert_eq!(token.username, "admin");
        assert_eq!(token.password, "secret");
        assert_eq!(token.password_type, PasswordType::Text);
        assert!(token.nonce.is_none());
    }

    #[test]
    fn parse_missing_username_token_returns_err() {
        let xml = security_xml_no_token();
        let result = parse_username_token(&xml);
        assert!(result.is_err());
        let fault = result.unwrap_err();
        assert!(
            fault.reason.contains("Missing UsernameToken"),
            "got: {}",
            fault.reason
        );
    }

    #[test]
    fn parse_missing_password_returns_err() {
        let xml = security_xml_no_password();
        let result = parse_username_token(&xml);
        assert!(result.is_err());
    }

    // ---- validate_username_token tests ----

    fn get_password(username: &str) -> Option<String> {
        if username == "admin" {
            Some(TEST_PASSWORD.to_string())
        } else {
            None
        }
    }

    #[test]
    fn validate_correct_digest_returns_username() {
        let xml = security_xml_digest(TEST_NONCE, TEST_CREATED, TEST_EXPECTED_DIGEST);
        let mut cache = make_nonce_cache();
        let result = validate_username_token(&xml, &get_password, &mut cache, 300, test_now());
        assert_eq!(result.unwrap(), "admin");
    }

    #[test]
    fn validate_wrong_password_returns_auth_failed() {
        // Compute digest with wrong password
        let bad_digest = compute_digest(TEST_NONCE, TEST_CREATED, "wrongpassword").unwrap();
        let xml = security_xml_digest(TEST_NONCE, TEST_CREATED, &bad_digest);
        let mut cache = make_nonce_cache();
        let result = validate_username_token(&xml, &get_password, &mut cache, 300, test_now());
        assert!(result.is_err());
        let fault = result.unwrap_err();
        assert!(
            fault.reason.contains("Authentication failed"),
            "got: {}",
            fault.reason
        );
    }

    #[test]
    fn validate_unknown_user_returns_err() {
        let xml = format!(r#"<wsse:Security xmlns:wsse="http://docs.oasis-open.org/wss/2004/01/oasis-200401-wss-wssecurity-secext-1.0.xsd" xmlns:wsu="http://docs.oasis-open.org/wss/2004/01/oasis-200401-wss-wssecurity-utility-1.0.xsd">
  <wsse:UsernameToken>
    <wsse:Username>unknownuser</wsse:Username>
    <wsse:Password Type="http://docs.oasis-open.org/wss/2004/01/oasis-200401-wss-username-token-profile-1.0#PasswordDigest">{TEST_EXPECTED_DIGEST}</wsse:Password>
    <wsse:Nonce>{TEST_NONCE}</wsse:Nonce>
    <wsu:Created>{TEST_CREATED}</wsu:Created>
  </wsse:UsernameToken>
</wsse:Security>"#);
        let mut cache = make_nonce_cache();
        let result = validate_username_token(xml.as_bytes(), &get_password, &mut cache, 300, test_now());
        assert!(result.is_err());
        let fault = result.unwrap_err();
        assert!(
            fault.reason.contains("Unknown user"),
            "got: {}",
            fault.reason
        );
    }

    #[test]
    fn validate_expired_timestamp_returns_err() {
        // Use a 'now' that is 400 seconds after the created timestamp
        let expired_now = Utc.with_ymd_and_hms(2010, 9, 9, 14, 25, 30).unwrap();
        let xml = security_xml_digest(TEST_NONCE, TEST_CREATED, TEST_EXPECTED_DIGEST);
        let mut cache = make_nonce_cache();
        let result = validate_username_token(&xml, &get_password, &mut cache, 300, expired_now);
        assert!(result.is_err());
        let fault = result.unwrap_err();
        assert!(
            fault.reason.contains("expired"),
            "got: {}",
            fault.reason
        );
    }

    #[test]
    fn validate_replayed_nonce_returns_err() {
        let xml = security_xml_digest(TEST_NONCE, TEST_CREATED, TEST_EXPECTED_DIGEST);
        let mut cache = make_nonce_cache();
        // First call succeeds
        validate_username_token(&xml, &get_password, &mut cache, 300, test_now()).unwrap();
        // Second call with same nonce should fail (replay)
        let result = validate_username_token(&xml, &get_password, &mut cache, 300, test_now());
        assert!(result.is_err());
        let fault = result.unwrap_err();
        assert!(
            fault.reason.contains("replay"),
            "got: {}",
            fault.reason
        );
    }

    #[test]
    fn validate_text_password_correct() {
        let xml = security_xml_text(TEST_PASSWORD);
        let mut cache = make_nonce_cache();
        let result = validate_username_token(&xml, &get_password, &mut cache, 300, test_now());
        assert_eq!(result.unwrap(), "admin");
    }

    #[test]
    fn validate_text_password_wrong_returns_err() {
        let xml = security_xml_text("wrongpassword");
        let mut cache = make_nonce_cache();
        let result = validate_username_token(&xml, &get_password, &mut cache, 300, test_now());
        assert!(result.is_err());
        let fault = result.unwrap_err();
        assert!(
            fault.reason.contains("Authentication failed"),
            "got: {}",
            fault.reason
        );
    }

    #[test]
    fn validate_missing_username_token_returns_err() {
        let xml = security_xml_no_token();
        let mut cache = make_nonce_cache();
        let result = validate_username_token(&xml, &get_password, &mut cache, 300, test_now());
        assert!(result.is_err());
        let fault = result.unwrap_err();
        assert!(
            fault.reason.contains("Missing UsernameToken"),
            "got: {}",
            fault.reason
        );
    }
}
