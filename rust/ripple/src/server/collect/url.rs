use std::sync::LazyLock;

use regex::Regex;

/// 추적용으로만 붙는 쿼리 파라미터(같은 기사를 다른 기사로 보지 않게 뺀다)
static TRACKING_PARAM: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)^(utm_[a-z]+|fbclid|gclid|plink|cooper)$").expect("정규식이 올바르다")
});
static TRAILING_SLASHES: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"/+$").expect("정규식이 올바르다"));

/// 같은 기사인지 가리는 키(D-19). http/https 차이, 끝 슬래시, 조각(#), 추적 파라미터를 무시한다.
/// http(s) 주소가 아니면 None.
pub fn url_key(raw: &str) -> Option<String> {
    let url = ::url::Url::parse(crate::server::text::js_trim(raw)).ok()?;
    if url.scheme() != "http" && url.scheme() != "https" {
        return None;
    }
    let mut params: Vec<(String, String)> = url
        .query_pairs()
        .filter(|(key, _)| !TRACKING_PARAM.is_match(key))
        .map(|(k, v)| (k.into_owned(), v.into_owned()))
        .collect();
    // JS 문자열 비교는 UTF-16 단위 순서다(안정 정렬).
    params.sort_by(|(a, _), (b, _)| a.encode_utf16().cmp(b.encode_utf16()));
    let query = if params.is_empty() {
        String::new()
    } else {
        format!(
            "?{}",
            ::url::form_urlencoded::Serializer::new(String::new())
                .extend_pairs(&params)
                .finish()
        )
    };
    let trimmed = TRAILING_SLASHES.replace(url.path(), "");
    let path = if trimmed.is_empty() {
        "/"
    } else {
        trimmed.as_ref()
    };
    let port = url.port().map_or(String::new(), |p| format!(":{p}"));
    Some(format!(
        "{}{port}{path}{query}",
        url.host_str().unwrap_or("").to_lowercase()
    ))
}

#[cfg(test)]
#[path = "url_tests.rs"]
mod tests;
