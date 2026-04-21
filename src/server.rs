// ServerBuilder + SoapService — integration layer composing all components.
// Produces an axum::Router serving SOAP 1.2 requests.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use axum::{
    extract::{MatchedPath, Query, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
    Router,
};
use bytes::Bytes;
use chrono::Utc;
use tokio::sync::Mutex;

use crate::dispatch::{self, DispatchError, DispatchTable};
use crate::envelope::{
    detect_soap_version, parse_envelope, response_content_type, serialize_envelope,
};
use crate::fault::SoapFault;
use crate::handler::SoapHandler;
use crate::qname::QName;
use crate::wsdl::definitions::SoapVersion;
use crate::wsdl::resolver::{resolve_wsdl, rewrite_wsdl_address, WsdlLoader};
use crate::wssec::{nonce_cache::RotatingNonceCache, username_token::validate_username_token};
use crate::xsd::types::TypeRegistry;

/// Default nonce cache half-window in seconds (150s → 300s total replay window).
const DEFAULT_NONCE_CACHE_HALF_WINDOW_SECS: u64 = 150;
/// Default timestamp tolerance in seconds (±300s).
const DEFAULT_TIMESTAMP_TOLERANCE_SECS: i64 = 300;

/// Authentication function type: takes a raw Authorization header value and returns a username if valid.
type AuthFn = Option<Arc<dyn Fn(&str) -> Option<String> + Send + Sync + 'static>>;

// ── ServerBuilder ─────────────────────────────────────────────────────────────

/// Builder for a SoapService. Accumulates WSDL source, handlers, auth config, and routing.
pub struct ServerBuilder {
    wsdl_bytes: Option<Vec<u8>>,
    wsdl_path: Option<std::path::PathBuf>,
    custom_loader: Option<Arc<dyn WsdlLoader>>,
    handlers: HashMap<String, Arc<dyn SoapHandler>>,
    default_handler: Option<Arc<dyn SoapHandler>>,
    auth_fn: AuthFn,
    auth_bypass: HashSet<String>,
    mount_path: String,
    timestamp_tolerance_secs: i64,
    nonce_cache_half_window_secs: u64,
}

impl ServerBuilder {
    fn new() -> Self {
        Self {
            wsdl_bytes: None,
            wsdl_path: None,
            custom_loader: None,
            handlers: HashMap::new(),
            default_handler: None,
            auth_fn: None,
            auth_bypass: HashSet::new(),
            mount_path: "/soap".to_string(),
            timestamp_tolerance_secs: DEFAULT_TIMESTAMP_TOLERANCE_SECS,
            nonce_cache_half_window_secs: DEFAULT_NONCE_CACHE_HALF_WINDOW_SECS,
        }
    }

    /// Load WSDL from the given file path at build time.
    pub fn from_wsdl_file(path: impl Into<std::path::PathBuf>) -> Self {
        let mut builder = Self::new();
        builder.wsdl_path = Some(path.into());
        builder
    }

    /// Use the provided WSDL bytes directly (no file I/O).
    /// For WSDLs with external imports, use `from_wsdl_file` or `from_wsdl_bytes_with_loader`.
    pub fn from_wsdl_bytes(bytes: impl Into<Vec<u8>>) -> Self {
        let mut builder = Self::new();
        builder.wsdl_bytes = Some(bytes.into());
        builder
    }

    /// Use the provided WSDL bytes with a custom loader for resolving external imports.
    /// The loader is invoked for any `wsdl:import` or `xs:import` location strings.
    pub fn from_wsdl_bytes_with_loader(
        bytes: impl Into<Vec<u8>>,
        loader: impl WsdlLoader + 'static,
    ) -> Self {
        let mut builder = Self::new();
        builder.wsdl_bytes = Some(bytes.into());
        builder.custom_loader = Some(Arc::new(loader));
        builder
    }

    /// Register a handler for the named WSDL operation.
    pub fn handler(mut self, operation: impl Into<String>, handler: impl SoapHandler) -> Self {
        self.handlers.insert(operation.into(), Arc::new(handler));
        self
    }

