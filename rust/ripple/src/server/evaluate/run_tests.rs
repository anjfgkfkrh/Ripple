use std::sync::atomic::{AtomicBool, Ordering};

use super::*;
use crate::server::ai::decider::types::{DeciderError, DeciderErrorKind};
use crate::server::budget::usage::record_usage;
use crate::server::settings::{
    BUDGET_MONTHLY_USD, EVALUATION_MAX_PER_RUN, EVALUATION_MIN_MAGNITUDE, EVALUATION_MIN_RELEVANCE,
    JEV_MODEL, write_num, write_str,
};
use crate::server::stocks::repository::{
    RegisterInput, RegisterResult, delete_stock, register_stock,
};
use crate::test_support::{
    FakeDecider, SharedTestDb, fake_article_decider, fake_decision, fake_merged_decision, ms,
    seed_lens, seed_listings, seed_market, seed_topic, shared_test_db,
};

// 한국 시간 2026-10-20 12:00
fn now() -> EpochMs {
    ms("2026-10-20T03:00:00Z")
}

const HOUR: i64 = 60 * 60 * 1000;

fn ago(hours: i64) -> EpochMs {
    now() - hours * HOUR
}

struct Ids {
    samsung: i64,
    nvidia: i64,
}

fn seeded() -> SharedTestDb {
    let shared = shared_test_db();
    seed_listings(&mut shared.db.lock().unwrap());
    shared
}

fn add_stocks(shared: &SharedTestDb) -> Ids {
    let conn = shared.db.lock().unwrap();
    let register =
        |market: Market, code: &str, user_input: &str, name_ko: Option<&str>| match register_stock(
            &conn,
            &RegisterInput {
                market,
                code,
                user_input,
                name_ko,
            },
            crate::server::time::now_ms(),
        )
        .unwrap()
        {
            RegisterResult::Created(stock) => stock.id,
            _ => panic!("표본에 없음"),
        };
    Ids {
        samsung: register(Market::Kr, "005930", "삼전", None),
        nvidia: register(Market::Us, "NVDA", "NVDA", Some("엔비디아")),
    }
}

/// 서로 다른 소식의 제목. 같은 낱말이 많이 겹치면 같은 소식으로 보고 앞선 평가를 그대로 쓰므로(11단계)
/// 평가 수를 세는 시험에는 이것을 쓴다.
const DISTINCT: [&str; 8] = [
    "반도체 수출 급증",
    "노조 파업 협상",
    "스마트폰 신제품 공개",
    "파운드리 대형 수주",
    "배당 확대 발표",
    "환율 하락 영향",
    "인도 공장 착공",
    "특허 소송 승소",
];

fn add_link(
    shared: &SharedTestDb,
    stock_id: i64,
    title: &str,
    published_at: EpochMs,
    kind: &str,
    path: &str,
    reason: Option<&str>,
) -> i64 {
    static SERIAL: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let serial = SERIAL.fetch_add(1, Ordering::SeqCst) + 1;
    let conn = shared.db.lock().unwrap();
    conn.execute(
        "INSERT INTO articles (url_key, url, title, summary, publisher, feed, published_at, collected_at) \
         VALUES (?1, ?2, ?3, ?4, '가경제', 'rss:a:economy', ?5, ?6)",
        rusqlite::params![format!("n.example/{serial}"), format!("https://n.example/{serial}"), title, format!("{title} 요약"), published_at, now()],
    )
    .unwrap();
    let article_id = conn.last_insert_rowid();
    conn.execute(
        "INSERT INTO article_matches (article_id, stock_id, keyword_path, keyword_reason, match_kind, level, created_at) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        rusqlite::params![article_id, stock_id, path, reason, kind, i64::from(kind == "keyword"), now()],
    )
    .unwrap();
    conn.last_insert_rowid()
}

fn name_link(shared: &SharedTestDb, stock_id: i64, title: &str, published_at: EpochMs) -> i64 {
    add_link(
        shared,
        stock_id,
        title,
        published_at,
        "name",
        "삼성전자",
        None,
    )
}

async fn evaluate(shared: &SharedTestDb, decider: Option<&FakeDecider>) -> EvaluationSummary {
    let at = || now();
    evaluate_pending(
        &shared.db,
        &EvaluateOptions {
            decider,
            now: Some(&at),
        },
    )
    .await
    .unwrap()
}

fn set_num(shared: &SharedTestDb, setting: &crate::server::settings::NumSetting, value: f64) {
    write_num(&shared.db.lock().unwrap(), setting, value, now()).unwrap();
}

fn count(shared: &SharedTestDb, table: &str) -> i64 {
    shared
        .db
        .lock()
        .unwrap()
        .query_row(&format!("SELECT count(*) FROM {table}"), [], |r| r.get(0))
        .unwrap()
}

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-9
}

