use std::time::Instant;

use rusqlite::{Connection, OptionalExtension, params};
use serde_json::{Value, json};

use crate::server::ai::decider::types::{
    Decider, DeciderError, DeciderErrorKind, Decision, DecisionArticle, DecisionInput,
    DecisionLink, JudgeItem, JudgeTarget, MergedDecision, MergedInput, MergedTopic,
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
    /// 주제 항목이면 그 토픽(같은 토픽의 한국·미국 항목을 한 토픽으로 묶는 데 쓴다)
    pub topic_id: Option<i64>,
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
                stocks.name, stocks.name_ko, stocks.market, stocks.exchange, stocks.code, topics.summary, stocks.topic_id \
         FROM article_matches \
         INNER JOIN articles ON articles.id = article_matches.article_id \
         INNER JOIN stocks ON stocks.id = article_matches.stock_id \
         LEFT JOIN topics ON topics.id = stocks.topic_id \
         LEFT JOIN evaluations ON evaluations.match_id = article_matches.id \
         WHERE evaluations.id IS NULL AND article_matches.gated = 0 AND stocks.deleted_at IS NULL \
           AND articles.published_at >= ?1 \
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
            topic_id: row.get(16)?,
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
    bystander: Option<f64>,
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
                evaluations.impact_probabilities, evaluations.bystander, evaluations.model, articles.title \
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
                        bystander: row.get(8)?,
                        model: row.get(9)?,
                    },
                    row.get(10)?,
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

/// 숨김 규칙 SQL 조각(종목 항목 평가에만 적용). 조회에 `evaluations`·`stocks` 별칭이 있어야 하고
/// `?` 하나에 `BYSTANDER_THRESHOLD`를 넣는다. 숨기는 경우: 구경꾼 확률이 기준 이상이거나,
/// 구경꾼 값이 없는 평가에서 같은 기사의 시장 항목 관련도가 이 종목 관련도보다 클 때.
pub const NOT_HIDDEN_SQL: &str = "NOT (stocks.kind = 'stock' AND (COALESCE(evaluations.bystander, 0) >= ? OR (evaluations.bystander IS NULL AND EXISTS (SELECT 1 FROM evaluations AS m INNER JOIN stocks AS ms ON ms.id = m.stock_id WHERE m.article_id = evaluations.article_id AND ms.kind = 'market' AND ms.deleted_at IS NULL AND m.relevance > evaluations.relevance))))";

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

/// 같은 소식의 앞선 평가를 이 연결의 평가로 복사한다. 표시 기준을 넘으면 true.
fn store_copy(
    conn: &Connection,
    item: &Pending,
    same: &Evaluation,
    at: EpochMs,
) -> rusqlite::Result<bool> {
    conn.execute(
        "INSERT INTO evaluations (match_id, article_id, stock_id, relevance, relevance_confidence, magnitude, \
         magnitude_confidence, impact, impact_confidence, impact_probabilities, bystander, model, copied_from, evaluated_at) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14) ON CONFLICT DO NOTHING",
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
            same.bystander,
            same.model,
            same.id,
            at
        ],
    )?;
    meets_display_rule(conn, same.relevance, same.magnitude)
}

/// 평가 한 줄과 사용량 한 줄을 저장한다. 표시 기준을 넘으면 true.
#[allow(clippy::too_many_arguments)]
fn store_evaluation(
    conn: &Connection,
    item: &Pending,
    relevance: (f64, f64),
    magnitude: (f64, f64),
    impact: (&str, f64, [f64; 3]),
    bystander: Option<f64>,
    model: &str,
    usage: (i64, i64),
    price: f64,
    at: EpochMs,
    summary: &mut EvaluationSummary,
) -> rusqlite::Result<bool> {
    let cost_usd = (usage.0 as f64 * price) / 1_000_000.0;
    record_usage(
        conn,
        &UsageEntry {
            provider: UsageProvider::Typesafe,
            model,
            purpose: "article_evaluation",
            usage: LlmUsage {
                input_tokens: usage.0,
                output_tokens: usage.1,
                cost_usd,
            },
            at,
            stock_id: Some(item.stock_id),
        },
    )?;
    let [positive, negative, neutral] = impact.2;
    let probabilities = json!({
        "positive": json_number(positive),
        "negative": json_number(negative),
        "neutral": json_number(neutral),
    });
    conn.execute(
        "INSERT INTO evaluations (match_id, article_id, stock_id, relevance, relevance_confidence, magnitude, \
         magnitude_confidence, impact, impact_confidence, impact_probabilities, bystander, model, evaluated_at) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13) ON CONFLICT DO NOTHING",
        params![
            item.match_id,
            item.article_id,
            item.stock_id,
            relevance.0,
            relevance.1,
            magnitude.0,
            magnitude.1,
            impact.0,
            impact.1,
            probabilities.to_string(),
            bystander,
            model,
            at
        ],
    )?;
    summary.evaluated += 1;
    summary.cost_usd += cost_usd;
    meets_display_rule(conn, relevance.0, Some(magnitude.0))
}