    /// Register a catch-all handler invoked for any WSDL operation without a specific handler.
    /// When set, `build()` will not return `UnregisteredOperation` for unhandled operations.
    pub fn default_handler(mut self, handler: impl SoapHandler) -> Self {
        self.default_handler = Some(Arc::new(handler));
        self
    }

    /// Configure credential lookup. The closure is called with a username and must return
    /// the stored password (plaintext) for that user, or `None` if the user does not exist.
    pub fn auth<F>(mut self, f: F) -> Self
    where
        F: Fn(&str) -> Option<String> + Send + Sync + 'static,
    {
        self.auth_fn = Some(Arc::new(f));
        self
    }

    /// Mark the named operations as auth-bypassed (no WS-Security header required).
    /// Accepts any iterable of string-like values.
    pub fn auth_bypass<I, S>(mut self, ops: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        for op in ops {
            self.auth_bypass.insert(op.into());
        }
        self
    }

    /// Override the mount path (default: "/soap").
    pub fn path(mut self, path: impl Into<String>) -> Self {
        self.mount_path = path.into();
        self
    }

    /// Override the WS-Security timestamp tolerance in seconds (default: 300).
    pub fn timestamp_tolerance_secs(mut self, secs: i64) -> Self {
        self.timestamp_tolerance_secs = secs;
        self
    }

