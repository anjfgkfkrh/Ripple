//! 로그인 확인과 요청 출처 검사(`proxy.ts`, Next 서버 액션의 Origin 검사).
//! 로그인하지 않으면 모든 페이지·API를 막는다(1차 확인, SPEC FR-1). 예외는 로그인 화면과 정적 파일뿐이다.
use std::sync::Arc;

use axum::body::Body;
use axum::extract::{Request, State};
use axum::http::{HeaderMap, HeaderValue, Method, StatusCode, header};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use url::Url;

use crate::server::auth::session::{SESSION_COOKIE, is_valid_session_token};
use crate::web::response::redirect;
use crate::web::state::WebState;

/// 요청의 세션 쿠키 값
pub fn session_cookie(headers: &HeaderMap) -> Option<String> {
    headers
        .get_all(header::COOKIE)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .flat_map(|value| value.split(';'))
        .find_map(|pair| {
            let (name, value) = pair.trim().split_once('=')?;
            (name == SESSION_COOKIE).then(|| value.to_string())
        })
}

pub fn has_valid_session(state: &WebState, headers: &HeaderMap) -> bool {
    let token = session_cookie(headers);
    is_valid_session_token(token.as_deref(), &state.auth, state.now())
}

fn is_public(path: &str) -> bool {
    path == "/login" || path == "/favicon.ico" || path.starts_with("/static/")
}

fn first_value(headers: &HeaderMap, name: &str) -> Option<String> {
    let value = headers.get(name)?.to_str().ok()?;
    let first = value.split(',').next()?.trim();
    (!first.is_empty()).then(|| first.to_string())
}

/// Next 서버 액션과 같은 검사: `Origin`이 있으면 그 호스트가 `x-forwarded-host`(없으면 `Host`)와 같아야 한다.
/// `Origin`이 없는 요청(손으로 만든 요청)은 통과시킨다. 쿠키 `SameSite=Lax`와 함께 쓴다(설계 5.1).
pub fn origin_allowed(headers: &HeaderMap) -> bool {
    let Some(origin) = headers.get(header::ORIGIN) else {
        return true;
    };
    let Ok(origin) = origin.to_str() else {
        return false;
    };
    let origin_host = if origin == "null" {
        "null".to_string()
    } else {
        let Ok(url) = Url::parse(origin) else {
            return false;
        };
        let Some(host) = url.host_str() else {
            return false;
        };
        match url.port() {
            Some(port) => format!("{host}:{port}"),
            None => host.to_string(),
        }
    };
    let host = first_value(headers, "x-forwarded-host").or_else(|| first_value(headers, "host"));
    host.as_deref() == Some(origin_host.as_str())
}

fn unauthorized_json() -> Response {
    let mut response = (
        StatusCode::UNAUTHORIZED,
        "{\"error\":\"unauthorized\"}".to_string(),
    )
        .into_response();
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/json"),
    );
    response
}

/// 로그인 화면 주소. 원래 주소가 `/`가 아니면 `?next=`로 남긴다(WHATWG 폼 인코딩, JS `URLSearchParams`와 같다).
pub fn login_location(path_and_query: &str) -> String {
    if path_and_query == "/" {
        return "/login".to_string();
    }
    let encoded: String = url::form_urlencoded::byte_serialize(path_and_query.as_bytes()).collect();
    format!("/login?next={encoded}")
}

pub async fn guard(
    State(state): State<Arc<WebState>>,
    request: Request<Body>,
    next: Next,
) -> Response {
    let safe_method = matches!(
        *request.method(),
        Method::GET | Method::HEAD | Method::OPTIONS
    );
    if !safe_method && !origin_allowed(request.headers()) {
        return (StatusCode::FORBIDDEN, "요청 출처가 맞지 않습니다.").into_response();
    }
    let path = request.uri().path().to_string();
    if is_public(&path) || has_valid_session(&state, request.headers()) {
        return next.run(request).await;
    }
    if path.starts_with("/api/") {
        return unauthorized_json();
    }
    let location = login_location(
        request
            .uri()
            .path_and_query()
            .map_or(path.as_str(), |pq| pq.as_str()),
    );
    // htmx 요청은 로그인 화면을 조각으로 바꿔 끼우지 않고 화면 전체를 이동시킨다.
    if request.headers().contains_key("hx-request") {
        let mut response = StatusCode::UNAUTHORIZED.into_response();
        if let Ok(value) = HeaderValue::from_str(&location) {
            response.headers_mut().insert("hx-redirect", value);
        }
        return response;
    }
    redirect(StatusCode::TEMPORARY_REDIRECT, &location)
}

#[cfg(test)]
#[path = "guard_tests.rs"]
mod tests;