// describe("evaluatePending (SPEC FR-5)")
#[tokio::test(flavor = "current_thread")]
async fn evaluates_name_links_first_then_recent_and_records_cost() {
    // it("평가하지 않은 연결을 종목 이름·직접 연결 먼저, 그다음 최근 기사부터 평가해 저장하고, 쓴 비용을 기록한다")
    let shared = seeded();
    let ids = add_stocks(&shared);
    let oldest = name_link(&shared, ids.samsung, "삼성전자 실적", ago(3));
    let newest = add_link(
        &shared,
        ids.samsung,
        "HBM 증설",
        ago(1),
        "keyword",
        "삼성전자 → HBM 메모리",
        Some("주력 메모리 제품"),
    );
    let middle = add_link(
        &shared,
        ids.nvidia,
        "엔비디아 신제품",
        ago(2),
        "name",
        "엔비디아",
        None,
    );
    let decider = fake_article_decider(|input| {
        let relevance = match input.article.title.as_str() {
            "삼성전자 실적" => 8.0,
            "HBM 증설" => 4.0,
            _ => 6.0,
        };
        Ok(crate::server::ai::decider::types::Decision {
            relevance,
            ..fake_decision()
        })
    });

    let summary = evaluate(&shared, Some(&decider)).await;

    assert_eq!(
        (
            summary.evaluated,
            summary.relevant,
            summary.failed,
            summary.remaining
        ),
        (3, 2, 0, 0)
    );
    assert_eq!(summary.note, None);
    // 입력 1000토큰 × 100만 토큰당 $0.042 × 3회
    assert!(close(summary.cost_usd, 0.000126), "{}", summary.cost_usd);

    // D-05: 이름 연결(최근 것부터) 다음에 키워드 연결
    let calls = decider.calls.lock().unwrap();
    let titles: Vec<&str> = calls.iter().map(|c| c.0.article.title.as_str()).collect();
    assert_eq!(titles, ["엔비디아 신제품", "삼성전자 실적", "HBM 증설"]);
    assert!(calls.iter().all(|c| c.1 == "jev-1.13.0"));
    let nvidia_name: String = shared
        .db
        .lock()
        .unwrap()
        .query_row("SELECT name FROM stocks WHERE id = ?1", [ids.nvidia], |r| {
            r.get(0)
        })
        .unwrap();
    let first = &calls[0].0;
    assert_eq!(
        first.stock.kind,
        crate::server::ai::decider::types::JudgeTarget::Stock
    );
    assert_eq!(first.stock.name, "엔비디아");
    assert_eq!(
        first.stock.official_name.as_deref(),
        Some(nvidia_name.as_str())
    );
    assert_eq!(
        (first.stock.market, first.stock.code.as_str()),
        (Market::Us, "NVDA")
    );
    assert_eq!(first.stock.summary, None);
    assert_eq!(
        (
            first.link.kind,
            first.link.path.as_str(),
            first.link.reason.clone()
        ),
        (MatchKind::Name, "엔비디아", None)
    );
    assert_eq!(
        (
            first.article.title.as_str(),
            first.article.summary.as_deref()
        ),
        ("엔비디아 신제품", Some("엔비디아 신제품 요약"))
    );
    assert_eq!(calls[2].0.stock.official_name, None);
    assert_eq!(
        (
            calls[2].0.link.kind,
            calls[2].0.link.path.as_str(),
            calls[2].0.link.reason.as_deref()
        ),
        (
            MatchKind::Keyword,
            "삼성전자 → HBM 메모리",
            Some("주력 메모리 제품")
        )
    );
    drop(calls);

    let conn = shared.db.lock().unwrap();
    let mut stmt = conn
        .prepare("SELECT match_id, relevance, impact, model, stock_id, relevance_confidence, impact_confidence, impact_probabilities, evaluated_at FROM evaluations ORDER BY match_id")
        .unwrap();
    #[allow(clippy::type_complexity)]
    let saved: Vec<(i64, f64, String, String, i64, f64, f64, String, i64)> = stmt
        .query_map([], |r| {
            Ok((
                r.get(0)?,
                r.get(1)?,
                r.get(2)?,
                r.get(3)?,
                r.get(4)?,
                r.get(5)?,
                r.get(6)?,
                r.get(7)?,
                r.get(8)?,
            ))
        })
        .unwrap()
        .map(Result::unwrap)
        .collect();
    let summary_rows: Vec<(i64, f64, &str, &str)> = saved
        .iter()
        .map(|e| (e.0, e.1, e.2.as_str(), e.3.as_str()))
        .collect();
    assert_eq!(
        summary_rows,
        [
            (oldest, 8.0, "positive", "jev-1.13.0"),
            (newest, 4.0, "positive", "jev-1.13.0"),
            (middle, 6.0, "positive", "jev-1.13.0")
        ]
    );
    assert_eq!(
        (saved[0].4, saved[0].5, saved[0].6, saved[0].8),
        (ids.samsung, 0.7, 0.8, now())
    );
    assert_eq!(
        saved[0].7,
        r#"{"positive":0.8,"negative":0.1,"neutral":0.1}"#
    );

    let usage: Vec<(String, String, String, i64, i64, f64, i64)> = conn
        .prepare("SELECT provider, model, purpose, input_tokens, output_tokens, cost_usd, at FROM usage_logs ORDER BY id")
        .unwrap()
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?, r.get(6)?)))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    assert_eq!(usage.len(), 3);
    assert_eq!(
        (
            usage[0].0.as_str(),
            usage[0].1.as_str(),
            usage[0].2.as_str(),
            usage[0].3,
            usage[0].4,
            usage[0].6
        ),
        (
            "typesafe",
            "jev-1.13.0",
            "article_evaluation",
            1000,
            50,
            now()
        )
    );
    assert!(close(usage[0].5, 0.000042));
}