    /// Build the SoapService, resolving the WSDL and building the dispatch table.
    /// Returns an error if the WSDL cannot be resolved or the dispatch table is inconsistent.
    pub fn build(self) -> Result<SoapService, BuildError> {
        // Step 1: Load WSDL bytes, preserving the file path for loader selection.
        let (wsdl_bytes, wsdl_file_path) = match (self.wsdl_bytes, self.wsdl_path) {
            (Some(bytes), _) => (bytes, None),
            (None, Some(path)) => {
                let bytes = std::fs::read(&path).map_err(|e| BuildError::WsdlIo(e.to_string()))?;
                (bytes, Some(path))
            }
            (None, None) => return Err(BuildError::MissingWsdl),
        };

        // Step 2: Resolve WSDL (pass 1 + pass 2).
        // Loader selection priority:
        //   1. custom_loader (explicitly provided via from_wsdl_bytes_with_loader)
        //   2. FileWsdlLoader (when loaded from a file path)
        //   3. NoOpLoader (embedded bytes — imports unsupported)
        let mut visited = HashSet::new();
        let resolved = if let Some(ref loader) = self.custom_loader {
            resolve_wsdl(&wsdl_bytes, loader.as_ref(), &mut visited)
                .map_err(|e| BuildError::WsdlParse(e.to_string()))?
        } else if let Some(ref path) = wsdl_file_path {
            let base_dir = path
                .parent()
                .unwrap_or_else(|| std::path::Path::new("."))
                .to_path_buf();
            let loader = FileWsdlLoader { base_dir };
            resolve_wsdl(&wsdl_bytes, &loader, &mut visited)
                .map_err(|e| BuildError::WsdlParse(e.to_string()))?
        } else {
            let loader = NoOpLoader;
            resolve_wsdl(&wsdl_bytes, &loader, &mut visited)
                .map_err(|e| BuildError::WsdlParse(e.to_string()))?
        };

        // Step 3: Build dispatch table(s).
        // If WSDL has multiple services, build per-service dispatch tables.
        // Otherwise, build a single dispatch table for backward compatibility.
        let service_names: Vec<String> = resolved.definition.services.keys().cloned().collect();
        let is_multi_service = service_names.len() > 1;

        let (dispatch_table, service_tables) = if is_multi_service {
            // Multi-service mode: build one table per service, mount at per-service path.
            // The handlers HashMap is shared across services — each service only uses the
            // handlers for operations it owns. We clone handlers for each service.
            let mut service_tables: HashMap<String, Arc<DispatchTable>> = HashMap::new();
            let mut all_ops_in_all_services: HashSet<String> = HashSet::new();

            // First pass: collect which ops belong to each service.
            for svc_name in &service_names {
                let svc = resolved.definition.services.get(svc_name).unwrap();
                for port in &svc.ports {
                    let binding_local = &port.binding.local_name;
                    if let Some(binding) = resolved.definition.bindings.get(binding_local) {
                        for binding_op in &binding.operations {
                            all_ops_in_all_services.insert(binding_op.name.clone());
                        }
                    }
                }
            }

            // Verify no handlers registered for unknown operations.
            for handler_name in self.handlers.keys() {
                if !all_ops_in_all_services.contains(handler_name) {
                    return Err(BuildError::UnknownOperation(handler_name.clone()));
                }
            }

            for svc_name in &service_names {
                let svc = resolved.definition.services.get(svc_name).unwrap();

                // Collect ops for this service and build handlers subset.
                let mut svc_op_names: Vec<String> = Vec::new();
                for port in &svc.ports {
                    let binding_local = &port.binding.local_name;
                    if let Some(binding) = resolved.definition.bindings.get(binding_local) {
                        for binding_op in &binding.operations {
                            svc_op_names.push(binding_op.name.clone());
                        }
                    }
                }

                // Build handlers for this service only.
                let mut svc_handlers: HashMap<String, Arc<dyn SoapHandler>> = HashMap::new();
                for op_name in &svc_op_names {
                    if let Some(h) = self.handlers.get(op_name) {
                        svc_handlers.insert(op_name.clone(), h.clone());
                    }
                }

                let table = dispatch::build_dispatch_table_for_service(
                    svc_name,
                    &resolved,
                    svc_handlers,
                    &self.auth_bypass,
                    self.default_handler.clone(),
                )
                .map_err(|e| match e {
                    DispatchError::UnregisteredOperation(op) => {
                        BuildError::UnregisteredOperation(op)
                    }
                    DispatchError::UnknownOperation(op) => BuildError::UnknownOperation(op),
                })?;

                // Derive the route path from the first port's address.
                let path = svc
                    .ports
                    .first()
                    .map(|p| extract_path_from_url(&p.address))
                    .unwrap_or_else(|| format!("/{}", svc_name.to_lowercase()));

                service_tables.insert(path, Arc::new(table));
            }

            // Build a combined table for the dispatch_table field (used for single-route fallback).
            // Use the first service's table as the primary — in multi-service mode,
            // routing uses service_tables exclusively.
            let first_table = service_tables
                .values()
                .next()
                .cloned()
                .unwrap_or_else(|| Arc::new(DispatchTable::empty()));

            (first_table, service_tables)
        } else {
            // Single-service mode: build one dispatch table, no per-service tables.
            let table = dispatch::build_dispatch_table(
                &resolved,
                self.handlers,
                &self.auth_bypass,
                self.default_handler,
            )
            .map_err(|e| match e {
                DispatchError::UnregisteredOperation(op) => BuildError::UnregisteredOperation(op),
                DispatchError::UnknownOperation(op) => BuildError::UnknownOperation(op),
            })?;
            (Arc::new(table), HashMap::new())
        };

        let type_registry = Arc::new(resolved.type_registry);

        Ok(SoapService {
            dispatch_table,
            service_tables,
            type_registry,
            wsdl_raw: Arc::new(wsdl_bytes),
            auth_fn: self.auth_fn,
            nonce_cache: Arc::new(Mutex::new(RotatingNonceCache::new(
                self.nonce_cache_half_window_secs,
            ))),
            timestamp_tolerance_secs: self.timestamp_tolerance_secs,
            mount_path: self.mount_path,
        })
    }
}

// ── Build errors ──────────────────────────────────────────────────────────────

/// Errors that can occur during ServerBuilder::build().
#[derive(Debug, thiserror::Error)]
pub enum BuildError {
    #[error("No WSDL source provided — call from_wsdl_bytes() or from_wsdl_file()")]
    MissingWsdl,
    #[error("Failed to read WSDL file: {0}")]
    WsdlIo(String),
    #[error("Failed to parse or resolve WSDL: {0}")]
    WsdlParse(String),
    #[error("WSDL operation '{0}' has no registered handler")]
    UnregisteredOperation(String),
    #[error("Registered handler '{0}' has no matching WSDL operation")]
    UnknownOperation(String),
}