fn store_stock_decision(
    conn: &Connection,
    item: &Pending,
    decision: &Decision,
    price: f64,
    at: EpochMs,
    summary: &mut EvaluationSummary,
) -> rusqlite::Result<()> {
    let relevant = store_evaluation(
        conn,
        item,
        (decision.relevance, decision.relevance_confidence),
        (decision.magnitude, decision.magnitude_confidence),
        (
            decision.impact.as_str(),
            decision.impact_confidence,
            decision.impact_probabilities,
        ),
        decision.bystander,
        &decision.model,
        (decision.usage.input_tokens, decision.usage.output_tokens),
        price,
        at,
        summary,
    )?;
    summary.relevant += i64::from(relevant);
    Ok(())
}

/// 토픽을 가리키는 키(같은 토픽의 한국·미국 항목은 같은 키)
fn topic_key(item: &Pending) -> i64 {
    item.topic_id.unwrap_or(-item.stock_id)
}

/// 같은 기사의 시장·주제 항목 연결들을 한 요청의 입력으로 만든다(17단계 eval-merge).
fn merged_input(members: &[&Pending]) -> MergedInput {
    let mut topic_keys: Vec<i64> = Vec::new();
    let mut topics: Vec<MergedTopic> = Vec::new();
    let mut market_items: Vec<Market> = Vec::new();
    let mut markets: Vec<Market> = Vec::new();
    for member in members {
        let stock = &member.input.stock;
        if !markets.contains(&stock.market) {
            markets.push(stock.market);
        }
        match stock.kind {
            JudgeTarget::Topic => {
                let key = topic_key(member);
                if !topic_keys.contains(&key) {
                    topic_keys.push(key);
                    topics.push(MergedTopic {
                        name: stock.name.clone(),
                        summary: stock.summary.clone(),
                    });
                }
            }
            JudgeTarget::Market if !market_items.contains(&stock.market) => {
                market_items.push(stock.market);
            }
            _ => {}
        }
    }
    markets.sort_by_key(|m| *m != Market::Kr);
    MergedInput {
        topics,
        market_items,
        markets,
        article: members[0].input.article.clone(),
    }
}

/// 합친 응답을 연결마다 평가 한 줄·사용량 한 줄로 저장한다. 응답에 빠진 항목이 있으면 아무것도 저장하지 않고 false.
/// 토큰은 연결 수로 나누고 나머지는 첫 줄에 더한다.
fn store_merged_decision(
    conn: &Connection,
    members: &[&Pending],
    decision: &MergedDecision,
    price: f64,
    at: EpochMs,
    summary: &mut EvaluationSummary,
) -> rusqlite::Result<bool> {
    let mut topic_keys: Vec<i64> = Vec::new();
    for member in members {
        if member.input.stock.kind == JudgeTarget::Topic && !topic_keys.contains(&topic_key(member))
        {
            topic_keys.push(topic_key(member));
        }
    }
    let mut resolved = Vec::with_capacity(members.len());
    for member in members {
        let stock = &member.input.stock;
        let Some(verdict) = decision.markets.iter().find(|v| v.market == stock.market) else {
            return Ok(false);
        };
        let relevance = if stock.kind == JudgeTarget::Topic {
            topic_keys
                .iter()
                .position(|k| *k == topic_key(member))
                .and_then(|n| decision.topics.get(n))
                .copied()
        } else {
            verdict.relevance
        };
        let Some(relevance) = relevance else {
            return Ok(false);
        };
        resolved.push((relevance, verdict));
    }
    let count = members.len() as i64;
    let (input_each, input_rest) = (
        decision.usage.input_tokens / count,
        decision.usage.input_tokens % count,
    );
    let (output_each, output_rest) = (
        decision.usage.output_tokens / count,
        decision.usage.output_tokens % count,
    );
    for (index, (member, (relevance, verdict))) in members.iter().zip(resolved).enumerate() {
        let first = index == 0;
        let relevant = store_evaluation(
            conn,
            member,
            (relevance.score, relevance.confidence),
            (verdict.magnitude.score, verdict.magnitude.confidence),
            (
                verdict.impact.as_str(),
                verdict.impact_confidence,
                verdict.impact_probabilities,
            ),
            None,
            &decision.model,
            (
                input_each + if first { input_rest } else { 0 },
                output_each + if first { output_rest } else { 0 },
            ),
            price,
            at,
            summary,
        )?;
        summary.relevant += i64::from(relevant);
    }
    Ok(true)
}

