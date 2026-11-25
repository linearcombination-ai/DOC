//! Routes, their OpenAPI annotations, error mapping and the FastAPI-compatible fallbacks.

use std::sync::Arc;

use axum::extract::{Path, Request, State};
use axum::http::{header, HeaderMap, HeaderValue, Method, StatusCode};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::Json;
use doc_acquire::GitAcquirer;
use doc_app::{App, AppError};
use doc_catalog::HttpCatalog;
use doc_contract::error::{DetailBody, ErrorKind, UnavailableBody, ValidationBody};
use doc_contract::validate::{BookSelection, LangCode, ValidationError};
use doc_contract::wire::{
    BookEntry, ChaptersInBooks, HealthDetail, HealthResponse, LanguageEntry, ResourceTypeEntry, WithFindings,
};
use doc_contract::{Mode, openapi::ApiDoc};
use doc_lookup::Outcome;
use serde::Serialize;
use tower_http::trace::{DefaultOnResponse, TraceLayer};
use tracing::Level;
use utoipa::OpenApi;
use utoipa_axum::{router::OpenApiRouter, routes};

pub struct Shared {
    pub app: Arc<App<HttpCatalog, GitAcquirer>>,
    pub default_mode: Mode,
}

type State_ = State<Arc<Shared>>;

const MODE_HEADER: &str = "x-doc-mode";

// ------------------------------------------------------------------ helpers

#[allow(clippy::result_large_err)] // the Err is the ready-made HTTP response, returned at once by the handler
fn mode_of(headers: &HeaderMap, shared: &Shared) -> Result<Mode, Response> {
    match headers.get(MODE_HEADER).and_then(|v| v.to_str().ok()) {
        None => Ok(shared.default_mode),
        Some(v) => Mode::parse(v).ok_or_else(|| {
            validation_response(&ValidationError { loc: "header.x-doc-mode".into(), msg: "must be as_built or fixed".into(), input: v.chars().take(32).collect() })
        }),
    }
}

fn validation_response(e: &ValidationError) -> Response {
    (StatusCode::UNPROCESSABLE_ENTITY, Json::<ValidationBody>(e.body())).into_response()
}

fn not_found() -> Response {
    (StatusCode::NOT_FOUND, Json(DetailBody { detail: "Not Found".into() })).into_response()
}

fn app_error(e: AppError) -> Response {
    match e {
        AppError::Catalog(c) => {
            let kind = match &c {
                doc_ports::CatalogError::Malformed { .. } => ErrorKind::CatalogMalformed,
                doc_ports::CatalogError::DeadlineExceeded { .. } => ErrorKind::DeadlineExceeded,
                doc_ports::CatalogError::Unavailable { .. } => ErrorKind::CatalogUnavailable,
            };
            let mut resp = (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(UnavailableBody { kind, message: c.to_string(), retryable: c.retryable() }),
            )
                .into_response();
            resp.headers_mut().insert(header::RETRY_AFTER, HeaderValue::from_static("5"));
            resp
        }
        // Starlette's body for an unhandled exception
        AppError::Internal(_) => (StatusCode::INTERNAL_SERVER_ERROR, "Internal Server Error").into_response(),
    }
}

/// The path parameters FastAPI would route: a decoded value containing `/` does not match `{x}`.
#[allow(clippy::result_large_err)] // the Err is the ready-made HTTP response, returned at once by the handler
fn lang_param(raw: &str, name: &str, mode: Mode) -> Result<String, Response> {
    if raw.contains('/') {
        return Err(not_found());
    }
    match mode {
        Mode::AsBuilt => Ok(raw.to_string()),
        Mode::Fixed => LangCode::parse(raw, name).map(|l| l.as_str().to_string()).map_err(|e| validation_response(&e)),
    }
}

fn with_header(mut resp: Response, outcome_findings: usize, mode: Mode) -> Response {
    if let Ok(v) = HeaderValue::from_str(&outcome_findings.to_string()) {
        resp.headers_mut().insert("x-doc-findings", v);
    }
    resp.headers_mut().insert("x-doc-mode", HeaderValue::from_static(mode.as_str()));
    resp
}