#[tokio::test(flavor = "current_thread")]
async fn stops_at_per_run_limit_and_skips_articles_older_than_seven_days() {
    // it("회차 한도(D-03)까지만 평가하고 나머지는 다음 회차로, 7일 지난 기사는 평가하지 않는다(D-05)")
    let shared = seeded();
    let ids = add_stocks(&shared);
    for i in 0..5 {
        name_link(&shared, ids.samsung, DISTINCT[i as usize], ago(i + 1));
    }
    name_link(&shared, ids.samsung, "삼성전자 8일 전 소식", ago(8 * 24));
    set_num(&shared, &EVALUATION_MAX_PER_RUN, 3.0);
    let decider = fake_article_decider(|_| Ok(fake_decision()));

    let first = evaluate(&shared, Some(&decider)).await;
    assert_eq!((first.evaluated, first.remaining), (3, 2));
    assert_eq!(
        first.note.as_deref(),
        Some("회차 한도 3건까지 평가했습니다. 남은 2건은 다음 회차에 평가합니다.")
    );
    let second = evaluate(&shared, Some(&decider)).await;
    assert_eq!((second.evaluated, second.remaining), (2, 0));
    let calls = decider.calls.lock().unwrap();
    assert!(
        !calls
            .iter()
            .any(|c| c.0.article.title == "삼성전자 8일 전 소식")
    );
    drop(calls);
    assert_eq!(
        count_pending_evaluations(&shared.db.lock().unwrap(), now()).unwrap(),
        0
    );
}

#[tokio::test(flavor = "current_thread")]
async fn already_evaluated_and_deleted_stock_links_are_not_evaluated_again() {
    // it("이미 평가한 연결과 삭제한 종목의 연결은 다시 평가하지 않는다")
    let shared = seeded();
    let ids = add_stocks(&shared);
    name_link(&shared, ids.samsung, "삼성전자 실적", ago(1));
    let decider = fake_article_decider(|_| Ok(fake_decision()));
    evaluate(&shared, Some(&decider)).await;

    add_link(
        &shared,
        ids.nvidia,
        "엔비디아 신제품",
        ago(1),
        "name",
        "엔비디아",
        None,
    );
    delete_stock(&shared.db.lock().unwrap(), ids.nvidia, now()).unwrap();
    let summary = evaluate(&shared, Some(&decider)).await;
    assert_eq!((summary.evaluated, summary.remaining), (0, 0));
    assert_eq!(decider.calls.lock().unwrap().len(), 1);
}

#[tokio::test(flavor = "current_thread")]
async fn uses_configured_model_version_and_display_rule() {
    // it("설정한 모델 버전과 표시 기준을 쓴다")
    let shared = seeded();
    let ids = add_stocks(&shared);
    name_link(&shared, ids.samsung, "삼성전자 실적", ago(1));
    write_str(&shared.db.lock().unwrap(), &JEV_MODEL, "jev-1.14.0", now()).unwrap();
    set_num(&shared, &EVALUATION_MIN_RELEVANCE, 8.0);
    let decider = fake_article_decider(|_| {
        Ok(crate::server::ai::decider::types::Decision {
            relevance: 7.9,
            model: "jev-1.14.0".into(),
            ..fake_decision()
        })
    });

    let summary = evaluate(&shared, Some(&decider)).await;
    assert_eq!((summary.evaluated, summary.relevant), (1, 0));
    assert_eq!(decider.calls.lock().unwrap()[0].1, "jev-1.14.0");
    let model: String = shared
        .db
        .lock()
        .unwrap()
        .query_row("SELECT model FROM evaluations", [], |r| r.get(0))
        .unwrap();
    assert_eq!(model, "jev-1.14.0");
}

#[tokio::test(flavor = "current_thread")]
async fn without_decider_nothing_is_evaluated_and_remaining_and_reason_are_returned() {
    // it("키가 없으면(decider=null) 평가하지 않고 남은 수와 이유를 돌려준다")
    let shared = seeded();
    let ids = add_stocks(&shared);
    name_link(&shared, ids.samsung, "삼성전자 실적", ago(1));
    name_link(&shared, ids.samsung, "삼성전자 투자", ago(2));

    let summary = evaluate(&shared, None).await;
    assert_eq!((summary.evaluated, summary.remaining), (0, 2));
    assert_eq!(
        summary.note.as_deref(),
        Some("TYPESAFE_API_KEY가 없어 평가하지 않았습니다.")
    );
    assert_eq!(count(&shared, "evaluations"), 0);
}

#[tokio::test(flavor = "current_thread")]
async fn exhausted_budget_means_no_call() {
    // it("이번 달 예산을 이미 다 썼으면 부르지 않는다")
    let shared = seeded();
    let ids = add_stocks(&shared);
    name_link(&shared, ids.samsung, "삼성전자 실적", ago(1));
    set_num(&shared, &BUDGET_MONTHLY_USD, 0.01);
    record_usage(
        &shared.db.lock().unwrap(),
        &UsageEntry {
            provider: UsageProvider::Openrouter,
            model: "m",
            purpose: "keyword_generation",
            usage: LlmUsage {
                input_tokens: 1,
                output_tokens: 1,
                cost_usd: 0.01,
            },
            at: now(),
            stock_id: None,
        },
    )
    .unwrap();
    let decider = fake_article_decider(|_| Ok(fake_decision()));

    let summary = evaluate(&shared, Some(&decider)).await;
    assert_eq!((summary.evaluated, summary.remaining), (0, 1));
    assert_eq!(
        summary.note.as_deref(),
        Some("이번 달 AI 예산을 모두 써서 평가를 멈췄습니다.")
    );
    assert_eq!(decider.calls.lock().unwrap().len(), 0);
}

