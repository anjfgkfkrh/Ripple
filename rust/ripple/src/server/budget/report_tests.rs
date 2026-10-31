use rusqlite::params;

use super::*;
use crate::server::ai::llm::LlmUsage;
use crate::server::budget::usage::{UsageEntry, record_usage};
use crate::server::settings::{BUDGET_MONTHLY_USD, write_num};
use crate::test_support::{ms, test_db};

// 한국 시간 2026-10-21 12:00
fn now() -> EpochMs {
    ms("2026-10-21T03:00:00Z")
}

fn log(
    conn: &Connection,
    provider: UsageProvider,
    purpose: &str,
    cost_usd: f64,
    at: EpochMs,
    stock_id: Option<i64>,
) {
    record_usage(
        conn,
        &UsageEntry {
            provider,
            model: "m",
            purpose,
            usage: LlmUsage {
                input_tokens: 1,
                output_tokens: 1,
                cost_usd,
            },
            at,
            stock_id,
        },
    )
    .unwrap();
}

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-9
}

// 이 파일의 항목 시험은 `topics/store`(R7)의 함수 대신 같은 모양의 행을 SQL로 직접 넣는다.
fn insert_stock(
    conn: &Connection,
    kind: &str,
    market: &str,
    code: &str,
    name: &str,
    topic_id: Option<i64>,
) -> i64 {
    conn.execute(
        "INSERT INTO stocks (market, code, name, name_ko, exchange, user_input, created_at, kind, topic_id) \
         VALUES (?1, ?2, ?3, ?3, '거래소', ?3, ?4, ?5, ?6)",
        params![market, code, name, now(), kind, topic_id],
    )
    .unwrap();
    conn.last_insert_rowid()
}

fn insert_topic(conn: &Connection, source: &str, name: &str) -> i64 {
    conn.execute(
        "INSERT INTO topics (source, name, terms, created_at) VALUES (?1, ?2, '[]', ?3)",
        params![source, name, now()],
    )
    .unwrap();
    conn.last_insert_rowid()
}

// describe("budgetLevel (D-07)")
#[test]
fn level_warns_from_80_percent_and_stops_at_100() {
    // it("80%부터 미리 알림, 100%부터 멈춤")
    assert_eq!(budget_level(3.99, 5.0), BudgetLevel::Ok);
    assert_eq!(budget_level(4.0, 5.0), BudgetLevel::Warn);
    assert_eq!(budget_level(5.0, 5.0), BudgetLevel::Over);
    assert_eq!(budget_level(6.0, 5.0), BudgetLevel::Over);
}

// describe("monthUsageReport (FR-8)")
#[test]
fn groups_this_month_by_provider_and_purpose_largest_first_with_limit_and_level() {
    // it("이번 달 기록만 제공자·용도별로 묶어 비용이 큰 것부터, 한도와 단계를 함께")
    let db = test_db();
    let c = &db.conn;
    log(
        c,
        UsageProvider::Typesafe,
        "article_evaluation",
        0.01,
        now(),
        None,
    );
    log(
        c,
        UsageProvider::Typesafe,
        "article_evaluation",
        0.02,
        now(),
        None,
    );
    log(
        c,
        UsageProvider::Openrouter,
        "keyword_generation",
        0.005,
        now(),
        None,
    );
    log(
        c,
        UsageProvider::Openrouter,
        "stock_match",
        0.001,
        now(),
        None,
    );
    // 지난달(한국 9월 30일 23:00)
    log(
        c,
        UsageProvider::Openrouter,
        "stock_match",
        1.0,
        ms("2026-09-30T14:00:00Z"),
        None,
    );
    write_num(c, &BUDGET_MONTHLY_USD, 0.04, now()).unwrap();

    let report = month_usage_report(c, now()).unwrap();
    let rows: Vec<(UsageProvider, &str, &str, i64)> = report
        .rows
        .iter()
        .map(|r| (r.provider, r.purpose.as_str(), r.label.as_str(), r.calls))
        .collect();
    assert_eq!(
        rows,
        [
            (
                UsageProvider::Typesafe,
                "article_evaluation",
                "기사 평가",
                2
            ),
            (
                UsageProvider::Openrouter,
                "keyword_generation",
                "키워드 만들기",
                1
            ),
            (UsageProvider::Openrouter, "stock_match", "종목 찾기", 1),
        ]
    );
    assert!(close(report.rows[0].cost_usd, 0.03) && close(report.rows[0].share, 0.03 / 0.036));
    assert!(close(report.rows[1].cost_usd, 0.005) && close(report.rows[1].share, 0.005 / 0.036));
    assert!(close(report.rows[2].cost_usd, 0.001) && close(report.rows[2].share, 0.001 / 0.036));
    assert!(close(report.total_usd, 0.036));
    assert_eq!(report.limit_usd, 0.04);
    assert_eq!(report.level, BudgetLevel::Warn);
}