fn plain<T: Serialize>(o: Outcome<T>, mode: Mode) -> Response {
    let n = o.findings.len();
    with_header(Json(o.value).into_response(), n, mode)
}

fn enveloped<T: Serialize>(o: Outcome<T>, mode: Mode) -> Response {
    let n = o.findings.len();
    with_header(Json(WithFindings { data: o.value, findings: o.findings, mode }).into_response(), n, mode)
}

macro_rules! mode_or_return {
    ($headers:expr, $shared:expr) => {
        match mode_of(&$headers, &$shared) {
            Ok(m) => m,
            Err(r) => return r,
        }
    };
}
macro_rules! ok_or_return {
    ($e:expr) => {
        match $e {
            Ok(v) => v,
            Err(r) => return r,
        }
    };
}

// ------------------------------------------------------------------ handlers

/// Ping-able server endpoint. DOC returns a Python tuple, which FastAPI serialises as `[{"status":"ok"},200]`.
#[utoipa::path(get, path = "/health/status", tag = "meta",
    responses((status = 200, description = "Alive", body = HealthResponse)))]
async fn health_status() -> Json<HealthResponse> {
    Json(HealthResponse(HealthDetail { status: "ok".into() }, 200))
}

/// Chapter numbers of every book, in canonical book order.
#[utoipa::path(get, path = "/chapters_in_books", tag = "books",
    responses((status = 200, description = "book code to chapter numbers", body = ChaptersInBooks)))]
async fn chapters_in_books(State(s): State_) -> Json<ChaptersInBooks> {
    Json(s.app.chapters_in_books())
}

/// All languages: `[code, "national (english)", is_gateway]`, sorted by display name.
#[utoipa::path(get, path = "/language_codes_and_names", tag = "languages",
    params(("X-Doc-Mode" = Option<Mode>, Header, description = "as_built (default) or fixed")),
    responses(
        (status = 200, description = "languages", body = Vec<LanguageEntry>),
        (status = 503, description = "data API unavailable (fixed mode)", body = UnavailableBody),
        (status = 422, description = "invalid header value", body = ValidationBody)))]
async fn language_codes_and_names(State(s): State_, headers: HeaderMap) -> Response {
    let mode = mode_or_return!(headers, s);
    match s.app.languages(mode).await {
        Ok(o) => plain(o, mode),
        Err(e) => app_error(e),
    }
}

/// Books available for a language, with names as the resources provide them.
#[utoipa::path(get, path = "/book_codes_for_lang/{lang_code}", tag = "books",
    params(("lang_code" = String, Path, description = "language code"), ("X-Doc-Mode" = Option<Mode>, Header, description = "as_built (default) or fixed")),
    responses(
        (status = 200, description = "book code and name", body = Vec<BookEntry>),
        (status = 422, description = "invalid path parameter (fixed mode)", body = ValidationBody),
        (status = 503, description = "data API unavailable (fixed mode)", body = UnavailableBody)))]
async fn book_codes_for_lang(State(s): State_, headers: HeaderMap, Path(lang): Path<String>) -> Response {
    let mode = mode_or_return!(headers, s);
    let lang = ok_or_return!(lang_param(&lang, "lang_code", mode));
    match s.app.book_codes_for_lang(&lang, false, mode).await {
        Ok(o) => plain(o, mode),
        Err(e) => app_error(e),
    }
}

/// As `book_codes_for_lang`, from USFM sources only.
#[utoipa::path(get, path = "/book_codes_for_lang_from_usfm_only/{lang_code}", tag = "books",
    params(("lang_code" = String, Path, description = "language code"), ("X-Doc-Mode" = Option<Mode>, Header, description = "as_built (default) or fixed")),
    responses(
        (status = 200, description = "book code and name", body = Vec<BookEntry>),
        (status = 422, description = "invalid path parameter (fixed mode)", body = ValidationBody),
        (status = 503, description = "data API unavailable (fixed mode)", body = UnavailableBody)))]
