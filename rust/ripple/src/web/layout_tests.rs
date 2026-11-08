use super::*;
use crate::web::test_support::start_web;

#[test]
fn banner_levels() {
    assert!(banner_for(1.0, 5.0).is_none());
    let warn = banner_for(4.0, 5.0).unwrap();
    assert!(!warn.over);
    assert_eq!(warn.text, "이번 달 AI 예산의 80%를 썼습니다($4.00 / $5). ");
    let over = banner_for(5.0, 5.0).unwrap();
    assert!(over.over);
    assert_eq!(over.link, "설정에서 한도 올리기");
}

#[tokio::test]
async fn unknown_path_is_404_page_for_logged_in() {
    let web = start_web(None).await;
    let response = web.get("/nope").send().await.unwrap();
    assert_eq!(response.status(), 404);
    assert!(
        response
            .text()
            .await
            .unwrap()
            .contains("페이지를 찾을 수 없습니다")
    );
}

#[tokio::test]
async fn guest_layout_has_no_logout_or_budget_banner_and_marks_access() {
    let web = start_web(None).await;
    web.writer()
        .execute(
            "INSERT INTO usage_logs (at, provider, model, purpose, input_tokens, output_tokens, cost_usd, stock_id) \
             VALUES (?1, 'openrouter', 'm', 'test', 1, 1, 1000.0, NULL)",
            [crate::web::test_support::T0],
        )
        .unwrap();

    let guest = web.client.get(web.url("/")).send().await.unwrap();
    let guest = guest.text().await.unwrap();
    assert!(guest.contains("data-access=\"guest\""));
    assert!(!guest.contains("action=\"/logout\""));
    assert!(!guest.contains("budget-banner"));
    // 메뉴 네 개는 그대로
    for href in ["href=\"/stocks\"", "href=\"/status\"", "href=\"/settings\""] {
        assert!(guest.contains(href), "{href}");
    }

    let member = web.get("/").send().await.unwrap().text().await.unwrap();
    assert!(member.contains("data-access=\"member\""));
    assert!(member.contains("action=\"/logout\""));
    assert!(member.contains("budget-banner"));
}
