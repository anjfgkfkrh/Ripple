use crate::web::test_support::{PASSWORD, start_web};

async fn post_login(
    web: &crate::web::test_support::TestWeb,
    password: &str,
    ip: &str,
) -> reqwest::Response {
    web.client
        .post(web.url("/login"))
        .header("cf-connecting-ip", ip)
        .header("content-type", "application/x-www-form-urlencoded")
        .body(format!("password={password}&next=%2Fstatus%3Ffrom%3Dlogin"))
        .send()
        .await
        .unwrap()
}

#[tokio::test]
async fn wrong_password_shows_error() {
    let web = start_web(None).await;
    let response = post_login(&web, "wrong-password-123", "a").await;
    assert_eq!(response.status(), 200);
    assert!(
        response
            .text()
            .await
            .unwrap()
            .contains("비밀번호가 맞지 않습니다.")
    );
}

#[tokio::test]
async fn success_sets_cookie_and_redirects_to_next() {
    let web = start_web(None).await;
    let response = post_login(&web, PASSWORD, "b").await;
    assert_eq!(response.status(), 303);
    assert_eq!(response.headers()["location"], "/status?from=login");
    let cookie = response.headers()["set-cookie"]
        .to_str()
        .unwrap()
        .to_string();
    for part in [
        "ripple_session=",
        "Max-Age=2592000",
        "HttpOnly",
        "SameSite=Lax",
        "Path=/",
    ] {
        assert!(cookie.contains(part), "{cookie}");
    }
    assert!(!cookie.contains("Secure"));
    let https = web
        .client
        .post(web.url("/login"))
        .header("x-forwarded-proto", "https")
        .header("content-type", "application/x-www-form-urlencoded")
        .body(format!("password={PASSWORD}"))
        .send()
        .await
        .unwrap();
    assert!(
        https.headers()["set-cookie"]
            .to_str()
            .unwrap()
            .contains("Secure")
    );
}

#[tokio::test]
async fn five_failures_block_even_correct_password() {
    let web = start_web(None).await;
    for _ in 0..4 {
        let text = post_login(&web, "wrong-password", "c")
            .await
            .text()
            .await
            .unwrap();
        assert!(text.contains("비밀번호가 맞지 않습니다."));
    }
    let text = post_login(&web, "wrong-password", "c")
        .await
        .text()
        .await
        .unwrap();
    assert!(text.contains("로그인 시도가 너무 많습니다. 15분 뒤에 다시 시도하세요."));
    let blocked = post_login(&web, PASSWORD, "c").await;
    assert_eq!(blocked.status(), 200);
}

#[tokio::test]
async fn logged_in_login_page_redirects_and_logout_clears() {
    let web = start_web(None).await;
    let response = web.get("/login?next=%2F%3Fagain%3D1").send().await.unwrap();
    assert_eq!(response.status(), 307);
    assert_eq!(response.headers()["location"], "/?again=1");
    let out = web.post("/logout").send().await.unwrap();
    assert_eq!(out.status(), 303);
    assert!(
        out.headers()["set-cookie"]
            .to_str()
            .unwrap()
            .contains("Max-Age=0")
    );
}

async fn post_unlock(
    web: &crate::web::test_support::TestWeb,
    password: &str,
    ip: &str,
) -> reqwest::Response {
    web.client
        .post(web.url("/unlock"))
        .header("cf-connecting-ip", ip)
        .header("content-type", "application/x-www-form-urlencoded")
        .body(format!("password={password}"))
        .send()
        .await
        .unwrap()
}

#[tokio::test]
async fn unlock_with_right_password_gives_cookie_that_can_edit() {
    let web = start_web(None).await;
    let response = post_unlock(&web, PASSWORD, "u1").await;
    assert_eq!(response.status(), 204);
    let set_cookie = response.headers()["set-cookie"].to_str().unwrap();
    assert!(set_cookie.contains("ripple_session="), "{set_cookie}");
    let pair = set_cookie.split(';').next().unwrap().to_string();
    // 새 토큰으로 수정 요청이 가드를 통과한다
    let edit = web
        .client
        .post(web.url("/status/collect"))
        .header("cookie", pair)
        .send()
        .await
        .unwrap();
    assert_ne!(edit.status(), 403);
    assert_ne!(edit.status(), 401);
}

#[tokio::test]
async fn unlock_with_wrong_password_is_401_json() {
    let web = start_web(None).await;
    let response = post_unlock(&web, "wrong-password", "u2").await;
    assert_eq!(response.status(), 401);
    assert!(response.headers().get("set-cookie").is_none());
    assert_eq!(
        response.json::<serde_json::Value>().await.unwrap(),
        serde_json::json!({"error": "비밀번호가 맞지 않습니다."})
    );
}

#[tokio::test]
async fn unlock_failures_share_the_login_record_and_block() {
    let web = start_web(None).await;
    // /login 3번 + /unlock 2번 = 5번 실패 → 막힘
    for _ in 0..3 {
        post_login(&web, "wrong-password", "u3").await;
    }
    for _ in 0..2 {
        post_unlock(&web, "wrong-password", "u3").await;
    }
    let blocked = post_unlock(&web, PASSWORD, "u3").await;
    assert_eq!(blocked.status(), 401);
    let body = blocked.json::<serde_json::Value>().await.unwrap();
    assert_eq!(
        body["error"],
        "로그인 시도가 너무 많습니다. 15분 뒤에 다시 시도하세요."
    );
    // /login도 같은 기록으로 막힌다
    let text = post_login(&web, PASSWORD, "u3").await.text().await.unwrap();
    assert!(text.contains("로그인 시도가 너무 많습니다."));
}
