use std::time::Instant;

use rusqlite::{Connection, OptionalExtension, params};
use serde_json::{Value, json};

use crate::server::ai::decider::types::{
    Decider, DecisionArticle, DecisionInput, DecisionLink, JudgeItem,
};
use crate::server::ai::llm::LlmUsage;
use crate::server::budget::usage::{
    BudgetCheckError, UsageEntry, assert_within_budget, record_usage,
};
use crate::server::db::client::Db;
use crate::server::db::schema::{EvaluationSummary, Market, MatchKind, UsageProvider};
use crate::server::log::log_error;
use crate::server::settings::{
    EVALUATION_MAX_PER_RUN, EVALUATION_MIN_MAGNITUDE, EVALUATION_MIN_RELEVANCE, JEV_MODEL,
    JEV_USD_PER_MILLION_INPUT_TOKENS, read_num, read_str,
};
use crate::server::similar_title::{
    SAME_NEWS_WINDOW_MS, TitleFeatures, is_same_news, title_features,
};
use crate::server::text::json_number;
use crate::server::time::{EpochMs, now_ms};

/// 연달아 이만큼 실패하면(TypeSafe 장애 등) 이번 회차 평가를 멈춘다. 호출마다 재시도·제한 시간을 다 기다리지 않게.
pub const MAX_CONSECUTIVE_FAILURES: i64 = 3;
/// 이보다 오래된 기사는 평가하지 않는다(D-05: 한도·예산에 밀린 옛 기사보다 요즘 기사를 먼저).
pub const EVALUATION_WINDOW_MS: i64 = 7 * 24 * 60 * 60 * 1000;

#[derive(Debug, Clone, PartialEq)]
pub struct Pending {
    pub match_id: i64,
    pub article_id: i64,
    pub stock_id: i64,
    pub published_at: EpochMs,
    pub input: DecisionInput,
}

fn text_enum<T: std::str::FromStr<Err = String>>(
    row: &rusqlite::Row<'_>,
    index: usize,
) -> rusqlite::Result<T> {
    let text: String = row.get(index)?;
    text.parse().map_err(|e: String| {
        rusqlite::Error::FromSqlConversionFailure(index, rusqlite::types::Type::Text, e.into())
    })
}

/// 아직 평가하지 않은 최근 7일 기사–항목 연결(D-05). 화면에 나올 가능성이 큰 종목 이름·Finnhub 직접 연결을
/// 먼저, 그다음 최근 기사부터(실측: 키워드 연결은 90건 중 1건만 6점 이상, docs/DECISIONS.md 2장).
pub fn pending_evaluations(conn: &Connection, now: EpochMs) -> rusqlite::Result<Vec<Pending>> {
    let mut stmt = conn.prepare(
        "SELECT article_matches.id, articles.id, stocks.id, article_matches.match_kind, article_matches.keyword_path, \
                article_matches.keyword_reason, articles.title, articles.summary, articles.published_at, stocks.kind, \
                stocks.name, stocks.name_ko, stocks.market, stocks.exchange, stocks.code, topics.summary \
         FROM article_matches \
         INNER JOIN articles ON articles.id = article_matches.article_id \
         INNER JOIN stocks ON stocks.id = article_matches.stock_id \
         LEFT JOIN topics ON topics.id = stocks.topic_id \
         LEFT JOIN evaluations ON evaluations.match_id = article_matches.id \
         WHERE evaluations.id IS NULL AND stocks.deleted_at IS NULL AND articles.published_at >= ?1 \
         ORDER BY CASE WHEN article_matches.match_kind = 'keyword' THEN 1 ELSE 0 END, \
                  articles.published_at DESC, article_matches.id DESC",
    )?;
    let rows = stmt.query_map(params![now - EVALUATION_WINDOW_MS], |row| {
        let name: String = row.get(10)?;
        let name_ko: Option<String> = row.get(11)?;
        let item_kind: crate::server::db::schema::ItemKind = text_enum(row, 9)?;
        let market: Market = text_enum(row, 12)?;
        let kind: MatchKind = text_enum(row, 3)?;
        Ok(Pending {
            match_id: row.get(0)?,
            article_id: row.get(1)?,
            stock_id: row.get(2)?,
            published_at: row.get(8)?,
            input: DecisionInput {
                stock: JudgeItem {
                    kind: item_kind.into(),
                    // `r.nameKo ?? r.name`, `r.nameKo && r.nameKo !== r.name ? r.name : null`
                    name: name_ko.clone().unwrap_or_else(|| name.clone()),
                    official_name: name_ko
                        .filter(|ko| !ko.is_empty() && *ko != name)
                        .map(|_| name.clone()),
                    market,
                    exchange: row.get(13)?,
                    code: row.get(14)?,
                    summary: row.get(15)?,
                },
                link: DecisionLink {
                    kind,
                    path: row.get(4)?,
                    reason: row.get(5)?,
                },
                article: DecisionArticle {
                    title: row.get(6)?,
                    summary: row.get(7)?,
                },
            },
        })
    })?;
    rows.collect()
}

