//! HTML 응답 도우미. 동적 화면은 캐시하지 않는다(Next의 동적 페이지와 같다).
use askama::Template;
use axum::http::{HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};

use crate::server::log::log_error;

const NO_STORE: &str = "private, no-cache, no-store, max-age=0, must-revalidate";

pub fn render<T: Template>(template: &T) -> Response {
    render_with_status(StatusCode::OK, template)
}

pub fn render_with_status<T: Template>(status: StatusCode, template: &T) -> Response {
    match template.render() {
        Ok(body) => html(status, body),
        Err(error) => {
            log_error("화면을 그리지 못했습니다", &error);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                "화면을 그리지 못했습니다.",
            )
                .into_response()
        }
    }
}

pub fn html(status: StatusCode, body: String) -> Response {
    let mut response = (status, body).into_response();
    let headers = response.headers_mut();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("text/html; charset=utf-8"),
    );
    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static(NO_STORE));
    response
}

/// 상대 주소로 이동시킨다(Next가 같은 호스트 이동을 상대 경로로 바꿔 주던 것과 같다, 설계 4.5).
pub fn redirect(status: StatusCode, location: &str) -> Response {
    let mut response = status.into_response();
    if let Ok(value) = HeaderValue::from_str(location) {
        response.headers_mut().insert(header::LOCATION, value);
    }
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static(NO_STORE));
    response
}
