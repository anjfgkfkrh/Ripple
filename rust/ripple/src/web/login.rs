//! 로그인 화면과 동작(`app/login/*`, `logout`).
use std::collections::HashMap;
use std::sync::Arc;

use askama::Template;
use axum::Form;
use axum::extract::{Query, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::Response;

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

pub async fn submit(
    State(state): State<Arc<WebState>>,
    headers: HeaderMap,
    Form(form): Form<HashMap<String, String>>,
) -> Response {
    let key = client_key(&headers);
    let next = safe_next_path(form.get("next").map(String::as_str));
    let now = state.now();

    // 막혀 있는 동안은 비밀번호를 확인하지 않는다.
    let blocked_ms = state.limiter.blocked_for(&key, now);
    if blocked_ms > 0 {
        return login_page(&state, next, Some(blocked_message(blocked_ms)));
    }

    let correct = form
        .get("password")
        .is_some_and(|password| is_password_correct(password, &state.auth.password));
    if !correct {
        let now_blocked_ms = state.limiter.record_failure(&key, now);
        log(&format!("로그인 실패 ({key})"));
        let message = if now_blocked_ms > 0 {
            blocked_message(now_blocked_ms)
        } else {
            "비밀번호가 맞지 않습니다.".to_string()
        };
        return login_page(&state, next, Some(message));
    }

    state.limiter.reset(&key);
    let token = create_session_token(&state.auth, now, state.edit_unlock_minutes);
    log(&format!("로그인 성공 ({key})"));
    let mut response = redirect(StatusCode::SEE_OTHER, &next);
    if let Ok(value) =
        HeaderValue::from_str(&session_cookie_header(&token, is_https_request(&headers)))
    {
        response.headers_mut().append(header::SET_COOKIE, value);
    }
    response
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