// ── WSDL loader (no-op for embedded/self-contained WSDLs) ────────────────────

struct NoOpLoader;

impl WsdlLoader for NoOpLoader {
    fn load(&self, location: &str) -> Result<Vec<u8>, crate::wsdl::parser::WsdlError> {
        Err(crate::wsdl::parser::WsdlError::MalformedXml(format!(
            "External WSDL import '{location}' not supported in embedded mode"
        )))
    }
}

/// A WsdlLoader that resolves import locations relative to a base directory on the filesystem.
/// Used automatically when ServerBuilder::from_wsdl_file() is called.
pub struct FileWsdlLoader {
    base_dir: std::path::PathBuf,
}

impl WsdlLoader for FileWsdlLoader {
    fn load(&self, location: &str) -> Result<Vec<u8>, crate::wsdl::parser::WsdlError> {
        // Resolve the location relative to the base directory, normalizing ".." components.
        let raw_path = self.base_dir.join(location);
        let path = normalize_path(&raw_path);
        std::fs::read(&path).map_err(|e| {
            crate::wsdl::parser::WsdlError::MalformedXml(format!(
                "Failed to load WSDL import '{location}' from '{}': {e}",
                path.display()
            ))
        })
    }
}

/// Normalize a path by resolving ".." components without requiring the path to exist.
/// This is needed because std::fs::canonicalize requires the path to exist on disk.
fn normalize_path(path: &std::path::Path) -> std::path::PathBuf {
    use std::path::Component;
    let mut normalized = std::path::PathBuf::new();
    for component in path.components() {
        match component {
            Component::ParentDir => {
                normalized.pop();
            }
            Component::CurDir => {}
            other => {
                normalized.push(other);
            }
        }
    }
    normalized
}

/// Extract the path component from a URL string (e.g., "http://host/soap/ServiceA" → "/soap/ServiceA").
/// Falls back to "/" if the URL has no path.
fn extract_path_from_url(url: &str) -> String {
    if let Some(after_scheme) = url.split_once("://").map(|x| x.1) {
        if let Some(slash_pos) = after_scheme.find('/') {
            return after_scheme[slash_pos..].to_string();
        }
        return "/".to_string();
    }
    if url.starts_with('/') {
        url.to_string()
    } else {
        format!("/{url}")
    }
}

// ── SoapService ───────────────────────────────────────────────────────────────

/// A fully configured SOAP service that can be converted into an axum Router.
pub struct SoapService {
    dispatch_table: Arc<DispatchTable>,
    /// Per-service dispatch tables keyed by route path.
    /// Non-empty when the WSDL has multiple services (multi-service mode).
    /// Empty in single-service mode (backward-compat).
    service_tables: HashMap<String, Arc<DispatchTable>>,
    type_registry: Arc<TypeRegistry>,
    wsdl_raw: Arc<Vec<u8>>,
    auth_fn: AuthFn,
    nonce_cache: Arc<Mutex<RotatingNonceCache>>,
    timestamp_tolerance_secs: i64,
    mount_path: String,
}

impl std::fmt::Debug for SoapService {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SoapService")
            .field("mount_path", &self.mount_path)
            .finish_non_exhaustive()
    }
}

impl SoapService {
    /// Convert this service into an axum Router with POST (SOAP) and GET (?wsdl) routes.
    /// The returned Router is composable with Router::merge().
    ///
    /// In multi-service mode: registers one POST route per service path (from service_tables).
    /// In single-service mode: registers a single POST + GET route at mount_path (backward-compat).
    pub fn into_router(self) -> Router {
        if !self.service_tables.is_empty() {
            // Multi-service mode: each service gets its own POST + GET (?wsdl) route.
            let state = Arc::new(self);
            let mut router = Router::new();
            for (path, table) in &state.service_tables {
                let route_state = SoapServiceRoute {
                    svc: state.clone(),
                    table: table.clone(),
                };
                router = router.route(
                    path,
                    post(soap_post_handler_for_route).with_state(route_state),
                );
                // Register GET ?wsdl handler with the shared Arc<SoapService> state.
                router = router.route(path, get(wsdl_get_handler).with_state(state.clone()));
            }
            router
        } else {
            // Single-service mode: single route (backward-compat).
            let mount_path = self.mount_path.clone();
            let state = Arc::new(self);
            Router::new()
                .route(&mount_path, post(soap_post_handler).get(wsdl_get_handler))
                .with_state(state)
        }
    }
}