struct Evaluation {
    id: i64,
    relevance: f64,
    relevance_confidence: f64,
    magnitude: Option<f64>,
    magnitude_confidence: Option<f64>,
    impact: String,
    impact_confidence: f64,
    impact_probabilities: String,
    model: String,
}

/// 같은 항목에 걸린 같은 소식(비슷한 제목, 발행 24시간 안)의 앞선 평가(11단계 결정: 같은 소식은 한 번만 Jev에 묻는다).
/// 다른 평가를 그대로 쓴 평가는 고르지 않는다(처음 평가한 것을 쓴다).
fn find_same_news_evaluation(
    conn: &Connection,
    item: &Pending,
) -> rusqlite::Result<Option<Evaluation>> {
    let at = item.published_at;
    let mut stmt = conn.prepare(
        "SELECT evaluations.id, evaluations.relevance, evaluations.relevance_confidence, evaluations.magnitude, \
                evaluations.magnitude_confidence, evaluations.impact, evaluations.impact_confidence, \
                evaluations.impact_probabilities, evaluations.model, articles.title \
         FROM evaluations INNER JOIN articles ON articles.id = evaluations.article_id \
         WHERE evaluations.stock_id = ?1 AND evaluations.copied_from IS NULL \
           AND articles.published_at >= ?2 AND articles.published_at <= ?3",
    )?;
    let candidates: Vec<(Evaluation, String)> = stmt
        .query_map(
            params![
                item.stock_id,
                at - SAME_NEWS_WINDOW_MS,
                at + SAME_NEWS_WINDOW_MS
            ],
            |row| {
                Ok((
                    Evaluation {
                        id: row.get(0)?,
                        relevance: row.get(1)?,
                        relevance_confidence: row.get(2)?,
                        magnitude: row.get(3)?,
                        magnitude_confidence: row.get(4)?,
                        impact: row.get(5)?,
                        impact_confidence: row.get(6)?,
                        impact_probabilities: row.get(7)?,
                        model: row.get(8)?,
                    },
                    row.get(9)?,
                ))
            },
        )?
        .collect::<rusqlite::Result<_>>()?;
    if candidates.is_empty() {
        return Ok(None);
    }
    let mine: TitleFeatures = title_features(&item.input.article.title);
    Ok(candidates
        .into_iter()
        .find(|(_, title)| is_same_news(&mine, &title_features(title)))
        .map(|(evaluation, _)| evaluation))
}

/// 표시 기준(관련도·영향도 둘 다, 11단계) 이상인지
pub fn meets_display_rule(
    conn: &Connection,
    relevance: f64,
    magnitude: Option<f64>,
) -> rusqlite::Result<bool> {
    if relevance < read_num(conn, &EVALUATION_MIN_RELEVANCE)? {
        return Ok(false);
    }
    // 11단계 전에 평가한 기사(영향도 없음)는 관련도만 본다.
    Ok(match magnitude {
        None => true,
        Some(m) => m >= read_num(conn, &EVALUATION_MIN_MAGNITUDE)?,
    })
}

