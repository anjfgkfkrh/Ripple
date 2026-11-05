use serde_json::json;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use super::*;
use crate::web::test_support::{TestWeb, start_web};

async fn post_form(web: &TestWeb, path: &str, fields: &[(&str, &str)]) -> (u16, String) {
    let body = url::form_urlencoded::Serializer::new(String::new())
        .extend_pairs(fields)
        .finish();
    let response = web
        .post(path)
        .header("content-type", "application/x-www-form-urlencoded")
        .body(body)
        .send()
        .await
        .unwrap();
    let status = response.status().as_u16();
    (status, response.text().await.unwrap())
}

/// 저장 폼의 12개 입력(화면이 보내는 그대로)
fn form_fields() -> Vec<(&'static str, &'static str)> {
    vec![
        ("collectIntervalMinutes", "60"),
        ("keywordsDepth", "2"),
        ("keywordsRegenerateEvery", "daily"),
        ("minRelevance", "6"),
        ("minMagnitude", "6"),
        ("keywordMinRelevance", "6"),
        ("keywordMinMagnitude", "5"),
        ("topicIntervalHours", "3"),
        ("topicTimes", "08:30, 15:40"),
        ("maxPerRun", "200"),
        ("monthlyBudgetUsd", "7.5"),
        ("llmModel", "anthropic/claude-haiku-5.5"),
    ]
}

#[test]
fn thousands_use_commas_like_ko_kr() {
    assert_eq!(group_thousands(0), "0");
    assert_eq!(group_thousands(999), "999");
    assert_eq!(group_thousands(1234), "1,234");
    assert_eq!(group_thousands(-1234567), "-1,234,567");
}

#[test]
fn share_text_matches_the_ts_rules() {
    assert_eq!(format_share(0.0), "0%");
    assert_eq!(format_share(0.004), "<1%");
    assert_eq!(format_share(0.005), "1%");
    assert_eq!(format_share(0.5), "50%");
    assert_eq!(format_share(0.125), "13%");
}

#[test]
fn percent_rounds_half_up_like_math_round() {
    assert_eq!(round_js(0.5), 1);
    assert_eq!(round_js(2.4), 2);
    assert_eq!(round_js(99.5), 100);
}

#[test]
fn provider_and_group_names_match_the_ts_labels() {
    assert_eq!(provider_label(UsageProvider::Openrouter), "OpenRouter");
    assert_eq!(provider_label(UsageProvider::Typesafe), "TypeSafe(Jev)");
    assert_eq!(group_short(ItemGroup::Unknown), "기록 없음");
}

#[test]
fn values_from_the_db_are_shown_as_js_would_print_them() {
    let form = SettingsForm {
        collect_interval_minutes: 30.0,
        keywords_depth: 2.0,
        keywords_regenerate_every: crate::server::settings_form::RegenerateEvery::Weekly,
        min_relevance: 6.5,
        min_magnitude: 6.0,
        keyword_min_relevance: 6.0,
        keyword_min_magnitude: 5.0,
        topic_interval_hours: 0.0,
        topic_times: vec!["08:30".to_string(), "15:40".to_string()],
        max_per_run: 200.0,
        monthly_budget_usd: 5.0,
        llm_model: "m".to_string(),
    };
    let values = SettingsValues::from_form(&form);
    assert_eq!(values.collect_interval_minutes, "30");
    assert_eq!(values.keywords_regenerate_every, "weekly");
    assert_eq!(values.min_relevance, "6.5");
    assert_eq!(values.topic_interval_hours, "0");
    assert_eq!(values.topic_times, "08:30, 15:40");
}

#[tokio::test]
async fn settings_page_shows_values_jev_model_and_usage() {
    let web = start_web(None).await;
    let body = web
        .get("/settings")
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    for part in [
        "data-testid=\"settings-form\"",
        "name=\"collectIntervalMinutes\"",
        "<option value=\"30\" selected>30분</option>",
        "value=\"anthropic/claude-haiku-5.5\"",
        "토픽 갱신 간격(시간, 0~24)",
        "jev-1.13.0",
        "이번 달 AI 사용량",
        "data-testid=\"usage\"",
        "data-testid=\"keyword-rebuild\"",
        "이번 달에는 아직 AI를 부르지 않았습니다.",
    ] {
        assert!(body.contains(part), "{part}");
    }
}

#[tokio::test]
async fn settings_page_needs_a_login() {
    let web = start_web(None).await;
    let response = web.client.get(web.url("/settings")).send().await.unwrap();
    assert_eq!(response.status().as_u16(), 307);
}