async fn book_codes_for_lang_from_usfm_only(State(s): State_, headers: HeaderMap, Path(lang): Path<String>) -> Response {
    let mode = mode_or_return!(headers, s);
    let lang = ok_or_return!(lang_param(&lang, "lang_code", mode));
    match s.app.book_codes_for_lang(&lang, true, mode).await {
        Ok(o) => plain(o, mode),
        Err(e) => app_error(e),
    }
}

/// Books both languages have, in the first language's order and names.
#[utoipa::path(get, path = "/shared_book_codes/{lang0_code}/{lang1_code}", tag = "books",
    params(("lang0_code" = String, Path, description = "first language"), ("lang1_code" = String, Path, description = "second language"),
        ("X-Doc-Mode" = Option<Mode>, Header, description = "as_built (default) or fixed")),
    responses(
        (status = 200, description = "book code and name", body = Vec<BookEntry>),
        (status = 422, description = "invalid path parameter (fixed mode)", body = ValidationBody),
        (status = 503, description = "data API unavailable (fixed mode)", body = UnavailableBody)))]
async fn shared_book_codes(State(s): State_, headers: HeaderMap, Path((l0, l1)): Path<(String, String)>) -> Response {
    let mode = mode_or_return!(headers, s);
    let l0 = ok_or_return!(lang_param(&l0, "lang0_code", mode));
    let l1 = ok_or_return!(lang_param(&l1, "lang1_code", mode));
    match s.app.shared_book_codes(&l0, &l1, mode).await {
        Ok(o) => plain(o, mode),
        Err(e) => app_error(e),
    }
}

#[allow(clippy::result_large_err)] // the Err is the ready-made HTTP response, returned at once by the handler
fn books_param(raw: &str, mode: Mode) -> Result<String, Response> {
    if raw.contains('/') {
        return Err(not_found());
    }
    match mode {
        Mode::AsBuilt => Ok(raw.to_string()),
        Mode::Fixed => {
            BookSelection::parse(raw, "book_codes", |c| doc_lookup::booklist::english_name(c).is_some()).map_err(|e| validation_response(&e))?;
            Ok(raw.to_string())
        }
    }
}

/// Resource types a language offers for the given books (`all` or a comma-separated list).
#[utoipa::path(get, path = "/resource_types/{lang_code}/{book_codes}", tag = "resources",
    params(("lang_code" = String, Path, description = "language code"), ("book_codes" = String, Path, description = "`all` or comma-separated book codes"),
        ("X-Doc-Mode" = Option<Mode>, Header, description = "as_built (default) or fixed")),
    responses(
        (status = 200, description = "resource type code and name", body = Vec<ResourceTypeEntry>),
        (status = 422, description = "invalid path parameter (fixed mode)", body = ValidationBody),
        (status = 503, description = "data API unavailable (fixed mode)", body = UnavailableBody)))]
async fn resource_types(State(s): State_, headers: HeaderMap, Path((lang, books)): Path<(String, String)>) -> Response {
    let mode = mode_or_return!(headers, s);
    let lang = ok_or_return!(lang_param(&lang, "lang_code", mode));
    let books = ok_or_return!(books_param(&books, mode));
    match s.app.resource_types(&lang, &books, mode).await {
        Ok(o) => plain(o, mode),
        Err(e) => app_error(e),
    }
}

// the same four with their findings

/// `book_codes_for_lang` with the findings the mode produced.
#[utoipa::path(get, path = "/findings/book_codes_for_lang/{lang_code}", tag = "findings",
    params(("lang_code" = String, Path, description = "language code"), ("X-Doc-Mode" = Option<Mode>, Header, description = "as_built (default) or fixed")),
    responses((status = 200, description = "books and findings", body = WithFindings<Vec<BookEntry>>),
        (status = 503, description = "data API unavailable (fixed mode)", body = UnavailableBody)))]
async fn findings_book_codes_for_lang(State(s): State_, headers: HeaderMap, Path(lang): Path<String>) -> Response {
    let mode = mode_or_return!(headers, s);
    let lang = ok_or_return!(lang_param(&lang, "lang_code", mode));
    match s.app.book_codes_for_lang(&lang, false, mode).await {
        Ok(o) => enveloped(o, mode),
        Err(e) => app_error(e),
    }
}