#[test]
fn market_issues_shown_separately_and_same_label_checks_merge_per_provider() {
    // it("시장 이슈 정리는 따로 보이고, 화면 이름이 같은 품질 확인은 제공자마다 한 줄로 합친다(10단계)")
    let db = test_db();
    let c = &db.conn;
    log(
        c,
        UsageProvider::Openrouter,
        "market_issues",
        0.007,
        now(),
        None,
    );
    log(
        c,
        UsageProvider::Openrouter,
        "keyword_generation_check",
        0.003,
        now(),
        None,
    );
    log(
        c,
        UsageProvider::Openrouter,
        "market_issues_check",
        0.002,
        now(),
        None,
    );
    log(
        c,
        UsageProvider::Openrouter,
        "model_compare",
        0.001,
        now(),
        None,
    );
    log(
        c,
        UsageProvider::Typesafe,
        "article_evaluation_check",
        0.004,
        now(),
        None,
    );

    let rows: Vec<(UsageProvider, String, i64, String)> = month_usage_report(c, now())
        .unwrap()
        .rows
        .iter()
        .map(|r| {
            (
                r.provider,
                r.label.clone(),
                r.calls,
                format!("{:.6}", r.cost_usd),
            )
        })
        .collect();
    assert_eq!(
        rows,
        [
            (
                UsageProvider::Openrouter,
                "시장 이슈 정리".to_string(),
                1,
                "0.007000".to_string()
            ),
            (
                UsageProvider::Openrouter,
                "품질 확인(개발 중 직접 호출)".to_string(),
                3,
                "0.006000".to_string()
            ),
            (
                UsageProvider::Typesafe,
                "품질 확인(개발 중 직접 호출)".to_string(),
                1,
                "0.004000".to_string()
            ),
        ]
    );
}