#[tokio::test(flavor = "current_thread")]
async fn crossing_the_budget_mid_run_stops_and_leaves_the_rest() {
    // it("평가 도중 예산을 넘기면 멈추고 나머지는 다음으로 남긴다")
    let shared = seeded();
    let ids = add_stocks(&shared);
    for i in 0..8 {
        name_link(&shared, ids.samsung, DISTINCT[i as usize], ago(i + 1));
    }
    // 한 번에 $0.000042. 두 번째 호출 전에는 한도 아래, 세 번째 호출 전에는 한도 이상.
    set_num(&shared, &BUDGET_MONTHLY_USD, 0.00005);
    let decider = fake_article_decider(|_| Ok(fake_decision()));

    let summary = evaluate(&shared, Some(&decider)).await;
    assert_eq!((summary.evaluated, summary.remaining), (2, 6));
    assert_eq!(
        summary.note.as_deref(),
        Some("이번 달 AI 예산을 모두 써서 평가를 멈췄습니다.")
    );
    assert_eq!(decider.calls.lock().unwrap().len(), 2);
}

#[tokio::test(flavor = "current_thread")]
async fn other_errors_count_as_failures_continue_and_are_retried_next_run() {
    // it("그 밖의 오류는 실패로 세고 계속하며, 다음 회차에 다시 평가한다")
    let shared = seeded();
    let ids = add_stocks(&shared);
    name_link(&shared, ids.samsung, DISTINCT[0], ago(1));
    name_link(&shared, ids.samsung, DISTINCT[1], ago(2));
    let down = std::sync::Arc::new(AtomicBool::new(true));
    let flag = down.clone();
    let decider = fake_article_decider(move |input| {
        if flag.load(Ordering::SeqCst) && input.article.title == DISTINCT[1] {
            Err(DeciderError::new(
                "TypeSafe 오류(HTTP 503)",
                DeciderErrorKind::Upstream,
            ))
        } else {
            Ok(fake_decision())
        }
    });

    let first = evaluate(&shared, Some(&decider)).await;
    assert_eq!((first.evaluated, first.failed, first.remaining), (1, 1, 0));
    down.store(false, Ordering::SeqCst);
    let second = evaluate(&shared, Some(&decider)).await;
    assert_eq!((second.evaluated, second.failed), (1, 0));
    assert_eq!(count(&shared, "evaluations"), 2);
}

#[tokio::test(flavor = "current_thread")]
async fn bad_key_stops_the_run_and_leaves_everything() {
    // it("키가 맞지 않으면 이번 회차 평가를 멈추고 모두 남긴다")
    let shared = seeded();
    let ids = add_stocks(&shared);
    for i in 0..6 {
        name_link(&shared, ids.samsung, DISTINCT[i as usize], ago(i + 1));
    }
    let decider = fake_article_decider(|_| {
        Err(DeciderError::new(
            "TypeSafe 키가 맞지 않거나 권한이 없습니다.",
            DeciderErrorKind::Unauthorized,
        ))
    });

    let summary = evaluate(&shared, Some(&decider)).await;
    assert_eq!(
        (summary.evaluated, summary.failed, summary.remaining),
        (0, 0, 6)
    );
    assert_eq!(
        summary.note.as_deref(),
        Some("TypeSafe 키가 맞지 않거나 권한이 없습니다.")
    );
    assert_eq!(decider.calls.lock().unwrap().len(), 1);
}

#[tokio::test(flavor = "current_thread")]
async fn consecutive_failures_stop_the_run_and_leave_the_rest() {
    // it(`${MAX_CONSECUTIVE_FAILURES}번 연달아 실패하면(장애) 멈추고 나머지는 남긴다`)
    let shared = seeded();
    let ids = add_stocks(&shared);
    for i in 0..6 {
        name_link(&shared, ids.samsung, DISTINCT[i as usize], ago(i + 1));
    }
    let decider = fake_article_decider(|_| {
        Err(DeciderError::new(
            "TypeSafe에 연결하지 못했습니다.",
            DeciderErrorKind::Upstream,
        ))
    });

    let summary = evaluate(&shared, Some(&decider)).await;
    assert_eq!(
        (summary.evaluated, summary.failed, summary.remaining),
        (0, MAX_CONSECUTIVE_FAILURES, 6 - MAX_CONSECUTIVE_FAILURES)
    );
    assert!(summary.note.unwrap().contains("연달아 실패"));
    assert_eq!(
        decider.calls.lock().unwrap().len(),
        MAX_CONSECUTIVE_FAILURES as usize
    );
}

// describe("evaluationTotals")는 `evaluate/read.ts`(R10)의 시험이라 그때 옮긴다.

