//! 정적 파일(설계 3.12): 실행 파일에 넣고 `/static/<이름>.<해시8자>.<확장자>`로 내보낸다.
//! `app.css`는 Tailwind 빌드 결과(`rust/build.sh`)이고 저장소에는 없다.
use std::sync::Arc;

use axum::extract::{Path, State};
use axum::http::{HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use sha2::{Digest, Sha256};

use crate::web::state::WebState;

static APP_CSS: &[u8] = include_bytes!("../../static/app.css");
static APP_JS: &[u8] = include_bytes!("../../static/app.js");
static HTMX_JS: &[u8] = include_bytes!("../../static/htmx.min.js");
static FAVICON: &[u8] = include_bytes!("../../static/favicon.ico");

#[derive(Debug, Clone)]
struct Asset {
    /// 주소에 붙는 이름(예: `app.1a2b3c4d.css`)
    name: String,
    content_type: &'static str,
    bytes: &'static [u8],
}

/// 템플릿에 넘기는 정적 파일 주소
#[derive(Debug, Clone)]
pub struct AssetUrls {
    pub css: String,
    pub app_js: String,
    pub htmx: String,
}

#[derive(Debug, Clone)]
pub struct Assets {
    files: Vec<Asset>,
    pub urls: AssetUrls,
}

fn hashed(stem: &str, ext: &str, content_type: &'static str, bytes: &'static [u8]) -> Asset {
    let digest = Sha256::digest(bytes);
    let hash: String = digest.iter().take(4).map(|b| format!("{b:02x}")).collect();
    Asset {
        name: format!("{stem}.{hash}.{ext}"),
        content_type,
        bytes,
    }
}

impl Assets {
    /// 시작할 때 한 번 해시를 계산한다.
    pub fn load() -> Self {
        let css = hashed("app", "css", "text/css; charset=utf-8", APP_CSS);
        let app_js = hashed("app", "js", "text/javascript; charset=utf-8", APP_JS);
        let htmx = hashed("htmx", "js", "text/javascript; charset=utf-8", HTMX_JS);
        let urls = AssetUrls {
            css: format!("/static/{}", css.name),
            app_js: format!("/static/{}", app_js.name),
            htmx: format!("/static/{}", htmx.name),
        };
        Self {
            files: vec![css, app_js, htmx],
            urls,
        }
    }

    fn find(&self, name: &str) -> Option<&Asset> {
        self.files.iter().find(|asset| asset.name == name)
    }
}

fn file_response(
    content_type: &'static str,
    bytes: &'static [u8],
    cache: &'static str,
) -> Response {
    let mut response = bytes.into_response();
    let headers = response.headers_mut();
    headers.insert(header::CONTENT_TYPE, HeaderValue::from_static(content_type));
    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static(cache));
    response
}

pub async fn serve(State(state): State<Arc<WebState>>, Path(name): Path<String>) -> Response {
    match state.assets.find(&name) {
        Some(asset) => file_response(
            asset.content_type,
            asset.bytes,
            "public, max-age=31536000, immutable",
        ),
        None => StatusCode::NOT_FOUND.into_response(),
    }
}

/// 로그인 없이 내보낸다(E2E가 기동 확인에 쓴다).
pub async fn favicon() -> Response {
    file_response("image/x-icon", FAVICON, "public, max-age=86400")
}

#[cfg(test)]
#[path = "assets_tests.rs"]
mod tests;