// describe("monthUsageReport: 항목별 비용 (11단계)")
#[test]
fn per_item_costs_group_by_stock_market_topic_and_shared() {
    // it("종목·시장·주제·공용으로 묶고 비율을 낸다. 같은 주제의 증시별 항목과 자동 토픽은 한 줄로")
    let db = test_db();
    let c = &db.conn;
    let stock = insert_stock(c, "stock", "KR", "005930", "삼성전자", None);
    let market = insert_stock(c, "market", "KR", "MARKET", "한국 증시", None);
    let user_topic = insert_topic(c, "user", "부동산 시장");
    // 사용자 주제: 한국·미국 기준 항목 두 줄을 한 주제로
    let user_kr = insert_stock(
        c,
        "topic",
        "KR",
        &format!("TOPIC-{user_topic}"),
        "부동산 시장",
        Some(user_topic),
    );
    let user_us = insert_stock(
        c,
        "topic",
        "US",
        &format!("TOPIC-{user_topic}"),
        "부동산 시장",
        Some(user_topic),
    );
    let auto_a_topic = insert_topic(c, "auto", "중동 긴장");
    let auto_a = insert_stock(
        c,
        "topic",
        "KR",
        &format!("TOPIC-{auto_a_topic}"),
        "중동 긴장",
        Some(auto_a_topic),
    );
    let auto_b_topic = insert_topic(c, "auto", "기준금리");
    let auto_b = insert_stock(
        c,
        "topic",
        "KR",
        &format!("TOPIC-{auto_b_topic}"),
        "기준금리",
        Some(auto_b_topic),
    );

    use UsageProvider::{Openrouter, Typesafe};
    log(c, Typesafe, "article_evaluation", 0.006, now(), Some(stock));
    log(c, Typesafe, "keyword_rating", 0.001, now(), Some(stock));
    log(
        c,
        Openrouter,
        "keyword_generation",
        0.002,
        now(),
        Some(market),
    );
    log(
        c,
        Typesafe,
        "article_evaluation",
        0.001,
        now(),
        Some(user_kr),
    );
    log(
        c,
        Typesafe,
        "article_evaluation",
        0.001,
        now(),
        Some(user_us),
    );
    log(
        c,
        Typesafe,
        "article_evaluation",
        0.0005,
        now(),
        Some(auto_a),
    );
    log(
        c,
        Typesafe,
        "article_evaluation",
        0.0005,
        now(),
        Some(auto_b),
    );
    log(c, Openrouter, "topic_refresh", 0.005, now(), None);
    // 11단계 전 기록(항목을 남기지 않음)
    log(c, Typesafe, "article_evaluation", 0.003, now(), None);

    let report = month_usage_report(c, now()).unwrap();
    let total = 0.02;
    assert!(close(report.total_usd, total));
    let groups: Vec<(&str, i64, String)> = report
        .groups
        .iter()
        .map(|g| (g.label, g.calls, format!("{:.6}", g.cost_usd)))
        .collect();
    assert_eq!(
        groups,
        [
            ("종목", 2, "0.007000".to_string()),
            ("시장", 1, "0.002000".to_string()),
            ("주제·토픽", 4, "0.003000".to_string()),
            ("공용", 1, "0.005000".to_string()),
            ("항목 기록 없음(11단계 전)", 1, "0.003000".to_string()),
        ]
    );
    assert!(close(report.groups[0].share, 0.007 / total));
    let items: Vec<(&str, ItemGroup, i64, String)> = report
        .items
        .iter()
        .map(|i| {
            (
                i.name.as_str(),
                i.group,
                i.calls,
                format!("{:.6}", i.cost_usd),
            )
        })
        .collect();
    assert_eq!(
        items,
        [
            ("삼성전자", ItemGroup::Stock, 2, "0.007000".to_string()),
            (
                "공용(토픽 갱신·시장 이슈·종목 찾기 등)",
                ItemGroup::Shared,
                1,
                "0.005000".to_string()
            ),
            (
                "항목 기록 없음(11단계 전)",
                ItemGroup::Unknown,
                1,
                "0.003000".to_string()
            ),
            ("부동산 시장", ItemGroup::Topic, 2, "0.002000".to_string()),
            ("한국 증시", ItemGroup::Market, 1, "0.002000".to_string()),
            (
                "자동 토픽(2개)",
                ItemGroup::Topic,
                2,
                "0.001000".to_string()
            ),
        ]
    );
    assert!(close(
        report.items.iter().map(|i| i.share).sum::<f64>(),
        1.0
    ));
}

#[test]
fn deleted_item_stays_with_name_and_deleted_flag() {
    // it("지운 항목도 이름과 '삭제함' 표시로 남는다")
    let db = test_db();
    let c = &db.conn;
    let topic = insert_topic(c, "user", "중동 정세");
    let lens = insert_stock(
        c,
        "topic",
        "KR",
        &format!("TOPIC-{topic}"),
        "중동 정세",
        Some(topic),
    );
    log(
        c,
        UsageProvider::Typesafe,
        "article_evaluation",
        0.001,
        now(),
        Some(lens),
    );
    c.execute(
        "UPDATE topics SET deleted_at = ?1 WHERE id = ?2",
        params![now(), topic],
    )
    .unwrap();

    let items = month_usage_report(c, now()).unwrap().items;
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].name, "중동 정세");
    assert!(items[0].deleted);
}

#[test]
fn unknown_purpose_uses_its_own_text_as_label() {
    // Rust에서만 필요한 점검: 화면 이름이 없는 용도는 용도 글자 그대로 보인다(`PURPOSE_LABEL[purpose] ?? purpose`).
    let db = test_db();
    log(
        &db.conn,
        UsageProvider::Openrouter,
        "something_new",
        0.001,
        now(),
        None,
    );
    let report = month_usage_report(&db.conn, now()).unwrap();
    assert_eq!(report.rows[0].label, "something_new");
}
