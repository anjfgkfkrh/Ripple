//! 로그인 화면과 동작(`app/login/*`, `logout`).
use std::collections::HashMap;
use std::sync::Arc;

use askama::Template;
use axum::Form;
use axum::extract::{Query, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};

use crate::server::auth::config::is_password_correct;
use crate::server::auth::request::{client_key, is_https_request, safe_next_path};
use crate::server::auth::session::{SESSION_COOKIE, SESSION_MAX_AGE_SECONDS, create_session_token};
use crate::server::log::log;
use crate::web::assets::AssetUrls;
use crate::web::guard::has_valid_session;
use crate::web::response::{redirect, render};
use crate::web::state::WebState;

#[derive(Template)]
#[template(path = "login.html")]
struct LoginTemplate {
    assets: AssetUrls,
    next: String,
    error: Option<String>,
}

fn login_page(state: &WebState, next: String, error: Option<String>) -> Response {
    render(&LoginTemplate {
        assets: state.assets.urls.clone(),
        next,
        error,
    })
}

pub async fn page(
    State(state): State<Arc<WebState>>,
    headers: HeaderMap,
    Query(query): Query<HashMap<String, String>>,
) -> Response {
    let next = safe_next_path(query.get("next").map(String::as_str));
    // 이미 로그인했으면 가려던 곳으로 보낸다.
    if has_valid_session(&state, &headers) {
        return redirect(StatusCode::TEMPORARY_REDIRECT, &next);
    }
    login_page(&state, next, None)
}

fn blocked_message(ms: i64) -> String {
    format!(
        "로그인 시도가 너무 많습니다. {}분 뒤에 다시 시도하세요.",
        (ms as f64 / 60_000.0).ceil() as i64
    )
}

fn session_cookie_header(token: &str, secure: bool) -> String {
    format!(
        "{SESSION_COOKIE}={token}; Max-Age={SESSION_MAX_AGE_SECONDS}; Path=/; HttpOnly; SameSite=Lax{}",
        if secure { "; Secure" } else { "" }
    )
}

/// 비밀번호를 확인한다(`/login`과 `/unlock`이 함께 쓴다). 맞으면 새 세션 쿠키 값을, 아니면 화면에 보일 문구를 돌려준다.
/// 막혀 있는 동안은 비밀번호를 확인하지 않는다.
fn check_password(
    state: &WebState,
    headers: &HeaderMap,
    password: Option<&String>,
) -> Result<HeaderValue, String> {
    let key = client_key(headers);
    let now = state.now();

    let blocked_ms = state.limiter.blocked_for(&key, now);
    if blocked_ms > 0 {
        return Err(blocked_message(blocked_ms));
    }

    let correct =
        password.is_some_and(|password| is_password_correct(password, &state.auth.password));
    if !correct {
        let now_blocked_ms = state.limiter.record_failure(&key, now);
        log(&format!("로그인 실패 ({key})"));
        return Err(if now_blocked_ms > 0 {
            blocked_message(now_blocked_ms)
        } else {
            "비밀번호가 맞지 않습니다.".to_string()
        });
    }

    state.limiter.reset(&key);
    let token = create_session_token(&state.auth, now, state.edit_unlock_minutes);
    log(&format!("로그인 성공 ({key})"));
    Ok(
        HeaderValue::from_str(&session_cookie_header(&token, is_https_request(headers)))
            .expect("세션 쿠키는 헤더 값으로 쓸 수 있다"),
    )
}

pub async fn submit(
    State(state): State<Arc<WebState>>,
    headers: HeaderMap,
    Form(form): Form<HashMap<String, String>>,
) -> Response {
    let next = safe_next_path(form.get("next").map(String::as_str));
    match check_password(&state, &headers, form.get("password")) {
        Err(message) => login_page(&state, next, Some(message)),
        Ok(cookie) => {
            let mut response = redirect(StatusCode::SEE_OTHER, &next);
            response.headers_mut().append(header::SET_COOKIE, cookie);
            response
        }
    }
}

/// `POST /unlock`: 비밀번호 창이 fetch로 부른다. 맞으면 새 토큰(수정 권한 새로 시작)을 쿠키로 주고 204,
/// 틀리거나 막혔으면 401 JSON.
pub async fn unlock(
    State(state): State<Arc<WebState>>,
    headers: HeaderMap,
    Form(form): Form<HashMap<String, String>>,
) -> Response {
    match check_password(&state, &headers, form.get("password")) {
        Ok(cookie) => {
            let mut response = StatusCode::NO_CONTENT.into_response();
            response.headers_mut().append(header::SET_COOKIE, cookie);
            response
        }
        Err(message) => {
            let mut response = (
                StatusCode::UNAUTHORIZED,
                serde_json::json!({ "error": message }).to_string(),
            )
                .into_response();
            response.headers_mut().insert(
                header::CONTENT_TYPE,
                HeaderValue::from_static("application/json"),
            );
            response
        }
    }
}

pub async fn logout() -> Response {
    let mut response = redirect(StatusCode::SEE_OTHER, "/login");
    response.headers_mut().append(
        header::SET_COOKIE,
        HeaderValue::from_static(
            "ripple_session=; Max-Age=0; Path=/; Expires=Thu, 01 Jan 1970 00:00:00 GMT",
        ),
    );
    response
}

#[cfg(test)]
#[path = "login_tests.rs"]
mod tests;