/// Thin wrapper for per-service route state in multi-service mode.
#[derive(Clone)]
struct SoapServiceRoute {
    svc: Arc<SoapService>,
    table: Arc<DispatchTable>,
}

// ── Helper: return a 500 SOAP fault response ──────────────────────────────────

fn fault_response(fault: SoapFault, version: crate::wsdl::definitions::SoapVersion) -> Response {
    let bytes = fault.to_xml_bytes_versioned(&version);
    let ct = response_content_type(&version);
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        [("Content-Type", ct)],
        bytes,
    )
        .into_response()
}

// ── Helper: extract QName of the first element in body_element bytes ──────────

fn extract_body_qname(body_bytes: &[u8]) -> Result<QName, SoapFault> {
    use quick_xml::events::Event;
    use quick_xml::NsReader;

    let mut reader = NsReader::from_reader(body_bytes);
    reader.config_mut().trim_text(true);

    loop {
        match reader
            .read_resolved_event()
            .map_err(|e| SoapFault::sender(format!("XML parse error in body: {e}")))?
        {
            (_, Event::Eof) => {
                return Err(SoapFault::sender("Empty SOAP Body element"));
            }
            (resolved_ns, Event::Start(e)) | (resolved_ns, Event::Empty(e)) => {
                let local = std::str::from_utf8(e.local_name().as_ref())
                    .map_err(|e| SoapFault::sender(format!("Invalid UTF-8 in element name: {e}")))?
                    .to_string();
                let ns = match resolved_ns {
                    quick_xml::name::ResolveResult::Bound(ns) => std::str::from_utf8(ns.0)
                        .map_err(|e| SoapFault::sender(format!("Invalid UTF-8 in namespace: {e}")))?
                        .to_string(),
                    _ => String::new(),
                };
                if ns.is_empty() {
                    return Ok(QName::local(&local));
                } else {
                    return Ok(QName::new(&ns, &local));
                }
            }
            _ => {}
        }
    }
}

// ── Helper: find the wsse:Security header bytes from header_children ──────────

fn find_security_header(header_children: &[Bytes]) -> Option<&Bytes> {
    for child in header_children {
        // Quick check: does this child contain "Security"?
        if let Ok(s) = std::str::from_utf8(child) {
            if s.contains("Security") {
                return Some(child);
            }
        }
    }
    None
}

// ── axum handler: POST /soap ──────────────────────────────────────────────────

