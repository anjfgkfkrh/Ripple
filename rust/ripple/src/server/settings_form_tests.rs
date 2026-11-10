//! TS에는 `settings-form.ts`의 시험이 따로 없어(워커 API 시험이 대신 부른다) 검사 규칙의 경계를 Rust 시험으로 더했다.
use serde_json::{Value, json};

use super::*;
use crate::test_support::{ms, test_db};

fn valid() -> Value {
    json!({
        "collectIntervalMinutes": "60",
        "keywordsDepth": "1",
        "keywordsRegenerateEvery": "daily",
        "minRelevance": "6.5",
        "minMagnitude": "5",
        "keywordMinRelevance": "7",
        "keywordMinMagnitude": "5",
        "topicIntervalHours": "4",
        "topicTimes": "15:40, 8:30,08:30",
        "maxPerRun": "150",
        "monthlyBudgetUsd": "7.5",
        "llmModel": "anthropic/claude-haiku-5.5",
    })
}

fn with(key: &str, value: Value) -> Value {
    let mut body = valid();
    body[key] = value;
    body
}

fn first_error(body: &Value, field: SettingsField) -> Option<String> {
    parse_settings_form(body).err().and_then(|errors| {
        errors
            .into_iter()
            .find(|(f, _)| *f == field)
            .map(|(_, m)| m)
    })
}

#[test]
fn parses_text_numbers_and_normalizes_times() {
    let form = parse_settings_form(&valid()).unwrap();
    assert_eq!(form.collect_interval_minutes, 60.0);
    assert_eq!(form.keywords_regenerate_every, RegenerateEvery::Daily);
    assert_eq!(form.min_relevance, 6.5);
    // 한 자리 시각은 두 자리로, 겹치는 시각은 하나로, 시각 순으로
    assert_eq!(form.topic_times, vec!["08:30", "15:40"]);
    assert_eq!(form.monthly_budget_usd, 7.5);
}

#[test]
fn rounds_scores_to_one_decimal_and_budget_to_cents() {
    let mut body = with("minRelevance", json!("6.55"));
    body["monthlyBudgetUsd"] = json!(7.456);
    let form = parse_settings_form(&body).unwrap();
    assert_eq!(form.min_relevance, 6.6);
    assert_eq!(form.monthly_budget_usd, 7.46);
}

#[test]
fn accepts_numbers_and_other_js_coercions() {
    // z.coerce.number(): 숫자·true·null·[5]도 Number()처럼 읽는다
    let mut body = valid();
    body["keywordsDepth"] = json!(2);
    body["topicIntervalHours"] = Value::Null;
    body["maxPerRun"] = json!([50]);
    let form = parse_settings_form(&body).unwrap();
    assert_eq!(form.keywords_depth, 2.0);
    assert_eq!(form.topic_interval_hours, 0.0);
    assert_eq!(form.max_per_run, 50.0);
}

#[test]
fn reports_first_message_per_field_in_schema_order() {
    let body = json!({
        "collectIntervalMinutes": "45",
        "minRelevance": "11",
        "minMagnitude": "-1",
        "topicIntervalHours": "1.5",
        "topicTimes": "09:00, 25:00",
        "llmModel": "모델",
    });
    let errors = parse_settings_form(&body).unwrap_err();
    let keys: Vec<_> = errors.iter().map(|(f, _)| f.key()).collect();
    assert_eq!(
        keys,
        vec![
            "collectIntervalMinutes",
            "keywordsDepth",
            "keywordsRegenerateEvery",
            "minRelevance",
            "minMagnitude",
            "keywordMinRelevance",
            "keywordMinMagnitude",
            "topicIntervalHours",
            "topicTimes",
            "maxPerRun",
            "monthlyBudgetUsd",
            "llmModel",
        ]
    );
    let message = |field| errors.iter().find(|(f, _)| *f == field).unwrap().1.clone();
    assert_eq!(
        message(SettingsField::CollectIntervalMinutes),
        "15·30·60·180분 중에서 고르세요."
    );
    assert_eq!(
        message(SettingsField::MinRelevance),
        "0~10 사이로 넣으세요."
    );
    assert_eq!(
        message(SettingsField::TopicIntervalHours),
        "0~24 사이 정수로 넣으세요."
    );
    // 값이 없는 항목은 zod 기본 메시지
    assert_eq!(
        message(SettingsField::KeywordsDepth),
        "Invalid input: expected number, received NaN"
    );
    assert_eq!(message(SettingsField::MaxPerRun), "숫자를 넣으세요.");
    assert_eq!(
        message(SettingsField::KeywordsRegenerateEvery),
        "하루 1회나 일주일 1회를 고르세요."
    );
}

#[test]
fn zod_default_messages_for_type_errors() {
    assert_eq!(
        first_error(&with("llmModel", json!(5)), SettingsField::LlmModel).unwrap(),
        "Invalid input: expected string, received number"
    );
    assert_eq!(
        first_error(&with("llmModel", json!([])), SettingsField::LlmModel).unwrap(),
        "Invalid input: expected string, received array"
    );
    assert_eq!(
        first_error(
            &with("keywordsDepth", json!("1e999")),
            SettingsField::KeywordsDepth
        )
        .unwrap(),
        "Invalid input: expected number, received Infinity"
    );
    assert_eq!(
        first_error(&with("topicTimes", json!(5)), SettingsField::TopicTimes).unwrap(),
        "09:00, 15:30처럼 넣으세요."
    );
}

