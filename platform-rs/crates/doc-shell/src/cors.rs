//! Starlette's `CORSMiddleware` as DOC configures it (`app.py`: explicit origins from
//! `BACKEND_CORS_ORIGINS`, `allow_credentials=True`, methods GET and POST, any request headers),
//! reproduced header for header so the preflight and simple-request answers match the oracle.
//! (tower-http's layer answers differently: empty body, `GET,POST`, extra `Vary` values.)

use axum::extract::{Request, State};
use axum::http::{header, HeaderMap, HeaderName, HeaderValue, Method, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};

#[derive(Clone, Debug)]
pub struct CorsPolicy {
    pub origins: Vec<String>,
}

impl CorsPolicy {
    fn allowed(&self, origin: &str) -> bool {
        self.origins.iter().any(|o| o == origin)
    }
}

const ALLOW_METHODS: [&str; 2] = ["GET", "POST"];

fn set(headers: &mut HeaderMap, name: HeaderName, value: &str) {
    if let Ok(v) = HeaderValue::from_str(value) {
        headers.insert(name, v);
    }
}

fn preflight(policy: &CorsPolicy, headers: &HeaderMap) -> Response {
    let origin = headers.get(header::ORIGIN).and_then(|v| v.to_str().ok()).unwrap_or("");
    let method = headers.get("access-control-request-method").and_then(|v| v.to_str().ok()).unwrap_or("");
    let requested_headers = headers.get("access-control-request-headers").and_then(|v| v.to_str().ok());
    let mut out = HeaderMap::new();
    set(&mut out, header::VARY, "Origin");
    set(&mut out, header::ACCESS_CONTROL_ALLOW_METHODS, &ALLOW_METHODS.join(", "));
    set(&mut out, header::ACCESS_CONTROL_MAX_AGE, "600");
    set(&mut out, header::ACCESS_CONTROL_ALLOW_CREDENTIALS, "true");
    let mut failures: Vec<&str> = Vec::new();
    if policy.allowed(origin) {
        set(&mut out, header::ACCESS_CONTROL_ALLOW_ORIGIN, origin);
    } else {
        failures.push("origin");
    }
    if !ALLOW_METHODS.contains(&method) {
        failures.push("method");
    }
    if let Some(h) = requested_headers {
        set(&mut out, header::ACCESS_CONTROL_ALLOW_HEADERS, h);
    }
    let (status, body) = if failures.is_empty() {
        (StatusCode::OK, "OK".to_string())
    } else {
        (StatusCode::BAD_REQUEST, format!("Disallowed CORS {}", failures.join(", ")))
    };
    let mut resp = (status, body).into_response();
    for (k, v) in out.iter() {
        resp.headers_mut().insert(k.clone(), v.clone());
    }
    resp.headers_mut().insert(header::CONTENT_TYPE, HeaderValue::from_static("text/plain; charset=utf-8"));
    resp
}

pub async fn cors(State(policy): State<std::sync::Arc<CorsPolicy>>, req: Request, next: Next) -> Response {
    let origin = req.headers().get(header::ORIGIN).and_then(|v| v.to_str().ok()).map(str::to_string);
    let is_preflight = req.method() == Method::OPTIONS && req.headers().contains_key("access-control-request-method");
    let Some(origin) = origin else {
        return next.run(req).await;
    };
    if is_preflight {
        return preflight(&policy, req.headers());
    }
    let mut resp = next.run(req).await;
    set(resp.headers_mut(), header::ACCESS_CONTROL_ALLOW_CREDENTIALS, "true");
    if policy.allowed(&origin) {
        set(resp.headers_mut(), header::ACCESS_CONTROL_ALLOW_ORIGIN, &origin);
        resp.headers_mut().append(header::VARY, HeaderValue::from_static("Origin"));
    }
    resp
}