async fn soap_post_handler(
    State(svc): State<Arc<SoapService>>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    // Step 1: Detect SOAP version from Content-Type.
    let content_type = headers
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");

    let soap_version = match detect_soap_version(content_type) {
        Ok(v) => v,
        Err(fault) => return fault_response(fault, crate::wsdl::definitions::SoapVersion::Soap12),
    };

    // Step 2: Parse envelope.
    let envelope = match parse_envelope(&body) {
        Ok(e) => e,
        Err(fault) => return fault_response(fault, soap_version),
    };

    // Step 3: Extract body first-child QName.
    let body_qname = match extract_body_qname(&envelope.body_element) {
        Ok(q) => q,
        Err(fault) => return fault_response(fault, envelope.soap_version.clone()),
    };

    // Step 4: Route to dispatch entry.
    let soap_action = headers
        .get("soapaction")
        .or_else(|| headers.get("SOAPAction"))
        .and_then(|v| v.to_str().ok())
        .map(|s| s.trim_matches('"'));

    let entry = match dispatch::route(&svc.dispatch_table, &body_qname, soap_action) {
        Ok(e) => e,
        Err(fault) => return fault_response(fault, envelope.soap_version.clone()),
    };

    // Step 5: If auth required, validate WS-Security UsernameToken.
    if entry.auth_required {
        match find_security_header(&envelope.header_children) {
            None => {
                return fault_response(
                    SoapFault::sender("WS-Security header required but not provided"),
                    envelope.soap_version.clone(),
                );
            }
            Some(security_bytes) => {
                let auth_fn = match &svc.auth_fn {
                    Some(f) => f.clone(),
                    None => {
                        return fault_response(
                            SoapFault::sender(
                                "Authentication required but no credential store configured",
                            ),
                            envelope.soap_version.clone(),
                        );
                    }
                };
                let mut nonce_cache = svc.nonce_cache.lock().await;
                let now = Utc::now();
                if let Err(fault) = validate_username_token(
                    security_bytes,
                    auth_fn.as_ref(),
                    &mut nonce_cache,
                    svc.timestamp_tolerance_secs,
                    now,
                ) {
                    return fault_response(fault, envelope.soap_version.clone());
                }
            }
        }
    }

    // Step 6: XSD structural validation.
    if let Err(fault) = dispatch::validate_request(
        &envelope.body_element,
        &svc.type_registry,
        entry.input_type.as_ref(),
    ) {
        return fault_response(fault, envelope.soap_version.clone());
    }

    // Step 7: Invoke handler.
    let response_body = match entry.handler.handle(envelope.body_element).await {
        Ok(bytes) => bytes,
        Err(fault) => return fault_response(fault, envelope.soap_version.clone()),
    };

    // Step 8: Serialize into SOAP envelope.
    let envelope_bytes = serialize_envelope(response_body, soap_version);
    let content_type_value = response_content_type(&envelope.soap_version);

    (
        StatusCode::OK,
        [("Content-Type", content_type_value)],
        envelope_bytes,
    )
        .into_response()
}

// ── axum handler: POST per-service route (multi-service mode) ─────────────────

async fn soap_post_handler_for_route(
    State(route_state): State<SoapServiceRoute>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let svc = &route_state.svc;
    let table = &route_state.table;

    // Step 1: Detect SOAP version from Content-Type.
    let content_type = headers
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");

    let soap_version = match detect_soap_version(content_type) {
        Ok(v) => v,
        Err(fault) => return fault_response(fault, SoapVersion::Soap12),
    };

    // Step 2: Parse envelope.
    let envelope = match parse_envelope(&body) {
        Ok(e) => e,
        Err(fault) => return fault_response(fault, soap_version),
    };

    // Step 3: Extract body first-child QName.
    let body_qname = match extract_body_qname(&envelope.body_element) {
        Ok(q) => q,
        Err(fault) => return fault_response(fault, envelope.soap_version.clone()),
    };

    // Step 4: Route using this service's specific dispatch table.
    let soap_action = headers
        .get("soapaction")
        .or_else(|| headers.get("SOAPAction"))
        .and_then(|v| v.to_str().ok())
        .map(|s| s.trim_matches('"'));

    let entry = match dispatch::route(table, &body_qname, soap_action) {
        Ok(e) => e,
        Err(fault) => return fault_response(fault, envelope.soap_version.clone()),
    };

    // Step 5: If auth required, validate WS-Security UsernameToken.
    if entry.auth_required {
        match find_security_header(&envelope.header_children) {
            None => {
                return fault_response(
                    SoapFault::sender("WS-Security header required but not provided"),
                    envelope.soap_version.clone(),
                );
            }
            Some(security_bytes) => {
                let auth_fn = match &svc.auth_fn {
                    Some(f) => f.clone(),
                    None => {
                        return fault_response(
                            SoapFault::sender(
                                "Authentication required but no credential store configured",
                            ),
                            envelope.soap_version.clone(),
                        );
                    }
                };
                let mut nonce_cache = svc.nonce_cache.lock().await;
                let now = Utc::now();
                if let Err(fault) = validate_username_token(
                    security_bytes,
                    auth_fn.as_ref(),
                    &mut nonce_cache,
                    svc.timestamp_tolerance_secs,
                    now,
                ) {
                    return fault_response(fault, envelope.soap_version.clone());
                }
            }
        }
    }

    // Step 6: XSD structural validation.
    if let Err(fault) = dispatch::validate_request(
        &envelope.body_element,
        &svc.type_registry,
        entry.input_type.as_ref(),
    ) {
        return fault_response(fault, envelope.soap_version.clone());
    }

    // Step 7: Invoke handler.
    let response_body = match entry.handler.handle(envelope.body_element).await {
        Ok(bytes) => bytes,
        Err(fault) => return fault_response(fault, envelope.soap_version.clone()),
    };

    // Step 8: Serialize into SOAP envelope.
    let envelope_bytes = serialize_envelope(response_body, soap_version);
    let content_type_value = response_content_type(&envelope.soap_version);

    (
        StatusCode::OK,
        [("Content-Type", content_type_value)],
        envelope_bytes,
    )
        .into_response()
}

