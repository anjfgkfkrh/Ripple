use crate::web::test_support::start_web;

fn location(response: &reqwest::Response) -> String {
    response.headers()["location"].to_str().unwrap().to_string()
}

// it("페이지는 로그인 화면으로 보내고, 가려던 경로를 next로 남긴다")
#[tokio::test]
async fn pages_redirect_to_login_with_next() {
    let web = start_web(None).await;
    for (path, expected) in [
        ("/", "/login"),
        (
            "/stocks/123?tab=keywords",
            "/login?next=%2Fstocks%2F123%3Ftab%3Dkeywords",
        ),
        ("/?tab=status", "/login?next=%2F%3Ftab%3Dstatus"),
    ] {
        let response = web.client.get(web.url(path)).send().await.unwrap();
        assert_eq!(response.status(), 307, "{path}");
        assert_eq!(location(&response), expected);
    }
}

// it("API는 401로 거절한다")
#[tokio::test]
async fn api_is_401() {
    let web = start_web(None).await;
    let response = web
        .client
        .get(web.url("/api/anything"))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 401);
    assert_eq!(
        response.json::<serde_json::Value>().await.unwrap(),
        serde_json::json!({"error": "unauthorized"})
    );
}

// it("잘못된 세션 쿠키도 막는다")
#[tokio::test]
async fn forged_cookie_is_blocked() {
    let web = start_web(None).await;
    let response = web
        .client
        .get(web.url("/"))
        .header("cookie", "ripple_session=forged-token")
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 307);
}

// it("로그인 화면은 그대로 통과시킨다") + 정적 파일·favicon은 로그인 없이
#[tokio::test]
async fn public_paths_pass() {
    let web = start_web(None).await;
    for path in ["/login", "/favicon.ico", &web.state.assets.urls.css.clone()] {
        let response = web.client.get(web.url(path)).send().await.unwrap();
        assert_eq!(response.status(), 200, "{path}");
    }
}

// it("유효한 세션이면 페이지·API 모두 통과시킨다")
#[tokio::test]
async fn valid_session_passes() {
    let web = start_web(None).await;
    assert_eq!(web.get("/status").send().await.unwrap().status(), 200);
    assert_eq!(web.get("/api/anything").send().await.unwrap().status(), 404);
}

// Rust 전용: Origin 검사(Next 서버 액션과 같은 규칙)
#[tokio::test]
async fn origin_must_match_host() {
    let web = start_web(None).await;
    let host = web.base.trim_start_matches("http://").to_string();
    let bad = web
        .post("/logout")
        .header("origin", "http://evil.example")
        .send()
        .await
        .unwrap();
    assert_eq!(bad.status(), 403);
    let ok = web
        .post("/logout")
        .header("origin", format!("http://{host}"))
        .send()
        .await
        .unwrap();
    assert_eq!(ok.status(), 303);
    let none = web.post("/logout").send().await.unwrap();
    assert_eq!(none.status(), 303);
    let forwarded = web
        .post("/logout")
        .header("origin", "https://tunnel.example")
        .header("x-forwarded-host", "tunnel.example, other")
        .send()
        .await
        .unwrap();
    assert_eq!(forwarded.status(), 303);
}

// Rust 전용: htmx 요청은 로그인 화면을 끼우지 않고 HX-Redirect로 이동시킨다
#[tokio::test]
async fn htmx_request_gets_hx_redirect() {
    let web = start_web(None).await;
    let response = web
        .client
        .get(web.url("/status/collection"))
        .header("hx-request", "true")
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 401);
    assert_eq!(
        response.headers()["hx-redirect"],
        "/login?next=%2Fstatus%2Fcollection"
    );
}