/// `book_codes_for_lang_from_usfm_only` with findings.
#[utoipa::path(get, path = "/findings/book_codes_for_lang_from_usfm_only/{lang_code}", tag = "findings",
    params(("lang_code" = String, Path, description = "language code"), ("X-Doc-Mode" = Option<Mode>, Header, description = "as_built (default) or fixed")),
    responses((status = 200, description = "books and findings", body = WithFindings<Vec<BookEntry>>),
        (status = 503, description = "data API unavailable (fixed mode)", body = UnavailableBody)))]
async fn findings_book_codes_usfm_only(State(s): State_, headers: HeaderMap, Path(lang): Path<String>) -> Response {
    let mode = mode_or_return!(headers, s);
    let lang = ok_or_return!(lang_param(&lang, "lang_code", mode));
    match s.app.book_codes_for_lang(&lang, true, mode).await {
        Ok(o) => enveloped(o, mode),
        Err(e) => app_error(e),
    }
}

/// `shared_book_codes` with findings.
#[utoipa::path(get, path = "/findings/shared_book_codes/{lang0_code}/{lang1_code}", tag = "findings",
    params(("lang0_code" = String, Path, description = "first language"), ("lang1_code" = String, Path, description = "second language"),
        ("X-Doc-Mode" = Option<Mode>, Header, description = "as_built (default) or fixed")),
    responses((status = 200, description = "books and findings", body = WithFindings<Vec<BookEntry>>),
        (status = 503, description = "data API unavailable (fixed mode)", body = UnavailableBody)))]
async fn findings_shared_book_codes(State(s): State_, headers: HeaderMap, Path((l0, l1)): Path<(String, String)>) -> Response {
    let mode = mode_or_return!(headers, s);
    let l0 = ok_or_return!(lang_param(&l0, "lang0_code", mode));
    let l1 = ok_or_return!(lang_param(&l1, "lang1_code", mode));
    match s.app.shared_book_codes(&l0, &l1, mode).await {
        Ok(o) => enveloped(o, mode),
        Err(e) => app_error(e),
    }
}

/// `resource_types` with findings.
#[utoipa::path(get, path = "/findings/resource_types/{lang_code}/{book_codes}", tag = "findings",
    params(("lang_code" = String, Path, description = "language code"), ("book_codes" = String, Path, description = "`all` or comma-separated book codes"),
        ("X-Doc-Mode" = Option<Mode>, Header, description = "as_built (default) or fixed")),
    responses((status = 200, description = "resource types and findings", body = WithFindings<Vec<ResourceTypeEntry>>),
        (status = 503, description = "data API unavailable (fixed mode)", body = UnavailableBody)))]
async fn findings_resource_types(State(s): State_, headers: HeaderMap, Path((lang, books)): Path<(String, String)>) -> Response {
    let mode = mode_or_return!(headers, s);
    let lang = ok_or_return!(lang_param(&lang, "lang_code", mode));
    let books = ok_or_return!(books_param(&books, mode));
    match s.app.resource_types(&lang, &books, mode).await {
        Ok(o) => enveloped(o, mode),
        Err(e) => app_error(e),
    }
}

// ------------------------------------------------------------------ FastAPI-compatible edges

/// Which paths are routes (any method), as Starlette's router would match them.
fn route_exists(path: &str) -> bool {
    let segs: Vec<&str> = path.trim_start_matches('/').split('/').collect();
    let ne = |i: usize| segs.get(i).is_some_and(|s| !s.is_empty());
    match segs.as_slice() {
        ["health", "status"] | ["chapters_in_books"] | ["language_codes_and_names"] | ["openapi.json"] => true,
        ["book_codes_for_lang", _] | ["book_codes_for_lang_from_usfm_only", _] => ne(1),
        ["shared_book_codes", _, _] | ["resource_types", _, _] => ne(1) && ne(2),
        ["findings", "book_codes_for_lang", _] | ["findings", "book_codes_for_lang_from_usfm_only", _] => ne(2),
        ["findings", "shared_book_codes", _, _] | ["findings", "resource_types", _, _] => ne(2) && ne(3),
        _ => false,
    }
}