// ── axum handler: GET /soap?wsdl ──────────────────────────────────────────────

#[derive(serde::Deserialize)]
struct WsdlQuery {
    wsdl: Option<String>,
}

async fn wsdl_get_handler(
    matched_path: Option<MatchedPath>,
    State(svc): State<Arc<SoapService>>,
    Query(params): Query<WsdlQuery>,
    headers: HeaderMap,
) -> Response {
    // Only respond when ?wsdl query parameter is present.
    if params.wsdl.is_none() {
        return StatusCode::NOT_FOUND.into_response();
    }

    // Build the server URL from Host header (or X-Forwarded-Host).
    let host = headers
        .get("x-forwarded-host")
        .or_else(|| headers.get("host"))
        .and_then(|v| v.to_str().ok())
        .unwrap_or("localhost");

    // Use the matched route path (per-service in multi-service mode, mount_path in single-service).
    // MatchedPath is always present when registered via router.route() — use Option as fallback.
    let path = matched_path
        .as_ref()
        .map(|mp| mp.as_str())
        .unwrap_or(&svc.mount_path);

    let server_url = format!("http://{host}{path}");

    let rewritten = rewrite_wsdl_address(&svc.wsdl_raw, &server_url);

    (
        StatusCode::OK,
        [("Content-Type", "text/xml; charset=utf-8")],
        rewritten,
    )
        .into_response()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fault::SoapFault;
    use crate::handler::FnHandler;
    use bytes::Bytes;

    const MINIMAL_WSDL: &[u8] = br#"<?xml version="1.0" encoding="utf-8"?>
<wsdl:definitions
    xmlns:wsdl="http://schemas.xmlsoap.org/wsdl/"
    xmlns:soap="http://schemas.xmlsoap.org/wsdl/soap12/"
    xmlns:xs="http://www.w3.org/2001/XMLSchema"
    xmlns:tns="http://example.com/test"
    targetNamespace="http://example.com/test">
    <wsdl:types>
        <xs:schema targetNamespace="http://example.com/test" elementFormDefault="qualified">
            <xs:element name="Ping">
                <xs:complexType><xs:sequence/></xs:complexType>
            </xs:element>
            <xs:element name="PingResponse">
                <xs:complexType><xs:sequence/></xs:complexType>
            </xs:element>
        </xs:schema>
    </wsdl:types>
    <wsdl:message name="PingRequest">
        <wsdl:part name="parameters" element="tns:Ping"/>
    </wsdl:message>
    <wsdl:message name="PingResponse">
        <wsdl:part name="parameters" element="tns:PingResponse"/>
    </wsdl:message>
    <wsdl:portType name="TestPortType">
        <wsdl:operation name="Ping">
            <wsdl:input message="tns:PingRequest"/>
            <wsdl:output message="tns:PingResponse"/>
        </wsdl:operation>
    </wsdl:portType>
    <wsdl:binding name="TestBinding" type="tns:TestPortType">
        <soap:binding style="document" transport="http://schemas.xmlsoap.org/soap/http"/>
        <wsdl:operation name="Ping">
            <soap:operation soapAction="http://example.com/test/Ping"/>
            <wsdl:input><soap:body use="literal"/></wsdl:input>
            <wsdl:output><soap:body use="literal"/></wsdl:output>
        </wsdl:operation>
    </wsdl:binding>
    <wsdl:service name="TestService">
        <wsdl:port name="TestPort" binding="tns:TestBinding">
            <soap:address location="http://localhost/soap"/>
        </wsdl:port>
    </wsdl:service>
</wsdl:definitions>"#;

    #[test]
    fn server_builder_builds_without_panic() {
        let svc = ServerBuilder::from_wsdl_bytes(MINIMAL_WSDL)
            .handler(
                "Ping",
                FnHandler::new(|_body: Bytes| async move {
                    Ok::<Bytes, SoapFault>(Bytes::from_static(b"<PingResponse/>"))
                }),
            )
            .auth_bypass(["Ping"])
            .build();
        assert!(svc.is_ok(), "build should succeed: {:?}", svc.err());
    }

    #[test]
    fn server_builder_into_router_returns_router() {
        let svc = ServerBuilder::from_wsdl_bytes(MINIMAL_WSDL)
            .handler(
                "Ping",
                FnHandler::new(|_body: Bytes| async move {
                    Ok::<Bytes, SoapFault>(Bytes::from_static(b"<PingResponse/>"))
                }),
            )
            .auth_bypass(["Ping"])
            .build()
            .unwrap();
        // into_router() must not panic
        let _router = svc.into_router();
    }

    #[test]
    fn server_builder_fails_with_unregistered_operation() {
        // WSDL has Ping but no handler is provided.
        let result = ServerBuilder::from_wsdl_bytes(MINIMAL_WSDL).build();
        assert!(result.is_err());
        match result.unwrap_err() {
            BuildError::UnregisteredOperation(op) => assert_eq!(op, "Ping"),
            other => panic!("Expected UnregisteredOperation, got: {other:?}"),
        }
    }

    #[test]
    fn server_builder_fails_with_unknown_handler_name() {
        let result = ServerBuilder::from_wsdl_bytes(MINIMAL_WSDL)
            .handler(
                "Ping",
                FnHandler::new(|_body: Bytes| async move {
                    Ok::<Bytes, SoapFault>(Bytes::from_static(b"<PingResponse/>"))
                }),
            )
            .handler(
                "NonExistentOp",
                FnHandler::new(|_body: Bytes| async move {
                    Ok::<Bytes, SoapFault>(Bytes::from_static(b"<resp/>"))
                }),
            )
            .auth_bypass(["Ping"])
            .build();
        assert!(result.is_err());
        match result.unwrap_err() {
            BuildError::UnknownOperation(op) => assert_eq!(op, "NonExistentOp"),
            other => panic!("Expected UnknownOperation, got: {other:?}"),
        }
    }

    #[test]
    fn fault_response_soap12_content_type() {
        use crate::wsdl::definitions::SoapVersion;
        let fault = SoapFault::sender("test");
        let response = fault_response(fault, SoapVersion::Soap12);
        let ct = response.headers().get("content-type").unwrap();
        assert_eq!(ct.to_str().unwrap(), "application/soap+xml; charset=utf-8");
    }

    #[test]
    fn fault_response_soap11_content_type() {
        use crate::wsdl::definitions::SoapVersion;
        let fault = SoapFault::sender("test");
        let response = fault_response(fault, SoapVersion::Soap11);
        let ct = response.headers().get("content-type").unwrap();
        assert_eq!(ct.to_str().unwrap(), "text/xml; charset=utf-8");
    }

    #[test]
    fn extract_body_qname_parses_namespaced_element() {
        let bytes = b"<tns:Ping xmlns:tns=\"http://example.com/test\"/>";
        let qname = extract_body_qname(bytes).unwrap();
        assert_eq!(qname.local_name, "Ping");
        assert_eq!(qname.namespace.as_deref(), Some("http://example.com/test"));
    }

    #[test]
    fn extract_body_qname_parses_unnamespaced_element() {
        let bytes = b"<Ping/>";
        let qname = extract_body_qname(bytes).unwrap();
        assert_eq!(qname.local_name, "Ping");
        assert_eq!(qname.namespace, None);
    }
}