// describe("evaluatePending: 11단계")
#[tokio::test(flavor = "current_thread")]
async fn saves_magnitude_and_display_rule_needs_both_scores() {
    // it("영향도도 저장하고, 표시 기준은 관련도·영향도가 둘 다 기준 이상일 때다")
    let shared = seeded();
    let ids = add_stocks(&shared);
    // 경계 시나리오는 옛 기본값(영향도 6)으로 지킨다.
    set_num(&shared, &EVALUATION_MIN_MAGNITUDE, 6.0);
    name_link(&shared, ids.samsung, DISTINCT[0], ago(1));
    name_link(&shared, ids.samsung, DISTINCT[1], ago(2));
    name_link(&shared, ids.samsung, DISTINCT[2], ago(3));
    let decider = fake_article_decider(|input| {
        let (relevance, magnitude) = match input.article.title.as_str() {
            t if t == DISTINCT[0] => (8.0, 5.9), // 영향도 미달
            t if t == DISTINCT[1] => (6.0, 6.0), // 경계값은 보인다
            _ => (5.9, 9.0),                     // 관련도 미달
        };
        Ok(crate::server::ai::decider::types::Decision {
            relevance,
            magnitude,
            magnitude_confidence: 0.6,
            ..fake_decision()
        })
    });

    let summary = evaluate(&shared, Some(&decider)).await;
    assert_eq!(
        (summary.evaluated, summary.relevant, summary.copied),
        (3, 1, 0)
    );
    let conn = shared.db.lock().unwrap();
    let saved: Vec<(f64, f64)> = conn
        .prepare("SELECT magnitude, magnitude_confidence FROM evaluations ORDER BY id")
        .unwrap()
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    assert_eq!(saved, [(5.9, 0.6), (6.0, 0.6), (9.0, 0.6)]);
    // 사용량은 그 항목의 것으로 남긴다(항목별 비용).
    let others: i64 = conn
        .query_row(
            "SELECT count(*) FROM usage_logs WHERE stock_id IS NOT ?1",
            [ids.samsung],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(others, 0);
}

fn bystanders(shared: &SharedTestDb) -> Vec<Option<f64>> {
    let conn = shared.db.lock().unwrap();
    conn.prepare("SELECT bystander FROM evaluations ORDER BY id")
        .unwrap()
        .query_map([], |r| r.get(0))
        .unwrap()
        .map(Result::unwrap)
        .collect()
}

#[tokio::test(flavor = "current_thread")]
async fn stock_evaluation_stores_bystander_and_same_news_copy_carries_it() {
    let shared = seeded();
    let ids = add_stocks(&shared);
    let title = "한은 기준금리 동결…연 2.5% 유지";
    name_link(&shared, ids.samsung, title, ago(5));
    name_link(
        &shared,
        ids.samsung,
        "[속보] 한은, 기준금리 동결…연 2.5% 유지",
        ago(4),
    );
    let decider = fake_article_decider(|_| {
        Ok(crate::server::ai::decider::types::Decision {
            bystander: Some(0.83),
            ..fake_decision()
        })
    });
    let summary = evaluate(&shared, Some(&decider)).await;
    assert_eq!((summary.evaluated, summary.copied), (1, 1));
    assert_eq!(bystanders(&shared), [Some(0.83), Some(0.83)]);
}

#[tokio::test(flavor = "current_thread")]
async fn same_news_copy_of_an_evaluation_without_bystander_stays_null() {
    let shared = seeded();
    let ids = add_stocks(&shared);
    let title = "한은 기준금리 동결…연 2.5% 유지";
    name_link(&shared, ids.samsung, title, ago(5));
    name_link(
        &shared,
        ids.samsung,
        "[속보] 한은, 기준금리 동결…연 2.5% 유지",
        ago(4),
    );
    let decider = fake_article_decider(|_| {
        Ok(crate::server::ai::decider::types::Decision {
            bystander: None,
            ..fake_decision()
        })
    });
    let summary = evaluate(&shared, Some(&decider)).await;
    assert_eq!((summary.evaluated, summary.copied), (1, 1));
    assert_eq!(bystanders(&shared), [None, None]);
}

#[tokio::test(flavor = "current_thread")]
async fn same_news_within_24_hours_for_same_item_reuses_the_earlier_evaluation() {
    // it("같은 항목에 24시간 안에 걸린 같은 소식은 앞선 평가를 그대로 쓰고 Jev를 부르지 않는다")
    let shared = seeded();
    let ids = add_stocks(&shared);
    let title = "한은 기준금리 동결…연 2.5% 유지";
    let copy = name_link(&shared, ids.samsung, title, ago(5));
    name_link(
        &shared,
        ids.samsung,
        "[속보] 한은, 기준금리 동결…연 2.5% 유지",
        ago(4),
    );
    // 다른 항목이면 따로 평가한다.
    add_link(&shared, ids.nvidia, title, ago(4), "name", "엔비디아", None);
    // 24시간 넘게 떨어지면 따로 평가한다.
    name_link(&shared, ids.samsung, title, ago(40));
    // 같은 소식 복사는 회차 한도에 세지 않는다.
    set_num(&shared, &EVALUATION_MAX_PER_RUN, 3.0);
    let decider = fake_article_decider(|_| {
        Ok(crate::server::ai::decider::types::Decision {
            relevance: 8.0,
            ..fake_decision()
        })
    });

    let summary = evaluate(&shared, Some(&decider)).await;

    assert_eq!(
        (
            summary.evaluated,
            summary.copied,
            summary.relevant,
            summary.remaining
        ),
        (3, 1, 4, 0)
    );
    assert_eq!(decider.calls.lock().unwrap().len(), 3);
    let conn = shared.db.lock().unwrap();
    type Copy = (f64, Option<f64>, String, String, Option<i64>, i64);
    let load = |sql: &str, key: i64| -> Copy {
        conn.query_row(sql, [key], |r| {
            Ok((
                r.get(0)?,
                r.get(1)?,
                r.get(2)?,
                r.get(3)?,
                r.get(4)?,
                r.get(5)?,
            ))
        })
        .unwrap()
    };
    let copied = load(
        "SELECT relevance, magnitude, impact, model, copied_from, id FROM evaluations WHERE match_id = ?1",
        copy,
    );
    let original = load(
        "SELECT relevance, magnitude, impact, model, copied_from, id FROM evaluations WHERE id = ?1",
        copied.4.expect("복사한 평가는 원본을 가리킨다"),
    );
    assert_eq!(
        (copied.0, copied.1, &copied.2, &copied.3),
        (original.0, original.1, &original.2, &original.3)
    );
    assert_eq!(original.4, None);
    // 복사는 비용을 쓰지 않는다.
    let usage: i64 = conn
        .query_row("SELECT count(*) FROM usage_logs", [], |r| r.get(0))
        .unwrap();
    assert_eq!(usage, 3);
    // 복사한 평가의 확률 JSON도 같은 글자다.
    let probabilities: String = conn
        .query_row(
            "SELECT impact_probabilities FROM evaluations WHERE id = ?1",
            [copied.5],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(
        probabilities,
        r#"{"positive":0.8,"negative":0.1,"neutral":0.1}"#
    );
}

#[tokio::test(flavor = "current_thread")]
async fn market_and_topic_items_put_their_kind_and_topic_summary_into_the_jev_input() {
    // it("시장·주제 항목은 그 종류와 주제 설명을 Jev 입력에 넣는다")
    let shared = seeded();
    let (market, lens) = {
        let conn = shared.db.lock().unwrap();
        let market = seed_market(&conn, "KR", now());
        let topic = seed_topic(&conn, "auto", "중동 긴장", &["사우디"], now());
        conn.execute(
            "UPDATE topics SET summary = '사우디 시설 공격 위협' WHERE id = ?1",
            [topic],
        )
        .unwrap();
        (
            market,
            seed_lens(&conn, topic, "중동 긴장", "auto", "KR", now()),
        )
    };
    add_link(
        &shared,
        market,
        DISTINCT[0],
        ago(1),
        "keyword",
        "한국 증시 → 수출",
        None,
    );
    add_link(
        &shared,
        lens,
        DISTINCT[1],
        ago(2),
        "keyword",
        "중동 긴장 → 사우디",
        None,
    );
    let decider = fake_article_decider(|_| Ok(fake_decision()));

    evaluate(&shared, Some(&decider)).await;

    // 시장·주제 항목은 기사당 한 요청(17단계)이라 합친 호출로 남는다.
    assert!(decider.calls.lock().unwrap().is_empty());
    let calls = decider.merged_calls.lock().unwrap();
    assert_eq!(calls.len(), 2);
    let (first, second) = (&calls[0].0, &calls[1].0);
    assert_eq!(
        (first.topics.len(), &first.market_items, &first.markets),
        (0, &vec![Market::Kr], &vec![Market::Kr])
    );
    assert_eq!(
        (
            second.topics[0].name.as_str(),
            second.topics[0].summary.as_deref(),
            second.market_items.len(),
            &second.markets
        ),
        (
            "중동 긴장",
            Some("사우디 시설 공격 위협"),
            0,
            &vec![Market::Kr]
        )
    );
}

#[test]
fn display_rule_without_magnitude_uses_relevance_only() {
    // Rust에서만 생기는 경우: 11단계 전에 평가한 기사(영향도 없음)는 관련도만 본다.
    let shared = seeded();
    let conn = shared.db.lock().unwrap();
    set_num_conn(&conn, &EVALUATION_MIN_MAGNITUDE, 9.0);
    assert!(meets_display_rule(&conn, 6.0, None).unwrap());
    assert!(!meets_display_rule(&conn, 5.9, None).unwrap());
    assert!(!meets_display_rule(&conn, 6.0, Some(8.9)).unwrap());
    assert!(meets_display_rule(&conn, 6.0, Some(9.0)).unwrap());
}

fn set_num_conn(conn: &Connection, setting: &crate::server::settings::NumSetting, value: f64) {
    write_num(conn, setting, value, now()).unwrap();
}

#[tokio::test(flavor = "current_thread")]
async fn summary_json_keeps_ts_key_order_and_integer_numbers() {
    // Rust에서만 생기는 경우: 회차 기록에 쓰는 평가 요약 JSON(키 순서·정수 값 `0`)
    let summary = EvaluationSummary {
        evaluated: 3,
        relevant: 2,
        copied: 0,
        failed: 0,
        remaining: 1,
        cost_usd: 0.0,
        ms: 12,
        note: None,
    };
    assert_eq!(
        summary.to_json().to_string(),
        r#"{"evaluated":3,"relevant":2,"copied":0,"failed":0,"remaining":1,"costUsd":0,"ms":12}"#
    );
    let with_note = EvaluationSummary {
        cost_usd: 0.000126,
        note: Some("멈춤".into()),
        ..summary
    };
    assert_eq!(
        with_note.to_json().to_string(),
        r#"{"evaluated":3,"relevant":2,"copied":0,"failed":0,"remaining":1,"costUsd":0.000126,"ms":12,"note":"멈춤"}"#
    );
}

#[tokio::test(flavor = "current_thread")]
async fn gated_links_are_not_pending_and_not_counted() {
    let shared = seeded();
    let ids = add_stocks(&shared);
    let gated = add_link(
        &shared,
        ids.samsung,
        "메모리 가격 동향",
        ago(1),
        "keyword",
        "삼성전자 → HBM 메모리",
        Some("r"),
    );
    let open = name_link(&shared, ids.samsung, "삼성전자 실적", ago(2));
    shared
        .db
        .lock()
        .unwrap()
        .execute(
            "UPDATE article_matches SET gated = 1 WHERE id = ?1",
            [gated],
        )
        .unwrap();

    let conn = shared.db.lock().unwrap();
    let pending = pending_evaluations(&conn, now()).unwrap();
    assert_eq!(
        pending.iter().map(|p| p.match_id).collect::<Vec<_>>(),
        [open]
    );
    assert_eq!(count_pending_evaluations(&conn, now()).unwrap(), 1);
}

// 17단계 eval-merge: 시장·주제 항목은 기사당 Jev 한 요청

struct Items {
    market_kr: i64,
    topic_kr: i64,
    topic_us: i64,
}

fn add_items(shared: &SharedTestDb) -> Items {
    let conn = shared.db.lock().unwrap();
    let market_kr = seed_market(&conn, "KR", now());
    let topic = seed_topic(&conn, "auto", "금리", &["금리"], now());
    Items {
        market_kr,
        topic_kr: seed_lens(&conn, topic, "금리", "auto", "KR", now()),
        topic_us: seed_lens(&conn, topic, "금리", "auto", "US", now()),
    }
}

/// 이미 있는 기사에 다른 항목이 걸린 연결을 더한다.
fn link_same_article(shared: &SharedTestDb, match_id: i64, stock_id: i64) -> i64 {
    let conn = shared.db.lock().unwrap();
    let article_id: i64 = conn
        .query_row(
            "SELECT article_id FROM article_matches WHERE id = ?1",
            [match_id],
            |r| r.get(0),
        )
        .unwrap();
    conn.execute(
        "INSERT INTO article_matches (article_id, stock_id, keyword_path, keyword_reason, match_kind, level, created_at) \
         VALUES (?1, ?2, '경로', NULL, 'keyword', 1, ?3)",
        rusqlite::params![article_id, stock_id, now()],
    )
    .unwrap();
    conn.last_insert_rowid()
}

/// 토픽(한국·미국 증시 항목)과 한국 시장 항목이 한 기사에 걸린 연결 셋
fn merged_trio(shared: &SharedTestDb, items: &Items, title: &str, at: EpochMs) -> [i64; 3] {
    let topic_kr = add_link(
        shared,
        items.topic_kr,
        title,
        at,
        "keyword",
        "금리 → 인하",
        None,
    );
    [
        topic_kr,
        link_same_article(shared, topic_kr, items.topic_us),
        link_same_article(shared, topic_kr, items.market_kr),
    ]
}

fn scored(score: f64, confidence: f64) -> crate::server::ai::decider::types::Scored {
    crate::server::ai::decider::types::Scored { score, confidence }
}

fn pin_thresholds(shared: &SharedTestDb) {
    // 경계값을 기본값에 기대지 않는다.
    set_num(shared, &EVALUATION_MIN_RELEVANCE, 6.0);
    set_num(shared, &EVALUATION_MIN_MAGNITUDE, 6.0);
}

type StoredRow = (f64, f64, String, f64, f64, Option<i64>);

fn stored(shared: &SharedTestDb, match_id: i64) -> StoredRow {
    shared
        .db
        .lock()
        .unwrap()
        .query_row(
            "SELECT relevance, magnitude, impact, relevance_confidence, magnitude_confidence, copied_from \
             FROM evaluations WHERE match_id = ?1",
            [match_id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?)),
        )
        .unwrap()
}

#[tokio::test(flavor = "current_thread")]
async fn topic_and_market_links_of_one_article_are_asked_in_one_call_and_stored_per_link() {
    let shared = seeded();
    let items = add_items(&shared);
    pin_thresholds(&shared);
    let [t_kr, t_us, m_kr] = merged_trio(&shared, &items, DISTINCT[0], ago(1));
    let decider = fake_article_decider(|_| Ok(fake_decision())).with_merged(|input| {
        let mut decision = fake_merged_decision(input);
        decision.topics = vec![scored(8.0, 0.9)];
        decision.markets[0].relevance = Some(scored(5.0, 0.4));
        decision.markets[0].magnitude = scored(6.0, 0.5);
        decision.markets[0].impact = crate::server::db::schema::Impact::Negative;
        decision.markets[1].magnitude = scored(4.0, 0.6);
        Ok(decision)
    });

    let summary = evaluate(&shared, Some(&decider)).await;

    assert_eq!(decider.merged_calls.lock().unwrap().len(), 1);
    assert!(decider.calls.lock().unwrap().is_empty());
    let input = decider.merged_calls.lock().unwrap()[0].0.clone();
    assert_eq!(input.topics.len(), 1);
    assert_eq!(input.market_items, [Market::Kr]);
    assert_eq!(input.markets, [Market::Kr, Market::Us]);
    // 토픽 항목(T, 증시 M)은 T 관련도·M 영향도·M 호재/악재, 시장 항목 M은 M 관련도·영향도·호재/악재
    assert_eq!(
        stored(&shared, t_kr),
        (8.0, 6.0, "negative".into(), 0.9, 0.5, None)
    );
    assert_eq!(
        stored(&shared, t_us),
        (8.0, 4.0, "positive".into(), 0.9, 0.6, None)
    );
    assert_eq!(
        stored(&shared, m_kr),
        (5.0, 6.0, "negative".into(), 0.4, 0.5, None)
    );
    // 시장·주제 평가 줄은 bystander가 NULL
    assert_eq!(bystanders(&shared), [None, None, None]);
    // 표시 기준: 토픽 한국(8·6)만 관련도·영향도를 모두 넘는다.
    assert_eq!(
        (
            summary.evaluated,
            summary.relevant,
            summary.failed,
            summary.remaining
        ),
        (3, 1, 0, 0)
    );
}

#[tokio::test(flavor = "current_thread")]
async fn usage_is_split_per_link_and_sums_to_the_request() {
    let shared = seeded();
    let items = add_items(&shared);
    merged_trio(&shared, &items, DISTINCT[0], ago(1));
    let decider = fake_article_decider(|_| Ok(fake_decision()));

    let summary = evaluate(&shared, Some(&decider)).await;

    let conn = shared.db.lock().unwrap();
    let mut stmt = conn
        .prepare(
            "SELECT stock_id, input_tokens, output_tokens, cost_usd, purpose FROM usage_logs ORDER BY id",
        )
        .unwrap();
    let rows: Vec<(i64, i64, i64, f64, String)> = stmt
        .query_map([], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?))
        })
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    assert_eq!(rows.len(), 3);
    // 1000/50토큰을 셋으로 나누고 나머지는 첫 줄에
    assert_eq!(
        rows.iter().map(|r| (r.1, r.2)).collect::<Vec<_>>(),
        [(334, 18), (333, 16), (333, 16)]
    );
    assert_eq!(rows.iter().map(|r| r.1).sum::<i64>(), 1000);
    assert_eq!(rows.iter().map(|r| r.2).sum::<i64>(), 50);
    assert!(rows.iter().all(|r| r.4 == "article_evaluation"));
    let mut stock_ids: Vec<i64> = rows.iter().map(|r| r.0).collect();
    stock_ids.sort_unstable();
    let mut expected = vec![items.market_kr, items.topic_kr, items.topic_us];
    expected.sort_unstable();
    assert_eq!(stock_ids, expected);
    // 비용은 그 줄의 입력 토큰 × 단가
    let price = read_num(
        &conn,
        &crate::server::settings::JEV_USD_PER_MILLION_INPUT_TOKENS,
    )
    .unwrap();
    for row in &rows {
        assert!(close(row.3, row.1 as f64 * price / 1_000_000.0));
    }
    assert!(close(
        summary.cost_usd,
        rows.iter().map(|r| r.3).sum::<f64>()
    ));
}

