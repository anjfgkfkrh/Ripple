use crate::web::test_support::{start_web, start_web_guest_off};

fn location(response: &reqwest::Response) -> String {
    response.headers()["location"].to_str().unwrap().to_string()
}

// it("페이지는 로그인 화면으로 보내고, 가려던 경로를 next로 남긴다")
#[tokio::test]
async fn pages_redirect_to_login_with_next() {
    // 손님 보기를 끄면 홈·종목도 로그인 화면으로 간다
    let web = start_web_guest_off().await;
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
    let web = start_web_guest_off().await;
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

// ---- 손님·회원·수정 권한 ----

#[tokio::test]
async fn guest_can_view_home_and_stocks() {
    let web = start_web(None).await;
    for path in ["/", "/stocks", "/stocks/list"] {
        let response = web.client.get(web.url(path)).send().await.unwrap();
        assert_eq!(response.status(), 200, "{path}");
    }
}

#[tokio::test]
async fn guest_is_sent_to_login_for_member_pages() {
    let web = start_web(None).await;
    for path in ["/status", "/settings"] {
        let response = web.client.get(web.url(path)).send().await.unwrap();
        assert_eq!(response.status(), 307, "{path}");
        assert_eq!(
            location(&response),
            format!("/login?next=%2F{}", &path[1..])
        );
    }
    let htmx = web
        .client
        .get(web.url("/status/system"))
        .header("hx-request", "true")
        .send()
        .await
        .unwrap();
    assert_eq!(htmx.status(), 401);
    assert_eq!(
        htmx.headers()["hx-redirect"],
        "/login?next=%2Fstatus%2Fsystem"
    );
}

#[tokio::test]
async fn guest_view_off_sends_home_to_login() {
    let web = start_web_guest_off().await;
    let response = web.client.get(web.url("/")).send().await.unwrap();
    assert_eq!(response.status(), 307);
    assert_eq!(location(&response), "/login");
}

#[tokio::test]
async fn writes_need_edit_permission() {
    let web = start_web(None).await;
    let expired = web.expired_edit_cookie();
    for path in [
        "/settings",
        "/stocks/register",
        "/status/collect",
        "/no/such/post",
    ] {
        // 손님
        let guest = web
            .client
            .post(web.url(path))
            .header("hx-request", "true")
            .send()
            .await
            .unwrap();
        assert_eq!(guest.status(), 403, "guest {path}");
        assert_eq!(guest.headers()["hx-trigger"], "unlock-required");
        // 수정 권한이 지난 회원
        let member = web
            .client
            .post(web.url(path))
            .header("cookie", &expired)
            .header("hx-request", "true")
            .send()
            .await
            .unwrap();
        assert_eq!(member.status(), 403, "member {path}");
        assert_eq!(member.headers()["hx-trigger"], "unlock-required");
    }
    // htmx가 아니면 글자로 알린다
    let plain = web.client.post(web.url("/settings")).send().await.unwrap();
    assert_eq!(plain.status(), 403);
    assert_eq!(
        plain.text().await.unwrap(),
        "수정하려면 비밀번호가 필요합니다."
    );
}

#[tokio::test]
async fn articles_read_needs_member_only() {
    let web = start_web(None).await;
    let expired = web.expired_edit_cookie();
    let member = web
        .client
        .post(web.url("/articles/read"))
        .header("cookie", &expired)
        .body("{}")
        .send()
        .await
        .unwrap();
    // 가드는 통과한다(본문이 틀려 핸들러가 400을 돌려준다)
    assert_ne!(member.status(), 401);
    assert_ne!(member.status(), 403);
    let guest = web
        .client
        .post(web.url("/articles/read"))
        .body("{}")
        .send()
        .await
        .unwrap();
    assert_eq!(guest.status(), 401);
    assert_eq!(
        guest.json::<serde_json::Value>().await.unwrap(),
        serde_json::json!({"error": "unauthorized"})
    );
}

#[tokio::test]
async fn every_response_is_noindex() {
    let web = start_web(None).await;
    for response in [
        web.client.get(web.url("/")).send().await.unwrap(),
        web.client.get(web.url("/status")).send().await.unwrap(),
        web.client.post(web.url("/settings")).send().await.unwrap(),
        web.client.get(web.url("/login")).send().await.unwrap(),
        web.get("/status").send().await.unwrap(),
    ] {
        assert_eq!(response.headers()["x-robots-tag"], "noindex, nofollow");
    }
}

// ---- 손님 요청 제한 ----

#[tokio::test]
async fn guest_get_is_limited_to_60_per_minute() {
    let web = start_web(None).await;
    for i in 1..=60 {
        let response = web.client.get(web.url("/")).send().await.unwrap();
        assert_eq!(response.status(), 200, "{i}번째");
    }
    let response = web.client.get(web.url("/")).send().await.unwrap();
    assert_eq!(response.status(), 429);
    assert_eq!(response.headers()["retry-after"], "60");
    assert_eq!(response.headers()["x-robots-tag"], "noindex, nofollow");
    assert_eq!(
        response.text().await.unwrap(),
        "요청이 너무 많습니다. 잠시 뒤 다시 시도하세요."
    );
    // 1분이 지나면 다시 받는다
    web.clock
        .fetch_add(60 * 1000, std::sync::atomic::Ordering::SeqCst);
    let response = web.client.get(web.url("/")).send().await.unwrap();
    assert_eq!(response.status(), 200);
}

#[tokio::test]
async fn members_are_not_limited() {
    let web = start_web(None).await;
    for i in 1..=61 {
        let response = web.get("/").send().await.unwrap();
        assert_eq!(response.status(), 200, "{i}번째");
    }
}

#[tokio::test]
async fn static_files_and_login_are_not_counted() {
    let web = start_web(None).await;
    for _ in 0..70 {
        web.client
            .get(web.url("/static/none.css"))
            .send()
            .await
            .unwrap();
        web.client.get(web.url("/login")).send().await.unwrap();
    }
    let response = web.client.get(web.url("/")).send().await.unwrap();
    assert_eq!(response.status(), 200);
}