#[tokio::test]
async fn save_passes_all_twelve_values_and_refreshes_usage_and_banner() {
    let worker = MockServer::start().await;
    Mock::given(method("PUT"))
        .and(path("/settings"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({ "ok": true, "changed": ["collect.intervalMinutes"] })),
        )
        .mount(&worker)
        .await;
    let web = start_web(Some(worker.uri())).await;
    let (status, body) = post_form(&web, "/settings", &form_fields()).await;
    assert_eq!(status, 200);
    assert!(
        body.contains(
            "<p role=\"status\" class=\"text-sm text-muted-foreground\">저장했습니다.</p>"
        )
    );
    assert!(body.contains("id=\"usage-section\""));
    assert!(body.contains("hx-swap-oob=\"true\""));
    assert!(body.contains("id=\"budget-slot\" hx-swap-oob=\"true\""));
    let received = worker.received_requests().await.unwrap();
    let sent: serde_json::Value = serde_json::from_slice(&received[0].body).unwrap();
    assert_eq!(sent.as_object().unwrap().len(), 12);
    assert_eq!(sent["monthlyBudgetUsd"], "7.5");
    assert_eq!(sent["topicTimes"], "08:30, 15:40");
}

#[tokio::test]
async fn save_with_nothing_changed_says_so() {
    let worker = MockServer::start().await;
    Mock::given(method("PUT"))
        .and(path("/settings"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(json!({ "ok": true, "changed": [] })),
        )
        .mount(&worker)
        .await;
    let web = start_web(Some(worker.uri())).await;
    let (_, body) = post_form(&web, "/settings", &form_fields()).await;
    assert!(body.contains("바뀐 값이 없습니다."));
}

#[tokio::test]
async fn invalid_values_show_per_field_errors_and_keep_what_was_typed() {
    let worker = MockServer::start().await;
    Mock::given(method("PUT"))
        .and(path("/settings"))
        .respond_with(ResponseTemplate::new(422).set_body_json(json!({
            "error": "invalid_settings",
            "fields": { "maxPerRun": "회차당 최대 평가 수는 10~1000 사이로 넣으세요." }
        })))
        .mount(&worker)
        .await;
    let web = start_web(Some(worker.uri())).await;
    let mut fields = form_fields();
    fields[9] = ("maxPerRun", "5");
    let (_, body) = post_form(&web, "/settings", &fields).await;
    assert!(body.contains("data-testid=\"maxPerRun-error\""));
    assert!(body.contains("회차당 최대 평가 수는 10~1000 사이로 넣으세요."));
    assert!(body.contains("고칠 항목이 있어 저장하지 않았습니다."));
    assert!(body.contains("value=\"5\""));
    // 실패하면 사용량과 배너는 다시 그리지 않는다
    assert!(!body.contains("id=\"usage-section\""));
}

#[tokio::test]
async fn unknown_model_and_model_check_errors_are_on_the_model_field() {
    let worker = MockServer::start().await;
    Mock::given(method("PUT"))
        .and(path("/settings"))
        .respond_with(ResponseTemplate::new(422).set_body_json(json!({ "error": "unknown_model" })))
        .mount(&worker)
        .await;
    let web = start_web(Some(worker.uri())).await;
    let (_, body) = post_form(&web, "/settings", &form_fields()).await;
    assert!(body.contains("OpenRouter에 없는 모델 ID입니다."));
    assert!(body.contains("data-testid=\"llmModel-error\""));

    let worker = MockServer::start().await;
    Mock::given(method("PUT"))
        .and(path("/settings"))
        .respond_with(
            ResponseTemplate::new(502).set_body_json(json!({ "error": "model_check_failed" })),
        )
        .mount(&worker)
        .await;
    let web = start_web(Some(worker.uri())).await;
    let (_, body) = post_form(&web, "/settings", &form_fields()).await;
    assert!(body.contains("OpenRouter 모델 목록을 확인하지 못했습니다. 잠시 뒤 다시 저장하세요."));
    assert!(body.contains("저장하지 않았습니다."));
}

#[tokio::test]
async fn rebuild_reports_how_many_stocks_it_queued() {
    let worker = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/keywords/rebuild"))
        .respond_with(ResponseTemplate::new(202).set_body_json(json!({ "count": 2 })))
        .mount(&worker)
        .await;
    let web = start_web(Some(worker.uri())).await;
    let body = web
        .post("/settings/rebuild")
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(body.contains(
        "2개 종목의 키워드를 다시 만들기 시작했습니다. 종목 화면에서 진행을 볼 수 있습니다."
    ));
    assert!(body.contains("id=\"keyword-rebuild\""));
}

#[tokio::test]
async fn rebuild_without_a_worker_says_it_is_unreachable() {
    let web = start_web(None).await;
    let body = web
        .post("/settings/rebuild")
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(body.contains("worker에 연결할 수 없습니다."));
    assert!(body.contains("disabled"));
}
