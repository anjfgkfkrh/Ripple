use super::*;
use crate::test_support::test_db;

const NOW: EpochMs = 1_792_000_000_000;

#[test]
fn defaults_when_nothing_saved() {
    // it("저장된 값이 없으면 기본값(D-02 모델, D-04 임시 $5)")
    let db = test_db();
    assert_eq!(LLM_MODEL.default, "anthropic/claude-haiku-5.5");
    assert_eq!(
        read_str(&db.conn, &LLM_MODEL).unwrap(),
        "anthropic/claude-haiku-5.5"
    );
    assert_eq!(read_num(&db.conn, &BUDGET_MONTHLY_USD).unwrap(), 5.0);
}

#[test]
fn written_value_is_read_and_can_be_overwritten() {
    // it("쓴 값을 읽고, 다시 쓰면 바뀐다")
    let db = test_db();
    write_str(&db.conn, &LLM_MODEL, "google/gemini-3.8-flash", NOW).unwrap();
    assert_eq!(
        read_str(&db.conn, &LLM_MODEL).unwrap(),
        "google/gemini-3.8-flash"
    );
    write_str(&db.conn, &LLM_MODEL, "openai/gpt-5.6-luna", NOW + 5).unwrap();
    assert_eq!(
        read_str(&db.conn, &LLM_MODEL).unwrap(),
        "openai/gpt-5.6-luna"
    );
    // 다시 쓰면 updated_at이 갱신된다(client.test.ts "수정하면 updatedAt이 갱신된다"의 Rust 쪽 확인)
    let updated: i64 = db
        .conn
        .query_row(
            "SELECT updated_at FROM settings WHERE key = 'llm.model'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(updated, NOW + 5);
}

#[test]
fn wrong_type_or_broken_json_falls_back_to_default() {
    // it("저장된 값의 형식이 틀리면 기본값")
    let db = test_db();
    db.conn
        .execute("INSERT INTO settings (key, value, updated_at) VALUES ('budget.monthlyUsd', '\"많이\"', 0)", [])
        .unwrap();
    assert_eq!(read_num(&db.conn, &BUDGET_MONTHLY_USD).unwrap(), 5.0);
    db.conn
        .execute("UPDATE settings SET value = 'not json'", [])
        .unwrap();
    assert_eq!(read_num(&db.conn, &BUDGET_MONTHLY_USD).unwrap(), 5.0);
}

#[test]
fn list_setting_is_used_only_when_all_items_are_strings() {
    // it("목록 설정(토픽 갱신 시각)은 글자 목록일 때만 쓰고 아니면 기본값(11단계)")
    let db = test_db();
    assert_eq!(
        read_list(&db.conn, &TOPICS_TIMES).unwrap(),
        Vec::<String>::new()
    );
    write_list(
        &db.conn,
        &TOPICS_TIMES,
        &["08:30".to_string(), "15:40".to_string()],
        NOW,
    )
    .unwrap();
    assert_eq!(
        read_list(&db.conn, &TOPICS_TIMES).unwrap(),
        ["08:30", "15:40"]
    );
    db.conn
        .execute("UPDATE settings SET value = '[8, 30]'", [])
        .unwrap();
    assert_eq!(
        read_list(&db.conn, &TOPICS_TIMES).unwrap(),
        Vec::<String>::new()
    );
    db.conn
        .execute("UPDATE settings SET value = '\"08:30\"'", [])
        .unwrap();
    assert_eq!(
        read_list(&db.conn, &TOPICS_TIMES).unwrap(),
        Vec::<String>::new()
    );
}

#[test]
fn numbers_are_stored_like_json_stringify() {
    // Rust에서만 생기는 경우(설계 3.9·8장): 정수 값은 `5`로, 소수는 짧은 표현으로 쓴다. 글자 목록도 JS와 같은 모양.
    let db = test_db();
    write_num(&db.conn, &BUDGET_MONTHLY_USD, 5.0, NOW).unwrap();
    write_num(&db.conn, &JEV_USD_PER_MILLION_INPUT_TOKENS, 0.042, NOW).unwrap();
    write_list(&db.conn, &TOPICS_TIMES, &["08:30".to_string()], NOW).unwrap();
    write_str(&db.conn, &TOPICS_ERROR, "줄\n바꿈 \"따옴표\"", NOW).unwrap();
    let stored = |key: &str| -> String {
        db.conn
            .query_row("SELECT value FROM settings WHERE key = ?1", [key], |r| {
                r.get(0)
            })
            .unwrap()
    };
    assert_eq!(stored("budget.monthlyUsd"), "5");
    assert_eq!(stored("jev.usdPerMillionInputTokens"), "0.042");
    assert_eq!(stored("topics.times"), r#"["08:30"]"#);
    assert_eq!(stored("topics.error"), r#""줄\n바꿈 \"따옴표\"""#);
    assert_eq!(
        read_str(&db.conn, &TOPICS_ERROR).unwrap(),
        "줄\n바꿈 \"따옴표\""
    );
}
