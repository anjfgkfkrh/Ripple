use std::collections::HashMap;
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::{Arc, Mutex};

use super::*;
use crate::server::collect::http::{HttpError, HttpErrorKind, HttpResponse};

fn rules(text: &str) -> Vec<Rule> {
    parse_robots(text, ROBOTS_AGENT)
}

// describe("parseRobots·isAllowed (RFC 9309)")
#[test]
fn uses_star_group_and_longest_matching_rule() {
    // it("'*' 그룹을 쓰고, 가장 길게 맞는 규칙을 따른다")
    let rules = rules(
        &[
            "User-agent: Googlebot",
            "Disallow: /",
            "",
            "User-agent: *",
            "Disallow: /",
            "Allow: /rss/",
            "Allow: /$",
        ]
        .join("\n"),
    );
    assert!(is_allowed(&rules, "/rss/allArticle.xml"));
    assert!(is_allowed(&rules, "/"));
    assert!(!is_allowed(&rules, "/news/1"));
}

#[test]
fn our_name_group_wins_over_star() {
    // it("우리 이름(ripple) 그룹이 있으면 그것만 쓴다")
    let rules = rules(
        &[
            "User-agent: *",
            "Allow: /",
            "",
            "User-agent: Ripple",
            "Disallow: /private",
        ]
        .join("\n"),
    );
    assert!(!is_allowed(&rules, "/private/x"));
    assert!(is_allowed(&rules, "/rss"));
}

#[test]
fn consecutive_user_agents_form_one_group_with_star_and_dollar_patterns() {
    // it("연달아 쓴 User-agent는 한 그룹, * 와 $ 패턴, 빈 Disallow는 허용")
    let rules = rules(
        &[
            "User-agent: a",
            "User-agent: *",
            "Disallow: /*?*cp=",
            "Disallow: /search$",
            "Disallow:",
        ]
        .join("\n"),
    );
    assert!(!is_allowed(&rules, "/view/1?cp=x"));
    assert!(!is_allowed(&rules, "/search"));
    assert!(is_allowed(&rules, "/search/more"));
    assert!(is_allowed(
        &super::parse_robots("User-agent: *\nDisallow:", ROBOTS_AGENT),
        "/anything"
    ));
}

#[test]
fn equal_length_allow_wins_and_no_rules_means_allowed() {
    // it("길이가 같으면 허용이 이긴다, 규칙이 없으면 허용")
    let tie = [
        Rule {
            allow: false,
            pattern: "/a".into(),
        },
        Rule {
            allow: true,
            pattern: "/a".into(),
        },
    ];
    assert!(is_allowed(&tie, "/a"));
    assert!(is_allowed(&[], "/x"));
}

#[test]
fn comments_crlf_and_unknown_lines_are_handled() {
    // Rust에서만 생기는 경우: 주석·CRLF·모르는 키·그룹 앞의 규칙
    let text = "Disallow: /before-any-group\r\nUser-agent: * # 모두\r\nCrawl-delay: 5\r\nDisallow: /x # 주석\r\n";
    let rules = rules(text);
    assert_eq!(
        rules,
        [Rule {
            allow: false,
            pattern: "/x".into()
        }]
    );
    // 정규식 특수문자는 글자 그대로 본다.
    let dots = super::parse_robots("User-agent: *\nDisallow: /a.b+c", ROBOTS_AGENT);
    assert!(!is_allowed(&dots, "/a.b+c"));
    assert!(is_allowed(&dots, "/aXb+c"));
}

enum Fake {
    Reply(u16, &'static str),
    Fail,
}

struct FakeHttp {
    robots: HashMap<String, Fake>,
    calls: Mutex<Vec<String>>,
}

impl HttpClient for FakeHttp {
    async fn get(&self, url: &str, _: &[(&str, &str)]) -> Result<HttpResponse, HttpError> {
        self.calls.lock().unwrap().push(url.to_string());
        let origin = url.trim_end_matches("/robots.txt");
        match self.robots.get(origin) {
            None => Ok(HttpResponse {
                status: 404,
                text: String::new(),
                content_type: String::new(),
            }),
            Some(Fake::Fail) => Err(HttpError::new("network", HttpErrorKind::Network)),
            Some(Fake::Reply(status, text)) => Ok(HttpResponse {
                status: *status,
                text: (*text).to_string(),
                content_type: "text/plain".into(),
            }),
        }
    }
}

fn fake_http(robots: Vec<(&str, Fake)>) -> FakeHttp {
    FakeHttp {
        robots: robots
            .into_iter()
            .map(|(k, v)| (k.to_string(), v))
            .collect(),
        calls: Mutex::new(Vec::new()),
    }
}

// describe("createRobotsChecker")
#[tokio::test(flavor = "current_thread")]
async fn fetches_once_per_site_and_remembers_for_a_day() {
    // it("사이트마다 한 번 받아 하루 동안 기억한다")
    let now = Arc::new(AtomicI64::new(0));
    let http = fake_http(vec![(
        "https://a.example",
        Fake::Reply(200, "User-agent: *\nDisallow: /private"),
    )]);
    let clock = now.clone();
    let robots = RobotsChecker::with_options(
        http,
        24 * 60 * 60 * 1000,
        Box::new(move || clock.load(Ordering::SeqCst)),
    );
    assert!(robots.allowed("https://a.example/rss.xml").await);
    assert!(!robots.allowed("https://a.example/private/x").await);
    assert_eq!(
        *robots.http.calls.lock().unwrap(),
        ["https://a.example/robots.txt"]
    );
    now.store(25 * 60 * 60 * 1000, Ordering::SeqCst);
    robots.allowed("https://a.example/rss.xml").await;
    assert_eq!(robots.http.calls.lock().unwrap().len(), 2);
}

#[tokio::test(flavor = "current_thread")]
async fn missing_robots_allows_but_server_error_or_failure_blocks_without_remembering() {
    // it("robots.txt가 없으면(4xx) 허용, 서버 오류·연결 실패면 이번엔 막는다(기억하지 않음)")
    let http = fake_http(vec![
        ("https://down.example", Fake::Reply(503, "")),
        ("https://err.example", Fake::Fail),
    ]);
    let robots = RobotsChecker::new(http);
    assert!(robots.allowed("https://none.example/feed").await);
    assert!(!robots.allowed("https://down.example/feed").await);
    assert!(!robots.allowed("https://err.example/feed").await);
    robots.allowed("https://down.example/feed").await;
    let calls = robots.http.calls.lock().unwrap();
    assert_eq!(
        calls.iter().filter(|c| c.contains("down.example")).count(),
        2
    );
}

#[tokio::test(flavor = "current_thread")]
async fn query_string_is_part_of_the_checked_path() {
    // Rust에서만 생기는 경우: pathname + search로 규칙을 본다(빈 `?`는 붙이지 않음)
    let http = fake_http(vec![(
        "https://q.example",
        Fake::Reply(200, "User-agent: *\nDisallow: /*?cp="),
    )]);
    let robots = RobotsChecker::new(http);
    assert!(!robots.allowed("https://q.example/view?cp=1").await);
    assert!(robots.allowed("https://q.example/view?other=1").await);
    assert!(robots.allowed("https://q.example/view?").await);
    assert!(!robots.allowed("not a url").await);
}
