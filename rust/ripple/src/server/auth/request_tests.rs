use axum::http::{HeaderMap, HeaderValue};

use super::*;

fn headers(values: &[(&'static str, &str)]) -> HeaderMap {
    let mut map = HeaderMap::new();
    for (name, value) in values {
        map.insert(*name, HeaderValue::from_str(value).unwrap());
    }
    map
}

mod client_key_tests {
    use super::*;

    // it("Cloudflare가 넣은 접속자 IP를 쓴다")
    #[test]
    fn uses_cloudflare_ip() {
        assert_eq!(
            client_key(&headers(&[("cf-connecting-ip", "203.0.113.7")])),
            "ip:203.0.113.7"
        );
    }

    // it("터널을 거치지 않은 로컬 접속은 하나로 묶고, X-Forwarded-For는 믿지 않는다")
    #[test]
    fn local_access_is_one_key() {
        assert_eq!(client_key(&headers(&[])), "direct");
        assert_eq!(
            client_key(&headers(&[("x-forwarded-for", "198.51.100.1")])),
            "direct"
        );
    }
}

// it("x-forwarded-proto의 첫 값이 https일 때만 참")
#[test]
fn https_only_when_first_forwarded_proto_is_https() {
    assert!(is_https_request(&headers(&[(
        "x-forwarded-proto",
        "https"
    )])));
    assert!(is_https_request(&headers(&[(
        "x-forwarded-proto",
        "https, http"
    )])));
    assert!(!is_https_request(&headers(&[(
        "x-forwarded-proto",
        "http"
    )])));
    assert!(!is_https_request(&headers(&[])));
}

mod safe_next_path_tests {
    use super::*;

    // it.each("같은 사이트 경로는 그대로: %s")
    #[test]
    fn same_site_paths_stay() {
        for (value, expected) in [
            ("/", "/"),
            ("/stocks", "/stocks"),
            ("/stocks?market=KR#top", "/stocks?market=KR#top"),
        ] {
            assert_eq!(safe_next_path(Some(value)), expected, "{value}");
        }
    }

    // it.each("외부 주소·이상한 값·로그인 화면이면 홈으로: %s")
    #[test]
    fn others_go_home() {
        let values = [
            None,
            Some(""),
            Some("stocks"),
            Some("https://evil.example"),
            Some("//evil.example"),
            Some("/\\evil.example"),
            Some("/\t/evil.example"),
            Some("javascript:alert(1)"),
            Some("/login"),
            Some("/login?next=/"),
        ];
        for value in values {
            assert_eq!(safe_next_path(value), "/", "{value:?}");
        }
    }
}