pub struct EvaluateOptions<'a, D> {
    pub decider: Option<&'a D>,
    pub now: Option<&'a (dyn Fn() -> EpochMs + Send + Sync)>,
}

/// 저장된 JSON 글자를 `JSON.parse` → `JSON.stringify`처럼 다시 쓴다(키 순서 유지).
fn renormalize_json(text: &str) -> String {
    serde_json::from_str::<Value>(text).map_or_else(|_| text.to_string(), |v| v.to_string())
}

/// 평가하지 않은 연결을 Jev로 평가한다(SPEC FR-5, 11단계: 관련도·영향도·호재/악재). 호출마다 월 예산을 먼저 보고
/// (CLAUDE.md), 쓴 비용을 항목과 함께 기록한다(D-06: 입력 토큰 × 단가). 하나씩 차례로 불러 예산 검사에 앞선 호출이
/// 모두 들어가게 한다. 같은 항목의 같은 소식(비슷한 제목)은 앞선 평가를 그대로 쓰고 Jev를 부르지 않는다(회차 한도에도 세지 않음).
/// 예산을 다 쓰거나, 키가 없거나 맞지 않거나, 연달아 실패하면 멈추고 나머지는 남긴다. 실패한 것은 다음 회차에 다시 평가한다.
pub async fn evaluate_pending<D: Decider>(
    db: &Db,
    options: &EvaluateOptions<'_, D>,
) -> rusqlite::Result<EvaluationSummary> {
    let now = || options.now.map_or_else(now_ms, |f| f());
    let started = Instant::now();
    let (pending, model, price, max_per_run) = {
        let conn = db.lock().expect("DB 잠금이 깨졌습니다");
        (
            pending_evaluations(&conn, now())?,
            read_str(&conn, &JEV_MODEL)?,
            read_num(&conn, &JEV_USD_PER_MILLION_INPUT_TOKENS)?,
            // 회차당 한도(D-03). 넘는 것은 다음 회차로 넘긴다(D-05). 같은 소식 복사는 세지 않는다.
            read_num(&conn, &EVALUATION_MAX_PER_RUN)?.max(1.0),
        )
    };
    let mut summary = EvaluationSummary::empty();
    let Some(decider) = options.decider else {
        return Ok(EvaluationSummary {
            remaining: pending.len() as i64,
            note: Some("TYPESAFE_API_KEY가 없어 평가하지 않았습니다.".to_string()),
            ..summary
        });
    };

    let mut note: Option<String> = None;
    let mut failed_in_row = 0;
    let mut calls = 0.0_f64;
    let mut stopped_at = pending.len();
    for (index, item) in pending.iter().enumerate() {
        {
            let conn = db.lock().expect("DB 잠금이 깨졌습니다");
            if let Some(same) = find_same_news_evaluation(&conn, item)? {
                conn.execute(
                    "INSERT INTO evaluations (match_id, article_id, stock_id, relevance, relevance_confidence, magnitude, \
                     magnitude_confidence, impact, impact_confidence, impact_probabilities, model, copied_from, evaluated_at) \
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13) ON CONFLICT DO NOTHING",
                    params![
                        item.match_id,
                        item.article_id,
                        item.stock_id,
                        same.relevance,
                        same.relevance_confidence,
                        same.magnitude,
                        same.magnitude_confidence,
                        same.impact,
                        same.impact_confidence,
                        renormalize_json(&same.impact_probabilities),
                        same.model,
                        same.id,
                        now()
                    ],
                )?;
                summary.copied += 1;
                if meets_display_rule(&conn, same.relevance, same.magnitude)? {
                    summary.relevant += 1;
                }
                continue;
            }
            if calls >= max_per_run {
                stopped_at = index;
                note = Some(format!(
                    "회차 한도 {}건까지 평가했습니다.",
                    crate::server::text::js_number(max_per_run)
                ));
                break;
            }
            match assert_within_budget(&conn, now()) {
                Ok(()) => {}
                Err(BudgetCheckError::Exceeded(_)) => {
                    note = Some("이번 달 AI 예산을 모두 써서 평가를 멈췄습니다.".to_string());
                    stopped_at = index;
                    break;
                }
                Err(BudgetCheckError::Db(e)) => return Err(e),
            }
        }
        calls += 1.0;
        match decider.decide(&item.input, &model).await {
            Ok(decision) => {
                let cost_usd = (decision.usage.input_tokens as f64 * price) / 1_000_000.0;
                let at = now();
                let conn = db.lock().expect("DB 잠금이 깨졌습니다");
                record_usage(
                    &conn,
                    &UsageEntry {
                        provider: UsageProvider::Typesafe,
                        model: &decision.model,
                        purpose: "article_evaluation",
                        usage: LlmUsage {
                            input_tokens: decision.usage.input_tokens,
                            output_tokens: decision.usage.output_tokens,
                            cost_usd,
                        },
                        at,
                        stock_id: Some(item.stock_id),
                    },
                )?;
                let [positive, negative, neutral] = decision.impact_probabilities;
                let probabilities = json!({
                    "positive": json_number(positive),
                    "negative": json_number(negative),
                    "neutral": json_number(neutral),
                });
                conn.execute(
                    "INSERT INTO evaluations (match_id, article_id, stock_id, relevance, relevance_confidence, magnitude, \
                     magnitude_confidence, impact, impact_confidence, impact_probabilities, model, evaluated_at) \
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12) ON CONFLICT DO NOTHING",
                    params![
                        item.match_id,
                        item.article_id,
                        item.stock_id,
                        decision.relevance,
                        decision.relevance_confidence,
                        decision.magnitude,
                        decision.magnitude_confidence,
                        decision.impact.as_str(),
                        decision.impact_confidence,
                        probabilities.to_string(),
                        decision.model,
                        at
                    ],
                )?;
                summary.evaluated += 1;
                summary.cost_usd += cost_usd;
                if meets_display_rule(&conn, decision.relevance, Some(decision.magnitude))? {
                    summary.relevant += 1;
                }
                failed_in_row = 0;
            }
            Err(error) if error.fatal() => {
                note = Some(error.message);
                stopped_at = index;
                break;
            }
            Err(error) => {
                summary.failed += 1;
                failed_in_row += 1;
                log_error(
                    &format!("기사 평가 실패: 연결 {}", item.match_id),
                    &error.message,
                );
                if failed_in_row >= MAX_CONSECUTIVE_FAILURES {
                    note = Some(format!(
                        "Jev 호출이 {MAX_CONSECUTIVE_FAILURES}번 연달아 실패해 평가를 멈췄습니다. 다음 회차에 다시 합니다."
                    ));
                    stopped_at = index + 1;
                    break;
                }
            }
        }
    }

    summary.remaining = (pending.len() - stopped_at) as i64;
    summary.ms = i64::try_from(started.elapsed().as_millis()).unwrap_or(i64::MAX);
    if let Some(text) = &note
        && text.starts_with("회차 한도")
    {
        note = Some(format!(
            "{text} 남은 {}건은 다음 회차에 평가합니다.",
            summary.remaining
        ));
    }
    summary.note = note;
    Ok(summary)
}

/// 평가하지 않은 최근 7일 기사–종목 연결 수(삭제한 종목 제외). `evaluate/read.ts`의 `countPendingEvaluations`.
pub fn count_pending_evaluations(conn: &Connection, now: EpochMs) -> rusqlite::Result<i64> {
    conn.query_row(
        "SELECT count(*) FROM article_matches \
         INNER JOIN articles ON articles.id = article_matches.article_id \
         INNER JOIN stocks ON stocks.id = article_matches.stock_id \
         LEFT JOIN evaluations ON evaluations.match_id = article_matches.id \
         WHERE evaluations.id IS NULL AND stocks.deleted_at IS NULL AND articles.published_at >= ?1",
        params![now - EVALUATION_WINDOW_MS],
        |row| row.get(0),
    )
    .optional()
    .map(|n| n.unwrap_or(0))
}

#[cfg(test)]
#[path = "run_tests.rs"]
mod tests;