async fn fallback(req: Request) -> Response {
    let path = req.uri().path();
    // Starlette's redirect_slashes: `/x/` redirects to `/x` when `/x` is a route
    if path.len() > 1 && path.ends_with('/') && route_exists(path.trim_end_matches('/')) {
        let host = req.headers().get(header::HOST).and_then(|h| h.to_str().ok()).unwrap_or("localhost");
        let query = req.uri().query().map(|q| format!("?{q}")).unwrap_or_default();
        let location = format!("http://{host}{}{query}", path.trim_end_matches('/'));
        let mut resp = Response::new(axum::body::Body::empty());
        *resp.status_mut() = StatusCode::TEMPORARY_REDIRECT;
        if let Ok(v) = HeaderValue::from_str(&location) {
            resp.headers_mut().insert(header::LOCATION, v);
        }
        return resp;
    }
    not_found()
}

async fn method_not_allowed() -> Response {
    let mut resp = (StatusCode::METHOD_NOT_ALLOWED, Json(DetailBody { detail: "Method Not Allowed".into() })).into_response();
    resp.headers_mut().insert(header::ALLOW, HeaderValue::from_static("GET"));
    resp
}

/// FastAPI answers 405 to HEAD on a GET route; axum would answer it as GET without a body.
async fn no_head(req: Request, next: Next) -> Response {
    if req.method() == Method::HEAD && route_exists(req.uri().path()) {
        return method_not_allowed().await;
    }
    next.run(req).await
}

/// Build the router and the OpenAPI document from the same registrations.
pub fn build(shared: Arc<Shared>, cors_origins: &[String]) -> (axum::Router, utoipa::openapi::OpenApi) {
    build_with(shared, cors_origins, None, None)
}

/// P5: `extra` carries the job routes (POST /documents_docx, GET /task_status/{id}, the extended
/// GET /health/status). When it is given it replaces the plain health route, and it sits inside the same
/// CORS, tracing and no-HEAD layers as every other route.
pub fn build_with(shared: Arc<Shared>, cors_origins: &[String], extra: Option<axum::Router>, extra_doc: Option<utoipa::openapi::OpenApi>) -> (axum::Router, utoipa::openapi::OpenApi) {
    let base = OpenApiRouter::with_openapi(ApiDoc::openapi());
    let base = if extra.is_none() { base.routes(routes!(health_status)) } else { base };
    let (router, api) = base
        .routes(routes!(chapters_in_books))
        .routes(routes!(language_codes_and_names))
        .routes(routes!(book_codes_for_lang))
        .routes(routes!(book_codes_for_lang_from_usfm_only))
        .routes(routes!(shared_book_codes))
        .routes(routes!(resource_types))
        .routes(routes!(findings_book_codes_for_lang))
        .routes(routes!(findings_book_codes_usfm_only))
        .routes(routes!(findings_shared_book_codes))
        .routes(routes!(findings_resource_types))
        .split_for_parts();
    let mut api = api;
    if let Some(d) = extra_doc {
        api.merge(d);
    }
    let api_for_route = api.clone();
    let router = router
        .route("/openapi.json", axum::routing::get(move || {
            let doc = api_for_route.clone();
            async move { Json(doc) }
        }))
        .fallback(fallback)
        .method_not_allowed_fallback(method_not_allowed)
        .with_state(shared);
    let router = match extra {
        Some(e) => router.merge(e),
        None => router,
    };
    // wrap in an outer router: `Router::layer` would sit inside each route's method router, which
    // adds an `Allow` header to the CORS preflight answer (RAN: the first corpus pass showed it)
    let router = axum::Router::new().fallback_service(router).layer(middleware::from_fn(no_head));
    let policy = Arc::new(crate::cors::CorsPolicy { origins: cors_origins.to_vec() });
    let router = router
        .layer(middleware::from_fn_with_state(policy, crate::cors::cors))
        .layer(TraceLayer::new_for_http().on_response(DefaultOnResponse::new().level(Level::INFO)));
    (router, api)
}
