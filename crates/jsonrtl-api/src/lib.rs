use std::sync::atomic::{AtomicU64, Ordering};

use axum::{
    Json, Router,
    body::to_bytes,
    extract::{Extension, Request},
    http::{HeaderMap, StatusCode},
    response::{Html, IntoResponse, Response},
    routing::{get, post},
};
use jsonrtl::{
    CircuitDocument, CompileOptions, Diagnostic, KERNEL_VERSION, Kernel, KernelLimits,
    LimitDiagnostic, ParseError, SUPPORTED_SCHEMA_VERSION, SchemaDiagnostic, SourceMap,
    VerilogIdentifier,
};
use serde::Serialize;

static REQUEST_COUNTER: AtomicU64 = AtomicU64::new(1);

const APP_HTML: &str = r##"<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>Logic Kernel Workbench</title>
<link rel="icon" href="data:,">
<style>
:root{color-scheme:dark;--bg:#0f141b;--panel:#171e27;--line:#2b3745;--text:#e8edf3;--muted:#95a3b5;--accent:#5dd39e;--bad:#ff7b72;--warn:#f0c674}
*{box-sizing:border-box}body{margin:0;background:var(--bg);color:var(--text);font:14px/1.45 system-ui,-apple-system,Segoe UI,sans-serif}main{min-height:100vh;display:grid;grid-template-rows:auto 1fr;gap:16px;padding:18px}
header{display:flex;align-items:center;justify-content:space-between;gap:16px;border-bottom:1px solid var(--line);padding-bottom:14px}h1{font-size:20px;margin:0;letter-spacing:0}.mark{display:flex;align-items:center;gap:12px}.mark svg{width:80px;height:38px}
.status{color:var(--muted)}.grid{display:grid;grid-template-columns:minmax(0,1fr) minmax(0,1fr);gap:16px;min-height:0}.pane{background:var(--panel);border:1px solid var(--line);border-radius:8px;display:grid;grid-template-rows:auto 1fr;min-height:0}
.bar{display:flex;align-items:center;justify-content:space-between;gap:8px;padding:10px 12px;border-bottom:1px solid var(--line)}.actions{display:flex;gap:8px;flex-wrap:wrap}
button{border:1px solid var(--line);border-radius:6px;background:#202a36;color:var(--text);padding:8px 10px;cursor:pointer}button.primary{border-color:#2f8f68;background:#1c3b31}button:disabled{opacity:.55;cursor:wait}
textarea,pre{width:100%;height:100%;min-height:0;margin:0;border:0;background:transparent;color:var(--text);font:12px/1.45 ui-monospace,SFMono-Regular,Menlo,Consolas,monospace;padding:12px;resize:none;outline:none;overflow:auto}
.valid{color:var(--accent)}.invalid{color:var(--bad)}.warn{color:var(--warn)}@media(max-width:850px){main{padding:12px}.grid{grid-template-columns:1fr;grid-auto-rows:minmax(320px,1fr)}header{align-items:flex-start;flex-direction:column}}
</style>
</head>
<body>
<main>
<header>
  <div class="mark">
    <svg viewBox="0 0 160 76" role="img" aria-label="AND gate circuit">
      <path d="M18 20h42M18 56h42M60 12h30a30 30 0 0 1 0 60H60zM120 42h28" fill="none" stroke="#5dd39e" stroke-width="5" stroke-linecap="round"/>
      <circle cx="18" cy="20" r="5" fill="#95a3b5"/><circle cx="18" cy="56" r="5" fill="#95a3b5"/><circle cx="148" cy="42" r="5" fill="#95a3b5"/>
    </svg>
    <h1>Logic Kernel Workbench</h1>
  </div>
  <div id="status" class="status">Ready</div>
</header>
<section class="grid">
  <div class="pane">
    <div class="bar">
      <span>Circuit JSON</span>
      <div class="actions">
        <button id="load">Load Sample</button>
        <button id="validate" class="primary">Validate</button>
        <button id="compile" class="primary">Compile</button>
      </div>
    </div>
    <textarea id="input" spellcheck="false"></textarea>
  </div>
  <div class="pane">
    <div class="bar">
      <span>Kernel Output</span>
      <div id="badge" class="status">No run yet</div>
    </div>
    <pre id="output"></pre>
  </div>
</section>
</main>
<script>
const sample = {
  schemaVersion: "1.0",
  circuit: {
    id: "minimal-and",
    name: "Minimal AND",
    ports: [
      { id: "input-a", name: "a", direction: "input", width: 1, netId: "net-a" },
      { id: "input-b", name: "b", direction: "input", width: 1, netId: "net-b" },
      { id: "output-y", name: "y", direction: "output", width: 1, netId: "net-y" }
    ],
    components: [
      { id: "and-1", name: "and_gate", type: "AND", width: 1, connections: { A: "net-a", B: "net-b", Y: "net-y" }, parameters: {} }
    ],
    nets: [
      { id: "net-a", name: "a", width: 1 },
      { id: "net-b", name: "b", width: 1 },
      { id: "net-y", name: "y", width: 1 }
    ]
  }
};
const input = document.getElementById("input");
const output = document.getElementById("output");
const status = document.getElementById("status");
const badge = document.getElementById("badge");
const buttons = [...document.querySelectorAll("button")];
function setBusy(value){buttons.forEach(button => button.disabled = value)}
function show(kind, body){badge.className = kind; badge.textContent = body; status.textContent = body}
async function post(path){
  setBusy(true);
  try {
    const response = await fetch(path, {method:"POST", headers:{"content-type":"application/json"}, body:input.value});
    const json = await response.json();
    if (json.verilog) output.textContent = json.verilog;
    else output.textContent = JSON.stringify(json, null, 2);
    show(response.ok && json.valid !== false ? "valid" : "invalid", `${response.status} ${response.statusText}`);
  } catch (error) {
    output.textContent = String(error);
    show("invalid", "Request failed");
  } finally {
    setBusy(false);
  }
}
document.getElementById("load").addEventListener("click", () => {
  input.value = JSON.stringify(sample, null, 2);
  output.textContent = "";
  show("status", "Sample loaded");
});
document.getElementById("validate").addEventListener("click", () => post("/api/v1/validate"));
document.getElementById("compile").addEventListener("click", () => post("/api/v1/compile/verilog"));
input.value = JSON.stringify(sample, null, 2);
</script>
</body>
</html>"##;

#[derive(Debug, Serialize)]
#[serde(untagged)]
enum ApiDiagnostic {
    Diagnostic(Diagnostic),
    SchemaDiagnostic(SchemaDiagnostic),
    LimitDiagnostic(LimitDiagnostic),
}

fn request_id() -> String {
    let pid = std::process::id();
    let seq = REQUEST_COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("req-{pid}-{seq:012x}")
}

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

pub struct RouterBuilder {
    kernel_limits: KernelLimits,
    request_body_limit: usize,
}

impl RouterBuilder {
    #[must_use]
    pub fn new() -> Self {
        Self {
            kernel_limits: KernelLimits::default(),
            request_body_limit: KernelLimits::default().max_document_bytes,
        }
    }

    #[must_use]
    pub fn kernel_limits(mut self, limits: KernelLimits) -> Self {
        self.kernel_limits = limits;
        self
    }

    #[must_use]
    pub fn request_body_limit(mut self, bytes: usize) -> Self {
        self.request_body_limit = bytes;
        self
    }

    pub fn finish(self) -> Router {
        let kernel = Kernel::new(self.kernel_limits);
        Router::new()
            .route("/", get(site))
            .route("/health", get(health))
            .route("/api/v1/validate", post(validate))
            .route("/api/v1/compile/verilog", post(compile_verilog))
            .layer(Extension((kernel, self.request_body_limit)))
    }
}

impl Default for RouterBuilder {
    fn default() -> Self {
        Self::new()
    }
}

pub fn router() -> Router {
    RouterBuilder::new().finish()
}

// ---------------------------------------------------------------------------
// Wire types
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct HealthResponse {
    success: bool,
    live: bool,
    ready: bool,
    external_tools_required: bool,
    schema_version: &'static str,
    compiler_version: &'static str,
    request_id: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ValidateResponse {
    success: bool,
    valid: bool,
    diagnostics: Vec<ApiDiagnostic>,
    schema_version: &'static str,
    compiler_version: &'static str,
    request_id: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct CompileResponse {
    success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    module_name: Option<VerilogIdentifier>,
    #[serde(skip_serializing_if = "Option::is_none")]
    verilog: Option<String>,
    diagnostics: Vec<ApiDiagnostic>,
    #[serde(skip_serializing_if = "Option::is_none")]
    source_map: Option<SourceMap>,
    schema_version: &'static str,
    compiler_version: &'static str,
    request_id: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ErrorBody {
    category: &'static str,
    message: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ErrorResponse {
    success: bool,
    error: ErrorBody,
    diagnostics: Vec<ApiDiagnostic>,
    schema_version: &'static str,
    compiler_version: &'static str,
    request_id: String,
}

// ---------------------------------------------------------------------------
// Response helpers
// ---------------------------------------------------------------------------

fn with_request_id(id: &str, response: impl IntoResponse) -> Response {
    let mut resp = response.into_response();
    if let Ok(value) = axum::http::HeaderValue::from_str(id) {
        resp.headers_mut().insert("x-request-id", value);
    }
    resp
}

fn error_response(
    status: StatusCode,
    id: &str,
    category: &'static str,
    message: impl Into<String>,
) -> Response {
    let body = ErrorResponse {
        success: false,
        error: ErrorBody {
            category,
            message: message.into(),
        },
        diagnostics: Vec::new(),
        schema_version: SUPPORTED_SCHEMA_VERSION,
        compiler_version: KERNEL_VERSION,
        request_id: id.to_owned(),
    };
    with_request_id(id, (status, Json(body)))
}

fn error_response_with_diagnostics(
    status: StatusCode,
    id: &str,
    category: &'static str,
    message: impl Into<String>,
    diagnostics: Vec<ApiDiagnostic>,
) -> Response {
    let body = ErrorResponse {
        success: false,
        error: ErrorBody {
            category,
            message: message.into(),
        },
        diagnostics,
        schema_version: SUPPORTED_SCHEMA_VERSION,
        compiler_version: KERNEL_VERSION,
        request_id: id.to_owned(),
    };
    with_request_id(id, (status, Json(body)))
}

// ---------------------------------------------------------------------------
// Content-type validation
// ---------------------------------------------------------------------------

fn require_json_content_type(headers: &HeaderMap) -> Result<(), String> {
    let Some(value) = headers.get("content-type") else {
        return Err("Missing Content-Type header.".to_owned());
    };

    let Ok(text) = value.to_str() else {
        return Err("Content-Type header is not valid ASCII.".to_owned());
    };

    let media_type = text
        .split(';')
        .next()
        .unwrap_or("")
        .trim()
        .to_ascii_lowercase();

    let is_json = media_type == "application/json"
        || (media_type.starts_with("application/") && media_type.ends_with("+json"));

    if !is_json {
        return Err(format!(
            "Expected application/json or *+json, received '{media_type}'."
        ));
    }

    for param in text.split(';').skip(1) {
        let param = param.trim().to_ascii_lowercase();
        if let Some((key, val)) = param.split_once('=') {
            if key.trim() == "charset" && val.trim() != "utf-8" {
                return Err(format!("Expected charset=utf-8, received '{val}'."));
            }
        }
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Body length error detection
// ---------------------------------------------------------------------------

fn is_length_limit_error(err: &(dyn std::error::Error + 'static)) -> bool {
    if err
        .downcast_ref::<http_body_util::LengthLimitError>()
        .is_some()
    {
        return true;
    }
    err.source().is_some_and(is_length_limit_error)
}

// ---------------------------------------------------------------------------
// Parse-error classification
// ---------------------------------------------------------------------------

fn classify_parse_error(error: &ParseError) -> (StatusCode, &'static str, String) {
    match error {
        ParseError::DocumentTooLarge { actual, maximum } => (
            StatusCode::PAYLOAD_TOO_LARGE,
            "DOCUMENT_TOO_LARGE",
            format!("Document is {actual} bytes; configured maximum is {maximum} bytes."),
        ),
        ParseError::MalformedJson {
            message,
            line,
            column,
        } => (
            StatusCode::BAD_REQUEST,
            "MALFORMED_JSON",
            format!("Malformed JSON at line {line}, column {column}: {message}."),
        ),
        ParseError::UnsupportedSchemaVersion { found, supported } => (
            StatusCode::BAD_REQUEST,
            "UNSUPPORTED_VERSION",
            format!(
                "Unsupported schema version '{found}'; supported versions are {}.",
                supported.join(", ")
            ),
        ),
        ParseError::Schema { .. } => (
            StatusCode::BAD_REQUEST,
            "SCHEMA_VALIDATION",
            format!(
                "Document does not satisfy canonical schema v{}.",
                SUPPORTED_SCHEMA_VERSION
            ),
        ),
        ParseError::ResourceLimits { .. } => (
            StatusCode::PAYLOAD_TOO_LARGE,
            "LOGICAL_LIMITS",
            "Document exceeds configured kernel limits.".to_owned(),
        ),
        ParseError::InvalidEmbeddedSchema { .. } | ParseError::Deserialization { .. } => (
            StatusCode::INTERNAL_SERVER_ERROR,
            "INTERNAL_ERROR",
            "An internal error occurred.".to_owned(),
        ),
    }
}

fn parse_error_diagnostics(error: &ParseError) -> Vec<ApiDiagnostic> {
    match error {
        ParseError::Schema { diagnostics } => diagnostics
            .iter()
            .map(|d| ApiDiagnostic::SchemaDiagnostic(d.clone()))
            .collect(),
        ParseError::ResourceLimits { diagnostics } => diagnostics
            .iter()
            .map(|d| ApiDiagnostic::LimitDiagnostic(d.clone()))
            .collect(),
        _ => Vec::new(),
    }
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

async fn site() -> Html<&'static str> {
    Html(APP_HTML)
}

async fn health() -> Response {
    let id = request_id();
    let body = HealthResponse {
        success: true,
        live: true,
        ready: true,
        external_tools_required: false,
        schema_version: SUPPORTED_SCHEMA_VERSION,
        compiler_version: KERNEL_VERSION,
        request_id: id.clone(),
    };
    with_request_id(&id, Json(body))
}

async fn validate(
    Extension((kernel, max_body)): Extension<(Kernel, usize)>,
    request: Request,
) -> Response {
    let id = request_id();

    let content_type_error = require_json_content_type(request.headers());
    if let Err(msg) = content_type_error {
        return error_response(
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            &id,
            "UNSUPPORTED_MEDIA_TYPE",
            msg,
        );
    }

    let body_bytes = match to_bytes(request.into_body(), max_body).await {
        Ok(bytes) => bytes,
        Err(err) => {
            if is_length_limit_error(&err) {
                return error_response(
                    StatusCode::PAYLOAD_TOO_LARGE,
                    &id,
                    "DOCUMENT_TOO_LARGE",
                    format!("Request body exceeds the {} byte limit.", max_body),
                );
            }
            return error_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                &id,
                "INTERNAL_ERROR",
                "Failed to read request body.".to_owned(),
            );
        }
    };

    let text = match std::str::from_utf8(&body_bytes) {
        Ok(text) => text.to_string(),
        Err(_) => {
            return error_response(
                StatusCode::BAD_REQUEST,
                &id,
                "MALFORMED_JSON",
                "Request body is not valid UTF-8.".to_owned(),
            );
        }
    };

    let document = match CircuitDocument::from_json_with_limits(&text, kernel.limits()) {
        Ok(doc) => doc,
        Err(error) => {
            let diagnostics = parse_error_diagnostics(&error);
            let (status, category, message) = classify_parse_error(&error);
            if diagnostics.is_empty() {
                return error_response(status, &id, category, message);
            }
            return error_response_with_diagnostics(status, &id, category, message, diagnostics);
        }
    };

    let report = kernel.validate(&document);
    let diagnostics: Vec<ApiDiagnostic> = report
        .diagnostics()
        .iter()
        .map(|d| ApiDiagnostic::Diagnostic(d.clone()))
        .collect();

    with_request_id(
        &id,
        Json(ValidateResponse {
            success: true,
            valid: !report.has_errors(),
            diagnostics,
            schema_version: SUPPORTED_SCHEMA_VERSION,
            compiler_version: KERNEL_VERSION,
            request_id: id.clone(),
        }),
    )
}

async fn compile_verilog(
    Extension((kernel, max_body)): Extension<(Kernel, usize)>,
    request: Request,
) -> Response {
    let id = request_id();

    let content_type_error = require_json_content_type(request.headers());
    if let Err(msg) = content_type_error {
        return error_response(
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            &id,
            "UNSUPPORTED_MEDIA_TYPE",
            msg,
        );
    }

    let body_bytes = match to_bytes(request.into_body(), max_body).await {
        Ok(bytes) => bytes,
        Err(err) => {
            if is_length_limit_error(&err) {
                return error_response(
                    StatusCode::PAYLOAD_TOO_LARGE,
                    &id,
                    "DOCUMENT_TOO_LARGE",
                    format!("Request body exceeds the {} byte limit.", max_body),
                );
            }
            return error_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                &id,
                "INTERNAL_ERROR",
                "Failed to read request body.".to_owned(),
            );
        }
    };

    let text = match std::str::from_utf8(&body_bytes) {
        Ok(text) => text.to_string(),
        Err(_) => {
            return error_response(
                StatusCode::BAD_REQUEST,
                &id,
                "MALFORMED_JSON",
                "Request body is not valid UTF-8.".to_owned(),
            );
        }
    };

    let document = match CircuitDocument::from_json_with_limits(&text, kernel.limits()) {
        Ok(doc) => doc,
        Err(error) => {
            let diagnostics = parse_error_diagnostics(&error);
            let (status, category, message) = classify_parse_error(&error);
            if diagnostics.is_empty() {
                return error_response(status, &id, category, message);
            }
            return error_response_with_diagnostics(status, &id, category, message, diagnostics);
        }
    };

    let result = kernel.compile_verilog(&document, &CompileOptions::default());
    let diagnostics: Vec<ApiDiagnostic> = result
        .diagnostics
        .diagnostics()
        .iter()
        .map(|d| ApiDiagnostic::Diagnostic(d.clone()))
        .collect();

    if result.has_output() || !result.diagnostics.has_errors() {
        with_request_id(
            &id,
            Json(CompileResponse {
                success: true,
                module_name: result.module_name,
                verilog: result.verilog,
                diagnostics,
                source_map: result.source_map,
                schema_version: SUPPORTED_SCHEMA_VERSION,
                compiler_version: KERNEL_VERSION,
                request_id: id.clone(),
            }),
        )
    } else {
        error_response_with_diagnostics(
            StatusCode::UNPROCESSABLE_ENTITY,
            &id,
            "SEMANTIC_ERROR",
            "Document has semantic errors that prevent compilation.",
            diagnostics,
        )
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use tower::Service;

    fn app() -> Router {
        router()
    }

    async fn call(app: &mut Router, req: Request<Body>) -> axum::response::Response {
        Service::call(app, req).await.unwrap()
    }

    async fn body_json(resp: axum::response::Response) -> serde_json::Value {
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        serde_json::from_slice(&bytes).unwrap()
    }

    async fn body_json_and_request_id(
        resp: axum::response::Response,
    ) -> (serde_json::Value, String) {
        let request_id = resp
            .headers()
            .get("x-request-id")
            .and_then(|v| v.to_str().ok())
            .unwrap_or_default()
            .to_owned();
        let json = body_json(resp).await;
        (json, request_id)
    }

    const MINIMAL_AND: &str = include_str!("../../../tests/fixtures/valid/minimal-and.json");
    const COMBINED_INVALID: &str =
        include_str!("../../../tests/fixtures/semantic/combined-invalid.json");
    const MALFORMED: &str = include_str!("../../../tests/fixtures/invalid/malformed.json");
    const UNSUPPORTED_VERSION: &str =
        include_str!("../../../tests/fixtures/invalid/unsupported-version.json");
    const MISSING_REQUIRED: &str =
        include_str!("../../../tests/fixtures/invalid/missing-required-field.json");

    #[tokio::test]
    async fn site_returns_kernel_workbench() {
        let mut app = app();
        let req = Request::builder().uri("/").body(Body::empty()).unwrap();
        let resp = call(&mut app, req).await;
        assert_eq!(resp.status(), StatusCode::OK);
        assert_eq!(
            resp.headers()
                .get("content-type")
                .and_then(|value| value.to_str().ok()),
            Some("text/html; charset=utf-8")
        );

        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let html = std::str::from_utf8(&bytes).unwrap();
        assert!(html.contains("Logic Kernel Workbench"));
        assert!(html.contains("/api/v1/validate"));
        assert!(html.contains("/api/v1/compile/verilog"));
    }

    // -- health ----------------------------------------------------------

    #[tokio::test]
    async fn health_returns_200_with_required_fields() {
        let mut app = app();
        let req = Request::builder()
            .uri("/health")
            .body(Body::empty())
            .unwrap();
        let resp = call(&mut app, req).await;
        assert_eq!(resp.status(), StatusCode::OK);
        let json = body_json(resp).await;
        assert_eq!(json["success"], true);
        assert_eq!(json["live"], true);
        assert_eq!(json["ready"], true);
        assert_eq!(json["externalToolsRequired"], false);
        assert!(json["schemaVersion"].is_string());
        assert!(json["compilerVersion"].is_string());
        assert!(json["requestId"].is_string());
    }

    #[tokio::test]
    async fn health_header_matches_body_id() {
        let mut app = app();
        let req = Request::builder()
            .uri("/health")
            .body(Body::empty())
            .unwrap();
        let resp = call(&mut app, req).await;
        let (json, rid) = body_json_and_request_id(resp).await;
        assert_eq!(rid, json["requestId"].as_str().unwrap());
    }

    // -- validate -------------------------------------------------------

    #[tokio::test]
    async fn validate_valid_returns_200_with_all_fields() {
        let mut app = app();
        let req = Request::builder()
            .method("POST")
            .uri("/api/v1/validate")
            .header("content-type", "application/json")
            .body(Body::from(MINIMAL_AND))
            .unwrap();
        let resp = call(&mut app, req).await;
        assert_eq!(resp.status(), StatusCode::OK);
        let (json, rid) = body_json_and_request_id(resp).await;
        assert_eq!(json["success"], true);
        assert_eq!(json["valid"], true);
        assert!(json["diagnostics"].is_array());
        assert!(json["schemaVersion"].is_string());
        assert!(json["compilerVersion"].is_string());
        assert!(json["requestId"].is_string());
        assert!(json.get("error").is_none());
        assert_eq!(rid, json["requestId"].as_str().unwrap());
    }

    #[tokio::test]
    async fn validate_semantic_invalid_returns_200_valid_false() {
        let mut app = app();
        let req = Request::builder()
            .method("POST")
            .uri("/api/v1/validate")
            .header("content-type", "application/json")
            .body(Body::from(COMBINED_INVALID))
            .unwrap();
        let resp = call(&mut app, req).await;
        assert_eq!(resp.status(), StatusCode::OK);
        let json = body_json(resp).await;
        assert_eq!(json["success"], true);
        assert_eq!(json["valid"], false);
        assert!(!json["diagnostics"].as_array().unwrap().is_empty());
    }

    #[tokio::test]
    async fn validate_diagnostics_match_kernel_directly() {
        let mut app = app();
        let req = Request::builder()
            .method("POST")
            .uri("/api/v1/validate")
            .header("content-type", "application/json")
            .body(Body::from(COMBINED_INVALID))
            .unwrap();
        let resp = call(&mut app, req).await;
        let json = body_json(resp).await;

        let document = CircuitDocument::from_json(COMBINED_INVALID).expect("fixture parses");
        let report = Kernel::default().validate(&document);
        let expected = serde_json::to_value(report.diagnostics()).unwrap();
        assert_eq!(json["diagnostics"], expected);
    }

    // -- compile --------------------------------------------------------

    #[tokio::test]
    async fn compile_valid_returns_200_with_all_fields() {
        let mut app = app();
        let req = Request::builder()
            .method("POST")
            .uri("/api/v1/compile/verilog")
            .header("content-type", "application/json")
            .body(Body::from(MINIMAL_AND))
            .unwrap();
        let resp = call(&mut app, req).await;
        assert_eq!(resp.status(), StatusCode::OK);
        let (json, rid) = body_json_and_request_id(resp).await;
        assert_eq!(json["success"], true);
        assert!(json["moduleName"].is_string());
        assert!(json["verilog"].is_string());
        assert!(json["diagnostics"].is_array());
        assert!(json["sourceMap"].is_object());
        assert!(json["schemaVersion"].is_string());
        assert!(json["compilerVersion"].is_string());
        assert!(json["requestId"].is_string());
        assert!(json.get("error").is_none());
        assert_eq!(rid, json["requestId"].as_str().unwrap());
    }

    #[tokio::test]
    async fn compile_verilog_matches_golden() {
        let mut app = app();
        let req = Request::builder()
            .method("POST")
            .uri("/api/v1/compile/verilog")
            .header("content-type", "application/json")
            .body(Body::from(MINIMAL_AND))
            .unwrap();
        let resp = call(&mut app, req).await;
        let json = body_json(resp).await;
        let expected_verilog = include_str!("../../../tests/golden/minimal-and.v");
        assert_eq!(json["verilog"].as_str().unwrap(), expected_verilog);
    }

    #[tokio::test]
    async fn compile_semantic_error_returns_422_no_output() {
        let mut app = app();
        let req = Request::builder()
            .method("POST")
            .uri("/api/v1/compile/verilog")
            .header("content-type", "application/json")
            .body(Body::from(COMBINED_INVALID))
            .unwrap();
        let resp = call(&mut app, req).await;
        assert_eq!(resp.status(), StatusCode::UNPROCESSABLE_ENTITY);
        let (json, rid) = body_json_and_request_id(resp).await;
        assert_eq!(json["success"], false);
        assert_eq!(json["error"]["category"], "SEMANTIC_ERROR");
        assert!(json.get("moduleName").is_none());
        assert!(json.get("verilog").is_none());
        assert!(json.get("sourceMap").is_none());
        assert!(json["diagnostics"].is_array());
        assert!(json["schemaVersion"].is_string());
        assert!(json["compilerVersion"].is_string());
        assert_eq!(rid, json["requestId"].as_str().unwrap());
    }

    // -- malformed / schema / version 400 -------------------------------

    #[tokio::test]
    async fn malformed_body_returns_400_with_error_envelope() {
        let mut app = app();
        let req = Request::builder()
            .method("POST")
            .uri("/api/v1/validate")
            .header("content-type", "application/json")
            .body(Body::from(MALFORMED))
            .unwrap();
        let resp = call(&mut app, req).await;
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
        let (json, rid) = body_json_and_request_id(resp).await;
        assert_eq!(json["success"], false);
        assert_eq!(json["error"]["category"], "MALFORMED_JSON");
        assert!(json["error"]["message"].is_string());
        assert!(json["schemaVersion"].is_string());
        assert!(json["compilerVersion"].is_string());
        assert!(json["requestId"].is_string());
        assert!(json.get("versions").is_none());
        assert!(json.get("category").is_none());
        assert!(json.get("message").is_none());
        assert_eq!(rid, json["requestId"].as_str().unwrap());
    }

    #[tokio::test]
    async fn unsupported_version_returns_400() {
        let mut app = app();
        let req = Request::builder()
            .method("POST")
            .uri("/api/v1/validate")
            .header("content-type", "application/json")
            .body(Body::from(UNSUPPORTED_VERSION))
            .unwrap();
        let resp = call(&mut app, req).await;
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
        let (json, rid) = body_json_and_request_id(resp).await;
        assert_eq!(json["error"]["category"], "UNSUPPORTED_VERSION");
        assert_eq!(rid, json["requestId"].as_str().unwrap());
    }

    #[tokio::test]
    async fn schema_error_returns_400_with_native_diagnostics() {
        let mut app = app();
        let req = Request::builder()
            .method("POST")
            .uri("/api/v1/validate")
            .header("content-type", "application/json")
            .body(Body::from(MISSING_REQUIRED))
            .unwrap();
        let resp = call(&mut app, req).await;
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
        let (json, rid) = body_json_and_request_id(resp).await;
        assert_eq!(json["error"]["category"], "SCHEMA_VALIDATION");
        let diagnostics = json["diagnostics"].as_array().unwrap();
        assert!(!diagnostics.is_empty());
        let first = &diagnostics[0];
        assert!(
            first.get("code").is_some(),
            "SchemaDiagnostic must have code"
        );
        assert!(
            first.get("jsonPath").is_some(),
            "SchemaDiagnostic must have jsonPath"
        );
        assert!(
            first.get("schemaPath").is_some(),
            "SchemaDiagnostic must have schemaPath"
        );
        assert_eq!(rid, json["requestId"].as_str().unwrap());
    }

    // -- content-type 415 -----------------------------------------------

    #[tokio::test]
    async fn content_type_missing_returns_415() {
        let mut app = app();
        let req = Request::builder()
            .method("POST")
            .uri("/api/v1/validate")
            .body(Body::from(MINIMAL_AND))
            .unwrap();
        let resp = call(&mut app, req).await;
        assert_eq!(resp.status(), StatusCode::UNSUPPORTED_MEDIA_TYPE);
        let (json, rid) = body_json_and_request_id(resp).await;
        assert_eq!(json["error"]["category"], "UNSUPPORTED_MEDIA_TYPE");
        assert_eq!(rid, json["requestId"].as_str().unwrap());
    }

    #[tokio::test]
    async fn content_type_wrong_returns_415() {
        let mut app = app();
        let req = Request::builder()
            .method("POST")
            .uri("/api/v1/validate")
            .header("content-type", "text/plain")
            .body(Body::from(MINIMAL_AND))
            .unwrap();
        let resp = call(&mut app, req).await;
        assert_eq!(resp.status(), StatusCode::UNSUPPORTED_MEDIA_TYPE);
    }

    #[tokio::test]
    async fn content_type_charset_utf8_succeeds() {
        let mut app = app();
        let req = Request::builder()
            .method("POST")
            .uri("/api/v1/validate")
            .header("content-type", "application/json; charset=utf-8")
            .body(Body::from(MINIMAL_AND))
            .unwrap();
        let resp = call(&mut app, req).await;
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn content_type_charset_wrong_returns_415() {
        let mut app = app();
        let req = Request::builder()
            .method("POST")
            .uri("/api/v1/validate")
            .header("content-type", "application/json; charset=ascii")
            .body(Body::from(MINIMAL_AND))
            .unwrap();
        let resp = call(&mut app, req).await;
        assert_eq!(resp.status(), StatusCode::UNSUPPORTED_MEDIA_TYPE);
    }

    #[tokio::test]
    async fn content_type_plus_json_suffix_accepted() {
        let mut app = app();
        let req = Request::builder()
            .method("POST")
            .uri("/api/v1/validate")
            .header("content-type", "application/ld+json")
            .body(Body::from(MINIMAL_AND))
            .unwrap();
        let resp = call(&mut app, req).await;
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn content_type_vnd_json_suffix_accepted() {
        let mut app = app();
        let req = Request::builder()
            .method("POST")
            .uri("/api/v1/validate")
            .header("content-type", "application/vnd.example.v1+json")
            .body(Body::from(MINIMAL_AND))
            .unwrap();
        let resp = call(&mut app, req).await;
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn content_type_text_plus_json_returns_415() {
        let mut app = app();
        let req = Request::builder()
            .method("POST")
            .uri("/api/v1/validate")
            .header("content-type", "text/foo+json")
            .body(Body::from(MINIMAL_AND))
            .unwrap();
        let resp = call(&mut app, req).await;
        assert_eq!(resp.status(), StatusCode::UNSUPPORTED_MEDIA_TYPE);
    }

    // -- body size 413 ---------------------------------------------------

    #[tokio::test]
    async fn request_body_limit_returns_413_json() {
        let mut app = RouterBuilder::new().request_body_limit(10).finish();
        let req = Request::builder()
            .method("POST")
            .uri("/api/v1/validate")
            .header("content-type", "application/json")
            .body(Body::from(MINIMAL_AND))
            .unwrap();
        let resp = call(&mut app, req).await;
        assert_eq!(resp.status(), StatusCode::PAYLOAD_TOO_LARGE);
        let (json, rid) = body_json_and_request_id(resp).await;
        assert_eq!(json["success"], false);
        assert_eq!(json["error"]["category"], "DOCUMENT_TOO_LARGE");
        assert!(json["error"]["message"].is_string());
        assert!(json["requestId"].is_string());
        assert_eq!(rid, json["requestId"].as_str().unwrap());
    }

    #[tokio::test]
    async fn logical_limits_return_413() {
        let mut app = RouterBuilder::new()
            .kernel_limits(KernelLimits {
                max_components: 0,
                ..KernelLimits::default()
            })
            .finish();
        let req = Request::builder()
            .method("POST")
            .uri("/api/v1/validate")
            .header("content-type", "application/json")
            .body(Body::from(include_str!(
                "../../../tests/fixtures/invalid/over-limit-components.json"
            )))
            .unwrap();
        let resp = call(&mut app, req).await;
        assert_eq!(resp.status(), StatusCode::PAYLOAD_TOO_LARGE);
        let (json, rid) = body_json_and_request_id(resp).await;
        assert_eq!(json["error"]["category"], "LOGICAL_LIMITS");
        assert_eq!(rid, json["requestId"].as_str().unwrap());
    }

    #[tokio::test]
    async fn logical_limit_diagnostics_are_native_limit_diagnostics() {
        let mut app = RouterBuilder::new()
            .kernel_limits(KernelLimits {
                max_components: 0,
                ..KernelLimits::default()
            })
            .finish();
        let req = Request::builder()
            .method("POST")
            .uri("/api/v1/validate")
            .header("content-type", "application/json")
            .body(Body::from(include_str!(
                "../../../tests/fixtures/invalid/over-limit-components.json"
            )))
            .unwrap();
        let resp = call(&mut app, req).await;
        let json = body_json(resp).await;
        let diagnostics = json["diagnostics"].as_array().unwrap();
        assert!(!diagnostics.is_empty());
        let first = &diagnostics[0];
        assert!(
            first.get("code").is_some(),
            "LimitDiagnostic must have code, not INTERNAL_INVARIANT"
        );
        assert!(
            first.get("jsonPath").is_some(),
            "LimitDiagnostic must have jsonPath"
        );
        assert!(
            first.get("actual").is_some(),
            "LimitDiagnostic must have actual"
        );
        assert!(
            first.get("maximum").is_some(),
            "LimitDiagnostic must have maximum"
        );
    }

    // -- concurrent unique IDs ------------------------------------------

    #[tokio::test]
    async fn concurrent_requests_have_unique_ids() {
        let router = router();
        let mut handles = Vec::new();
        for _ in 0..50 {
            let mut app = router.clone();
            handles.push(tokio::spawn(async move {
                let req = Request::builder()
                    .method("POST")
                    .uri("/api/v1/validate")
                    .header("content-type", "application/json")
                    .body(Body::from(MINIMAL_AND))
                    .unwrap();
                let resp = Service::call(&mut app, req).await.unwrap();
                let json = body_json(resp).await;
                json["requestId"].as_str().unwrap().to_owned()
            }));
        }
        let mut ids = std::collections::HashSet::new();
        for handle in handles {
            let id = handle.await.unwrap();
            assert!(ids.insert(id), "duplicate request ID");
        }
    }

    // -- inbound ID ignored ---------------------------------------------

    #[tokio::test]
    async fn inbound_x_request_id_is_ignored() {
        let mut app = app();
        let req = Request::builder()
            .method("POST")
            .uri("/api/v1/validate")
            .header("content-type", "application/json")
            .header("x-request-id", "client-supplied-id")
            .body(Body::from(MINIMAL_AND))
            .unwrap();
        let resp = call(&mut app, req).await;
        let (json, rid) = body_json_and_request_id(resp).await;
        let id = json["requestId"].as_str().unwrap();
        assert_ne!(id, "client-supplied-id", "inbound ID must be ignored");
        assert_eq!(rid, json["requestId"].as_str().unwrap());
    }

    // -- header/body ID match across all response categories ------------

    #[tokio::test]
    async fn header_body_id_match_malformed() {
        let mut app = app();
        let req = Request::builder()
            .method("POST")
            .uri("/api/v1/validate")
            .header("content-type", "application/json")
            .body(Body::from(MALFORMED))
            .unwrap();
        let resp = call(&mut app, req).await;
        let (json, rid) = body_json_and_request_id(resp).await;
        assert_eq!(rid, json["requestId"].as_str().unwrap());
    }

    #[tokio::test]
    async fn header_body_id_match_415() {
        let mut app = app();
        let req = Request::builder()
            .method("POST")
            .uri("/api/v1/validate")
            .body(Body::from(MINIMAL_AND))
            .unwrap();
        let resp = call(&mut app, req).await;
        let (json, rid) = body_json_and_request_id(resp).await;
        assert_eq!(rid, json["requestId"].as_str().unwrap());
    }

    #[tokio::test]
    async fn header_body_id_match_413_body_limit() {
        let mut app = RouterBuilder::new().request_body_limit(10).finish();
        let req = Request::builder()
            .method("POST")
            .uri("/api/v1/validate")
            .header("content-type", "application/json")
            .body(Body::from(MINIMAL_AND))
            .unwrap();
        let resp = call(&mut app, req).await;
        let (json, rid) = body_json_and_request_id(resp).await;
        assert_eq!(rid, json["requestId"].as_str().unwrap());
    }

    // -- editorMetadata invariance --------------------------------------

    #[tokio::test]
    async fn editor_metadata_does_not_affect_validate_output() {
        let mut app = app();
        let doc_with_meta = serde_json::json!({
            "schemaVersion": "1.0",
            "circuit": {
                "id": "test",
                "name": "Test",
                "ports": [],
                "components": [],
                "nets": []
            },
            "editorMetadata": { "zoom": 4.5 }
        });
        let doc_without = serde_json::json!({
            "schemaVersion": "1.0",
            "circuit": {
                "id": "test",
                "name": "Test",
                "ports": [],
                "components": [],
                "nets": []
            }
        });

        let req1 = Request::builder()
            .method("POST")
            .uri("/api/v1/validate")
            .header("content-type", "application/json")
            .body(Body::from(doc_with_meta.to_string()))
            .unwrap();
        let resp1 = call(&mut app, req1).await;
        let json1 = body_json(resp1).await;

        let req2 = Request::builder()
            .method("POST")
            .uri("/api/v1/validate")
            .header("content-type", "application/json")
            .body(Body::from(doc_without.to_string()))
            .unwrap();
        let resp2 = call(&mut app, req2).await;
        let json2 = body_json(resp2).await;

        assert_eq!(json1["diagnostics"], json2["diagnostics"]);
    }

    // -- no default CORS header -----------------------------------------

    #[tokio::test]
    async fn no_default_cors_header() {
        let mut app = app();
        let req = Request::builder()
            .uri("/health")
            .body(Body::empty())
            .unwrap();
        let resp = call(&mut app, req).await;
        assert!(
            resp.headers().get("access-control-allow-origin").is_none(),
            "default response must not have Access-Control-Allow-Origin"
        );
    }

    // -- error envelope top-level field check ---------------------------

    #[tokio::test]
    async fn error_envelope_has_no_top_level_category_or_message() {
        let mut app = app();
        let req = Request::builder()
            .method("POST")
            .uri("/api/v1/validate")
            .header("content-type", "application/json")
            .body(Body::from(MALFORMED))
            .unwrap();
        let resp = call(&mut app, req).await;
        let json = body_json(resp).await;
        assert!(json.get("category").is_none());
        assert!(json.get("message").is_none());
        assert!(json["error"].is_object());
        assert!(json["error"]["category"].is_string());
        assert!(json["error"]["message"].is_string());
    }

    #[tokio::test]
    async fn no_versions_nesting_in_any_response() {
        let mut app = app();

        let req = Request::builder()
            .uri("/health")
            .body(Body::empty())
            .unwrap();
        let resp = call(&mut app, req).await;
        let json = body_json(resp).await;
        assert!(
            json.get("versions").is_none(),
            "health must not have top-level versions"
        );
        assert!(json["schemaVersion"].is_string());
        assert!(json["compilerVersion"].is_string());

        let req = Request::builder()
            .method("POST")
            .uri("/api/v1/compile/verilog")
            .header("content-type", "application/json")
            .body(Body::from(MINIMAL_AND))
            .unwrap();
        let resp = call(&mut app, req).await;
        let json = body_json(resp).await;
        assert!(
            json.get("versions").is_none(),
            "compile must not have top-level versions"
        );
        assert!(json["schemaVersion"].is_string());
        assert!(json["compilerVersion"].is_string());
    }

    // -- fresh default-router equivalence -------------------------------

    #[tokio::test]
    async fn fresh_default_routers_equivalent_except_request_id() {
        let mut app1 = router();
        let mut app2 = router();

        let req1 = Request::builder()
            .method("POST")
            .uri("/api/v1/validate")
            .header("content-type", "application/json")
            .body(Body::from(MINIMAL_AND))
            .unwrap();
        let resp1 = call(&mut app1, req1).await;
        let mut json1 = body_json(resp1).await;

        let req2 = Request::builder()
            .method("POST")
            .uri("/api/v1/validate")
            .header("content-type", "application/json")
            .body(Body::from(MINIMAL_AND))
            .unwrap();
        let resp2 = call(&mut app2, req2).await;
        let mut json2 = body_json(resp2).await;

        json1.as_object_mut().unwrap().remove("requestId");
        json2.as_object_mut().unwrap().remove("requestId");

        assert_eq!(json1, json2);
    }
}
