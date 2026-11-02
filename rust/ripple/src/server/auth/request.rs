//! 요청에서 읽는 값(`auth/request.ts`): 로그인 실패 제한 키, HTTPS 여부, 로그인 뒤 이동 경로.
use axum::http::HeaderMap;
use url::Url;

fn header<'a>(headers: &'a HeaderMap, name: &str) -> Option<&'a str> {
    headers.get(name).and_then(|value| value.to_str().ok())
}

/// 로그인 실패 제한에 쓰는 접속자 키. Cloudflare Tunnel을 거친 요청에는 Cloudflare가 접속자 IP를
/// CF-Connecting-IP에 넣는다(클라이언트가 보낸 값은 덮어씀). 이 헤더가 없으면 터널을 거치지 않은
/// 로컬 접속이므로 하나로 묶는다. 위조하기 쉬운 X-Forwarded-For는 쓰지 않는다.
pub fn client_key(headers: &HeaderMap) -> String {
    match header(headers, "cf-connecting-ip").map(str::trim) {
        Some(ip) if !ip.is_empty() => format!("ip:{ip}"),
        _ => "direct".to_string(),
    }
}

/// HTTPS로 들어온 요청이면 세션 쿠키에 Secure를 붙인다(터널은 HTTPS, 로컬 접속은 HTTP).
pub fn is_https_request(headers: &HeaderMap) -> bool {
    header(headers, "x-forwarded-proto")
        .and_then(|value| value.split(',').next())
        .map(str::trim)
        == Some("https")
}

const BASE: &str = "http://ripple.local";

/// 로그인 후 이동할 경로. 같은 사이트 안의 경로만 허용해 외부 주소로 보내는 공격을 막는다.
/// JS `new URL(value, base)`와 같은 WHATWG 해석(`url` 크레이트)을 쓴다.
pub fn safe_next_path(value: Option<&str>) -> String {
    let Some(value) = value.filter(|v| v.starts_with('/')) else {
        return "/".to_string();
    };
    let base = Url::parse(BASE).expect("기준 주소가 올바르다");
    let Ok(url) = base.join(value) else {
        return "/".to_string();
    };
    if url.origin() != base.origin() || url.path() == "/login" {
        return "/".to_string();
    }
    let mut out = url.path().to_string();
    if let Some(query) = url.query() {
        out.push('?');
        out.push_str(query);
    }
    if let Some(fragment) = url.fragment() {
        out.push('#');
        out.push_str(fragment);
    }
    out
}

#[cfg(test)]
#[path = "request_tests.rs"]
mod tests;
