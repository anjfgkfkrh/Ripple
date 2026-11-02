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
