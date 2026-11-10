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
    FakeDecider, SharedTestDb, fake_article_decider, fake_decision, ms, seed_lens, seed_listings,
    seed_market, seed_topic, shared_test_db,
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

    let calls = decider.calls.lock().unwrap();
    let (first, second) = (&calls[0].0.stock, &calls[1].0.stock);
    assert_eq!(
        (first.kind, first.name.as_str(), first.market),
        (
            crate::server::ai::decider::types::JudgeTarget::Market,
            "한국 증시",
            Market::Kr
        )
    );
    assert_eq!(
        (second.kind, second.name.as_str(), second.summary.as_deref()),
        (
            crate::server::ai::decider::types::JudgeTarget::Topic,
            "중동 긴장",
            Some("사우디 시설 공격 위협")
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
