use std::collections::HashSet;

use super::*;

#[test]
fn scheme_trailing_slash_fragment_and_tracking_params_do_not_matter() {
    // it("스킴·끝 슬래시·조각·추적 파라미터 차이는 같은 기사로 본다")
    let keys: HashSet<Option<String>> = [
        "https://News.Example.com/a/1/",
        "http://news.example.com/a/1",
        "https://news.example.com/a/1#comments",
        "https://news.example.com/a/1?utm_source=rss&utm_medium=feed",
    ]
    .iter()
    .map(|u| url_key(u))
    .collect();
    assert_eq!(
        keys,
        [Some("news.example.com/a/1".to_string())]
            .into_iter()
            .collect()
    );
}

#[test]
fn article_query_stays_sorted_and_tracking_plink_cooper_dropped() {
    // it("기사를 가리키는 쿼리는 남기고 순서를 맞춘다(SBS처럼 추적용 plink·cooper는 뺀다)")
    assert_eq!(
        url_key("https://news.sbs.co.kr/news/endPage.do?news_id=N1&plink=RSSLINK&cooper=RSSREADER")
            .as_deref(),
        Some("news.sbs.co.kr/news/endPage.do?news_id=N1")
    );
    assert_eq!(
        url_key("https://e.com/v?b=2&a=1"),
        url_key("https://e.com/v?a=1&b=2")
    );
    assert_ne!(
        url_key("https://e.com/v?id=1"),
        url_key("https://e.com/v?id=2")
    );
}

#[test]
fn port_is_kept_and_non_http_or_garbage_is_none() {
    // it("포트는 남기고, http(s)가 아니거나 주소가 아니면 null")
    assert_eq!(
        url_key("http://127.0.0.1:4200/x").as_deref(),
        Some("127.0.0.1:4200/x")
    );
    assert_eq!(url_key("ftp://e.com/x"), None);
    assert_eq!(url_key("기사"), None);
}

#[test]
fn edge_cases_follow_javascript_url_behavior() {
    // Rust에서만 생기는 경우: 루트 경로, 기본 포트, 공백, 인코딩, 값 없는 파라미터
    assert_eq!(url_key("https://e.com").as_deref(), Some("e.com/"));
    assert_eq!(url_key("https://e.com///").as_deref(), Some("e.com/"));
    assert_eq!(url_key("https://e.com:443/x").as_deref(), Some("e.com/x"));
    assert_eq!(url_key("  https://e.com/x  ").as_deref(), Some("e.com/x"));
    assert_eq!(
        url_key("https://e.com/v?q=a b&flag").as_deref(),
        Some("e.com/v?flag=&q=a+b")
    );
    assert_eq!(
        url_key("https://e.com/한글?가=나").as_deref(),
        Some("e.com/%ED%95%9C%EA%B8%80?%EA%B0%80=%EB%82%98")
    );
    // 같은 이름의 파라미터는 들어온 순서를 지킨다(안정 정렬).
    assert_eq!(
        url_key("https://e.com/v?a=2&a=1").as_deref(),
        Some("e.com/v?a=2&a=1")
    );
}
