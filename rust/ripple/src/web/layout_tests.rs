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