/// 평가하지 않은 연결을 Jev로 평가한다(SPEC FR-5, 11단계: 관련도·영향도·호재/악재). 호출마다 월 예산을 먼저 보고
/// (CLAUDE.md), 쓴 비용을 항목과 함께 기록한다(D-06: 입력 토큰 × 단가). 하나씩 차례로 불러 예산 검사에 앞선 호출이
/// 모두 들어가게 한다. 시장·주제 항목은 같은 기사의 연결을 모아 한 요청으로 묻고(17단계) 연결마다 저장한다. 종목은 하나씩 묻는다. 같은 항목의 같은 소식(비슷한 제목)은 앞선 평가를 그대로 쓰고 Jev를 부르지 않는다(회차 한도에도 세지 않음).
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
    // 처리를 마친 연결(평가·복사·실패). 시장·주제 항목은 같은 기사의 연결을 한꺼번에 처리하므로 앞서 끝날 수 있다.
    let mut done = vec![false; pending.len()];
    for (index, item) in pending.iter().enumerate() {
        if done[index] {
            continue;
        }
        let merged = item.input.stock.kind != JudgeTarget::Stock;
        // 시장·주제 항목은 그 기사의 시장·주제 항목 연결을 모두 모아 Jev 한 요청으로 묻는다(17단계).
        let group: Vec<usize> = if merged {
            (index..pending.len())
                .filter(|&j| {
                    !done[j]
                        && pending[j].article_id == item.article_id
                        && pending[j].input.stock.kind != JudgeTarget::Stock
                })
                .collect()
        } else {
            vec![index]
        };
        let mut asked: Vec<usize> = Vec::new();
        {
            let conn = db.lock().expect("DB 잠금이 깨졌습니다");
            for &j in &group {
                if let Some(same) = find_same_news_evaluation(&conn, &pending[j])? {
                    if store_copy(&conn, &pending[j], &same, now())? {
                        summary.relevant += 1;
                    }
                    summary.copied += 1;
                    done[j] = true;
                } else {
                    asked.push(j);
                }
            }
            if asked.is_empty() {
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
        let result: Result<(), DeciderError> = if merged {
            let members: Vec<&Pending> = asked.iter().map(|&j| &pending[j]).collect();
            match decider.decide_merged(&merged_input(&members), &model).await {
                Ok(decision) => {
                    let at = now();
                    let conn = db.lock().expect("DB 잠금이 깨졌습니다");
                    if store_merged_decision(&conn, &members, &decision, price, at, &mut summary)? {
                        Ok(())
                    } else {
                        Err(DeciderError::new(
                            "Jev 응답 형식이 다릅니다.",
                            DeciderErrorKind::Upstream,
                        ))
                    }
                }
                Err(error) => Err(error),
            }
        } else {
            match decider.decide(&item.input, &model).await {
                Ok(decision) => {
                    let at = now();
                    let conn = db.lock().expect("DB 잠금이 깨졌습니다");
                    store_stock_decision(&conn, item, &decision, price, at, &mut summary)?;
                    Ok(())
                }
                Err(error) => Err(error),
            }
        };
        match result {
            Ok(()) => {
                for &j in &asked {
                    done[j] = true;
                }
                failed_in_row = 0;
            }
            Err(error) if error.fatal() => {
                note = Some(error.message);
                stopped_at = index;
                break;
            }
            Err(error) => {
                for &j in &asked {
                    done[j] = true;
                }
                summary.failed += 1;
                failed_in_row += 1;
                log_error(
                    &format!("기사 평가 실패: 연결 {}", pending[asked[0]].match_id),
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

    summary.remaining = done[stopped_at..].iter().filter(|d| !**d).count() as i64;
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
         WHERE evaluations.id IS NULL AND article_matches.gated = 0 AND stocks.deleted_at IS NULL \
           AND articles.published_at >= ?1",
        params![now - EVALUATION_WINDOW_MS],
        |row| row.get(0),
    )
    .optional()
    .map(|n| n.unwrap_or(0))
}

#[cfg(test)]
#[path = "run_tests.rs"]
mod tests;