#[tokio::test(flavor = "current_thread")]
async fn a_merged_call_counts_as_one_toward_the_per_run_limit() {
    let shared = seeded();
    let items = add_items(&shared);
    merged_trio(&shared, &items, DISTINCT[0], ago(1));
    add_link(
        &shared,
        items.market_kr,
        DISTINCT[1],
        ago(2),
        "keyword",
        "경로",
        None,
    );
    set_num(&shared, &EVALUATION_MAX_PER_RUN, 1.0);
    let decider = fake_article_decider(|_| Ok(fake_decision()));

    let summary = evaluate(&shared, Some(&decider)).await;

    assert_eq!(decider.merged_calls.lock().unwrap().len(), 1);
    assert_eq!((summary.evaluated, summary.remaining), (3, 1));
    assert!(summary.note.unwrap().starts_with("회차 한도 1건"));
}

#[tokio::test(flavor = "current_thread")]
async fn same_news_copies_are_looked_at_per_link_and_only_the_rest_is_asked() {
    let shared = seeded();
    let items = add_items(&shared);
    let title = "한은 기준금리 동결…연 2.5% 유지";
    // 앞서 같은 소식이 한국 시장 항목에서 평가돼 있다.
    let earlier = add_link(
        &shared,
        items.market_kr,
        title,
        ago(5),
        "keyword",
        "경로",
        None,
    );
    evaluate(
        &shared,
        Some(&fake_article_decider(|_| Ok(fake_decision()))),
    )
    .await;
    let [t_kr, t_us, m_kr] = merged_trio(
        &shared,
        &items,
        "[속보] 한은, 기준금리 동결…연 2.5% 유지",
        ago(4),
    );
    let decider = fake_article_decider(|_| Ok(fake_decision()));

    let summary = evaluate(&shared, Some(&decider)).await;

    let calls = decider.merged_calls.lock().unwrap();
    assert_eq!(calls.len(), 1);
    // 시장 항목은 복사로 끝나 질문에서 빠진다.
    assert!(calls[0].0.market_items.is_empty());
    assert_eq!((summary.evaluated, summary.copied), (2, 1));
    let original_id: i64 = shared
        .db
        .lock()
        .unwrap()
        .query_row(
            "SELECT id FROM evaluations WHERE match_id = ?1",
            [earlier],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(stored(&shared, m_kr).5, Some(original_id));
    assert_eq!(stored(&shared, t_kr).5, None);
    assert_eq!(stored(&shared, t_us).5, None);
}

#[tokio::test(flavor = "current_thread")]
async fn a_failed_merged_call_leaves_all_its_links_for_the_next_run() {
    let shared = seeded();
    let items = add_items(&shared);
    merged_trio(&shared, &items, DISTINCT[0], ago(1));
    let failing = fake_article_decider(|_| Ok(fake_decision()))
        .with_merged(|_| Err(DeciderError::new("일시 오류", DeciderErrorKind::Upstream)));

    let summary = evaluate(&shared, Some(&failing)).await;

    assert_eq!((summary.evaluated, summary.failed), (0, 1));
    assert_eq!(count(&shared, "evaluations"), 0);
    assert_eq!(count(&shared, "usage_logs"), 0);
    let ok = fake_article_decider(|_| Ok(fake_decision()));
    let again = evaluate(&shared, Some(&ok)).await;
    assert_eq!(again.evaluated, 3);
}

#[tokio::test(flavor = "current_thread")]
async fn stock_links_are_still_asked_one_by_one_beside_the_merged_call() {
    let shared = seeded();
    let ids = add_stocks(&shared);
    let items = add_items(&shared);
    let [t_kr, ..] = merged_trio(&shared, &items, DISTINCT[0], ago(1));
    link_same_article(&shared, t_kr, ids.samsung);
    let decider = fake_article_decider(|_| Ok(fake_decision()));

    let summary = evaluate(&shared, Some(&decider)).await;

    let calls = decider.calls.lock().unwrap();
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].0.stock.name, "삼성전자");
    assert_eq!(decider.merged_calls.lock().unwrap().len(), 1);
    assert_eq!(summary.evaluated, 4);
}