#[test]
fn field_limits() {
    assert_eq!(
        first_error(
            &with("keywordsDepth", json!("0.5")),
            SettingsField::KeywordsDepth
        )
        .unwrap(),
        "1~2 사이 정수로 넣으세요."
    );
    assert_eq!(
        first_error(
            &with("keywordsDepth", json!("3")),
            SettingsField::KeywordsDepth
        )
        .unwrap(),
        "1~2 사이로 넣으세요."
    );
    assert_eq!(
        first_error(&with("maxPerRun", json!("9")), SettingsField::MaxPerRun).unwrap(),
        "10~1000 사이로 넣으세요."
    );
    assert_eq!(
        first_error(&with("maxPerRun", json!("10.5")), SettingsField::MaxPerRun).unwrap(),
        "정수로 넣으세요."
    );
    assert_eq!(
        first_error(
            &with("monthlyBudgetUsd", json!("0.05")),
            SettingsField::MonthlyBudgetUsd
        )
        .unwrap(),
        "$0.1~$1000 사이로 넣으세요."
    );
    assert_eq!(
        first_error(&with("llmModel", json!("   ")), SettingsField::LlmModel).unwrap(),
        "모델 ID를 넣으세요."
    );
    assert_eq!(
        first_error(
            &with("llmModel", json!("a".repeat(201))),
            SettingsField::LlmModel
        )
        .unwrap(),
        "모델 ID가 너무 깁니다."
    );
    assert_eq!(
        first_error(
            &with("topicTimes", json!("x".repeat(201))),
            SettingsField::TopicTimes
        )
        .unwrap(),
        "너무 깁니다."
    );
    let many = (0..13)
        .map(|h| format!("{h:02}:00"))
        .collect::<Vec<_>>()
        .join(",");
    assert_eq!(
        first_error(&with("topicTimes", json!(many)), SettingsField::TopicTimes).unwrap(),
        "12개까지 넣을 수 있습니다."
    );
}

#[test]
fn trims_model_and_allows_empty_time_list() {
    let mut body = with("llmModel", json!("  ~typesafe/jev-latest  "));
    body["topicTimes"] = json!("");
    let form = parse_settings_form(&body).unwrap();
    assert_eq!(form.llm_model, "~typesafe/jev-latest");
    assert!(form.topic_times.is_empty());
}

#[test]
fn non_object_body_is_invalid_with_no_fields() {
    for body in [Value::Null, json!([]), json!("x")] {
        assert_eq!(parse_settings_form(&body).unwrap_err(), Vec::new());
    }
}

#[test]
fn writes_all_and_reports_changed_fields_in_order() {
    let db = test_db();
    let now = ms("2026-11-02T00:00:00Z");
    let before = read_settings_form(&db.conn).unwrap();
    assert_eq!(before.collect_interval_minutes, 30.0);
    let form = parse_settings_form(&valid()).unwrap();
    let changed = write_settings_form(&db.conn, &form, now).unwrap();
    let keys: Vec<_> = changed.iter().map(|f| f.key()).collect();
    assert_eq!(
        keys,
        vec![
            "collectIntervalMinutes",
            "keywordsDepth",
            "keywordsRegenerateEvery",
            "minRelevance",
            "keywordMinRelevance",
            "topicIntervalHours",
            "topicTimes",
            "maxPerRun",
            "monthlyBudgetUsd",
        ]
    );
    assert_eq!(read_settings_form(&db.conn).unwrap(), form);
    // 같은 값을 다시 저장하면 바뀐 항목이 없다
    assert!(
        write_settings_form(&db.conn, &form, now)
            .unwrap()
            .is_empty()
    );
}

#[test]
fn depth_accepts_only_one_or_two() {
    for ok in ["1", "2"] {
        assert!(
            first_error(
                &with("keywordsDepth", json!(ok)),
                SettingsField::KeywordsDepth
            )
            .is_none()
        );
    }
    for bad in ["3", "4", "0"] {
        assert_eq!(
            first_error(
                &with("keywordsDepth", json!(bad)),
                SettingsField::KeywordsDepth
            )
            .unwrap(),
            "1~2 사이로 넣으세요.",
            "{bad}"
        );
    }
}

#[test]
fn form_defaults_and_old_depth_shows_as_two() {
    let db = test_db();
    let form = read_settings_form(&db.conn).unwrap();
    assert_eq!(form.keywords_depth, 2.0);
    assert_eq!(form.keywords_regenerate_every, RegenerateEvery::Weekly);
    assert_eq!(form.min_magnitude, 5.0);
    assert_eq!(form.keyword_min_relevance, 6.0);
    assert_eq!(form.keyword_min_magnitude, 5.0);
    // 17단계 전에 저장한 깊이 3·4는 화면에 2로 보인다
    for old in ["3", "4"] {
        db.conn
            .execute(
                "INSERT OR REPLACE INTO settings (key, value, updated_at) VALUES ('keywords.depth', ?1, 0)",
                [old],
            )
            .unwrap();
        assert_eq!(read_settings_form(&db.conn).unwrap().keywords_depth, 2.0);
    }
}
