use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use tokio::sync::Notify;

use crate::server::ai::decider::types::{Decider, LeadInput, LeadSubject, LeadSubjectType};
use crate::server::ai::llm::schemas::topic_refresh;
use crate::server::ai::llm::{LlmClient, LlmError, LlmErrorKind, StructuredRequest};
use crate::server::budget::usage::{
    BudgetExceededError, CallOptions, GenerateError, RateError, generate_with_budget,
    rate_with_budget,
};
use crate::server::collect::backfill::link_stored_headlines;
use crate::server::collect::feed_headlines::recent_feed_titles;
use crate::server::db::client::{Db, atomically};
use crate::server::keywords::generate::judge_item;
use crate::server::log::{log, log_error};
use crate::server::settings::{
    KEYWORDS_MIN_MAGNITUDE, KEYWORDS_MIN_RELEVANCE, LLM_MODEL, TOPICS_ERROR, TOPICS_FAILED_AT,
    TOPICS_REFRESHED_AT, TOPICS_STARTED_AT, read_num, read_str, write_num, write_str,
};
use crate::server::text::js_to_fixed;
use crate::server::time::{EpochMs, now_ms};
use crate::server::topics::refresh::{
    MAX_TOPICS, PreviousTopic, RefreshedTopic, build_topic_refresh_messages, clean_topic_refresh,
    parse_topic_refresh,
};
use crate::server::topics::store::{
    StockRef, Topic, TopicArticleCounts, TopicLink, active_auto_topics, active_stocks,
    attach_topic_to_stock, attached_topics_by_stock, detach_topic_from_stock, find_topic,
    recently_ended_auto_topics, set_auto_topic_terms, sync_topic_lenses, topic_article_counts,
    topic_lenses,
};

const HOUR_MS: i64 = 60 * 60 * 1000;
/// 한 번에 보는 제목의 기간: 지난 갱신 이후, 길어도 24시간
const MAX_WINDOW_MS: i64 = 24 * HOUR_MS;
/// 새 제목이 이보다 적으면(밤·꺼져 있던 때) 토픽을 바꾸지 않는다.
pub const MIN_REFRESH_TITLES: usize = 20;
/// 한 번에 넣는 제목 수의 상한. 기본 모델(Haiku 5.5)은 입력이 10만 토큰을 넘으면 단가가 5배가 되므로
/// 그보다 넉넉히 아래에 둔다(2026-10-22 결정, 실측 1,500개에 약 6만 토큰).
pub const MAX_REFRESH_TITLES: i64 = 1500;
/// 새 토픽·붙인 가지에 거는 지난 기사 기간(11단계 결정)
const BACKFILL_MS: i64 = 24 * HOUR_MS;
/// 토픽 유지 판단(2026-10-30 결정): 최근 이 기간에 걸린 기사 수를 센다
const KEEP_WINDOW_MS: i64 = 24 * HOUR_MS;
/// AI가 목록에서 뺀 토픽도 `KEEP_WINDOW_MS` 동안 이만큼 이상 걸리고 그중 화면 기준을 통과한 기사가 1건 이상이면
/// 끝내지 않는다(2026-10-30 결정, 비교 뒤 통과 조건을 더함)
pub const KEEP_MIN_ARTICLES: i64 = 5;
/// 끝난 지 이 기간 안의 토픽은 AI에 보여 주고 같은 주제면 같은 id로 되살린다
const REVIVE_WINDOW_MS: i64 = 48 * HOUR_MS;
/// 최대 출력 토큰. 6,000이던 것을 2026-10-30에 올렸다: 새 프롬프트가 제목 444개 입력에서 3번 중 2번 6,000에 닿아 잘렸다
/// (본문은 약 1,800자라 숨은 추론 토큰이 먹은 것으로 보이나 확인하지 못함).
pub const MAX_REFRESH_OUTPUT_TOKENS: u32 = 10_000;
/// 2026-10-22 실측: 제목 1,500개에 약 11초
const REFRESH_TIMEOUT_MS: u64 = 120_000;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TopicRefreshSummary {
    pub titles: usize,
    pub topics: usize,
    pub continued: usize,
    /// 끝난 토픽이 같은 주제로 다시 나와 되살아난 수(continued에도 포함)
    pub revived: usize,
    /// AI가 뺐지만 기사가 계속 걸려 끝내지 않은 수
    pub kept: usize,
    pub created: usize,
    pub ended: usize,
    pub attached: usize,
    pub detached: usize,
    /// 지난 24시간 기사로 새로 건 연결 수
    pub linked: usize,
    /// 바꾸지 않은 이유(제목 부족 등)
    pub skipped: Option<String>,
    /// 관련 종목을 Jev로 평가하지 못한 이유(키 없음·예산 등)
    pub rating_stopped: Option<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum RefreshError {
    #[error(transparent)]
    Budget(#[from] BudgetExceededError),
    #[error(transparent)]
    Llm(#[from] LlmError),
    #[error(transparent)]
    Db(#[from] rusqlite::Error),
}

impl From<GenerateError> for RefreshError {
    fn from(error: GenerateError) -> Self {
        match error {
            GenerateError::Budget(e) => Self::Budget(e),
            GenerateError::Llm(e) => Self::Llm(e),
            GenerateError::Db(e) => Self::Db(e),
        }
    }
}

type NowRef<'a> = Option<&'a (dyn Fn() -> EpochMs + Send + Sync)>;

fn add_to(map: &mut HashMap<i64, HashSet<i64>>, key: i64, value: i64) {
    map.entry(key).or_default().insert(value);
}

/// 자동 토픽을 갱신한다(11단계, 토픽 우선). 지난 갱신 이후(길어도 24시간)의 제목과 이전 토픽 목록으로 AI가
/// 지금 시장을 움직이는 토픽을 정하고(이어지는 것·새것·식은 것), 토픽마다 영향을 받을 내 종목 후보를 낸다.
/// Jev가 토픽×종목 쌍의 관련도·영향도를 매겨 기준을 넘으면 그 종목에 토픽 가지를 붙이고, 아니면 뗀다.
/// 새 토픽 항목과 새로 붙인 가지에는 보관한 제목으로 지난 24시간 기사를 건다(평가는 다음 수집 회차).
/// AI 호출이 실패하면 오류를 돌려준다(스케줄러가 잠시 뒤 다시 한다).
pub async fn refresh_topics<L: LlmClient, D: Decider>(
    db: &Db,
    llm: &L,
    decider: Option<&D>,
    now: NowRef<'_>,
) -> Result<TopicRefreshSummary, RefreshError> {
    let at = now.map_or_else(now_ms, |f| f());
    let mut summary = TopicRefreshSummary::default();
    let (titles, active, stocks_now, min_relevance, min_magnitude, recently_ended) = {
        let conn = db.lock().expect("DB 잠금이 깨졌습니다");
        let refreshed_at = read_num(&conn, &TOPICS_REFRESHED_AT)?;
        let since = (at - MAX_WINDOW_MS).max(if refreshed_at == 0.0 {
            0
        } else {
            refreshed_at as i64
        });
        let titles = recent_feed_titles(&conn, since, MAX_REFRESH_TITLES)?;
        let active = active_auto_topics(&conn)?;
        summary.titles = titles.len();
        if titles.len() < MIN_REFRESH_TITLES {
            write_num(&conn, &TOPICS_REFRESHED_AT, at as f64, at)?;
            summary.topics = active.len();
            summary.skipped = Some(format!(
                "새 제목이 {}개라 토픽을 그대로 둡니다({MIN_REFRESH_TITLES}개 미만).",
                titles.len()
            ));
            return Ok(summary);
        }
        (
            titles,
            active,
            active_stocks(&conn)?,
            read_num(&conn, &KEYWORDS_MIN_RELEVANCE)?,
            read_num(&conn, &KEYWORDS_MIN_MAGNITUDE)?,
            recently_ended_auto_topics(&conn, at - REVIVE_WINDOW_MS)?,
        )
    };
    let item_names: Vec<String> = stocks_now
        .iter()
        .map(|s| s.name_ko.clone().unwrap_or_else(|| s.name.clone()))
        .collect();
    // 진행 중인 토픽과, 끝난 지 48시간 안의 토픽(되살릴 후보)에 최근 24시간 걸린 기사 수를 센다.
    let mut candidates: Vec<Topic> = active.clone();
    candidates.extend(recently_ended.iter().cloned());
    let (counts, model) = {
        let conn = db.lock().expect("DB 잠금이 깨졌습니다");
        (
            topic_article_counts(
                &conn,
                &candidates.iter().map(|t| t.id).collect::<Vec<_>>(),
                at - KEEP_WINDOW_MS,
                min_relevance,
                min_magnitude,
            )?,
            read_str(&conn, &LLM_MODEL)?,
        )
    };
    let ended_ids: HashSet<i64> = recently_ended.iter().map(|t| t.id).collect();
    let previous: Vec<PreviousTopic> = candidates
        .iter()
        .map(|topic| PreviousTopic {
            id: topic.id.to_string(),
            name: topic.name.clone(),
            summary: topic.summary.clone().unwrap_or_default(),
            terms: topic.terms.clone(),
            articles: counts.get(&topic.id).map_or(0, |c| c.matched),
            ended: ended_ids.contains(&topic.id),
        })
        .collect();
    let request = StructuredRequest {
        model,
        messages: build_topic_refresh_messages(&previous, &titles, &item_names),
        schema: topic_refresh(),
        schema_name: "topic_refresh".to_string(),
        max_tokens: MAX_REFRESH_OUTPUT_TOKENS,
        timeout_ms: Some(REFRESH_TIMEOUT_MS),
        parse: parse_topic_refresh,
    };
    let call = CallOptions {
        purpose: "topic_refresh",
        now,
        stock_id: None,
    };
    let value = generate_with_budget(db, llm, &request, &call).await?.value;
    let previous_ids: HashSet<String> = candidates.iter().map(|t| t.id.to_string()).collect();
    let refreshed = continue_topics(
        clean_topic_refresh(&value, &previous_ids, &item_names),
        &candidates,
    );
    // AI가 뺀 진행 중 토픽 가운데 기사가 계속 걸리는 것은 끝내지 않고 둔다(자리가 있을 만큼만).
    let refreshed_topic_ids: HashSet<i64> = refreshed
        .iter()
        .filter_map(|r| r.id.as_deref()?.parse().ok())
        .collect();
    let dropped: Vec<(Topic, TopicArticleCounts)> = active
        .iter()
        .filter(|t| !refreshed_topic_ids.contains(&t.id))
        .map(|t| (t.clone(), counts.get(&t.id).copied().unwrap_or_default()))
        .collect();
    let (carried, _) = retain_dropped(&dropped, MAX_TOPICS as i64 - refreshed.len() as i64);
    let carried_ids: HashSet<i64> = carried.iter().map(|t| t.id).collect();
    summary.kept = carried.len();

    // 토픽을 고치거나 만들고, 이번에 빠진 것은 끝낸다(끝난 토픽은 새 기사를 걸지 않는다).
    let mut new_lenses: Vec<i64> = Vec::new();
    let mut current: Vec<(Topic, RefreshedTopic)> = Vec::new();
    {
        let conn = db.lock().expect("DB 잠금이 깨졌습니다");
        atomically(&conn, || {
            let mut kept: HashSet<i64> = HashSet::new();
            for item in &refreshed {
                let id: Option<i64> = item.id.as_deref().and_then(|s| s.parse().ok());
                let terms =
                    serde_json::to_string(&item.terms).expect("글자 목록은 항상 JSON이 된다");
                let topic = if let Some(id) = id {
                    conn.execute(
                        "UPDATE topics SET name = ?1, summary = ?2, terms = ?3, last_seen_at = ?4, status = 'active', \
                         ended_at = NULL WHERE id = ?5",
                        rusqlite::params![item.name, item.summary, terms, at, id],
                    )?;
                    let topic = find_topic(&conn, id)?.expect("고친 토픽이 있다");
                    kept.insert(id);
                    summary.continued += 1;
                    if ended_ids.contains(&id) {
                        summary.revived += 1;
                    }
                    for lens in topic_lenses(&conn, id)? {
                        if lens.deleted_at.is_none() {
                            set_auto_topic_terms(&conn, lens.id, &topic, at)?;
                        }
                    }
                    topic
                } else {
                    conn.execute(
                        "INSERT INTO topics (source, name, summary, terms, created_at, last_seen_at) \
                         VALUES ('auto', ?1, ?2, ?3, ?4, ?4)",
                        rusqlite::params![item.name, item.summary, terms, at],
                    )?;
                    summary.created += 1;
                    find_topic(&conn, conn.last_insert_rowid())?.expect("만든 토픽이 있다")
                };
                new_lenses.extend(sync_topic_lenses(&conn, &topic, at)?);
                current.push((topic, item.clone()));
            }
            for topic in &active {
                if kept.contains(&topic.id) || carried_ids.contains(&topic.id) {
                    continue;
                }
                conn.execute(
                    "UPDATE topics SET status = 'ended', ended_at = ?1 WHERE id = ?2",
                    rusqlite::params![at, topic.id],
                )?;
                summary.ended += 1;
            }
            Ok(())
        })?;
    }
    summary.topics = current.len() + carried.len();

    // 토픽×종목: AI가 낸 관련 종목 후보를 Jev로 평가해 붙이거나 뗀다.
    let attached_before = {
        let conn = db.lock().expect("DB 잠금이 깨졌습니다");
        attached_topics_by_stock(&conn)?
    };
    let was_attached = |stock_id: i64, topic_id: i64| {
        attached_before
            .get(&stock_id)
            .is_some_and(|s| s.contains(&topic_id))
    };
    let mut wanted: HashMap<i64, HashSet<i64>> = HashMap::new();
    let mut newly_attached: Vec<i64> = Vec::new();
    let mut rating_stopped: Option<String> = if decider.is_some() {
        None
    } else {
        Some("TYPESAFE_API_KEY가 없습니다.".to_string())
    };
    for (topic, item) in &current {
        for relation in &item.related {
            let Some(stock) = stocks_now
                .iter()
                .find(|row| row.name_ko.as_deref().unwrap_or(&row.name) == relation.item)
            else {
                continue;
            };
            let Some(decider) = decider.filter(|_| rating_stopped.is_none()) else {
                // 평가할 수 없으면 이미 붙어 있던 가지는 그대로 둔다.
                if was_attached(stock.id, topic.id) {
                    add_to(&mut wanted, stock.id, topic.id);
                }
                continue;
            };
            let input = LeadInput {
                stock: judge_item(stock, None),
                subject: LeadSubject {
                    kind: LeadSubjectType::Topic,
                    text: topic.name.clone(),
                    path: None,
                    summary: topic.summary.clone(),
                    terms: Some(topic.terms.clone()),
                    reason: Some(relation.reason.clone()).filter(|r| !r.is_empty()),
                },
            };
            let call = CallOptions {
                purpose: "keyword_rating",
                now,
                stock_id: Some(stock.id),
            };
            match rate_with_budget(db, decider, &input, &call).await {
                Ok(rating) => {
                    if rating.relevance >= min_relevance && rating.magnitude >= min_magnitude {
                        add_to(&mut wanted, stock.id, topic.id);
                        let conn = db.lock().expect("DB 잠금이 깨졌습니다");
                        let fresh = attach_topic_to_stock(
                            &conn,
                            &StockRef::from(stock),
                            topic,
                            &TopicLink {
                                reason: &relation.reason,
                                relevance: rating.relevance,
                                magnitude: rating.magnitude,
                            },
                            at,
                        )?;
                        if fresh {
                            summary.attached += 1;
                            newly_attached.push(stock.id);
                        }
                    }
                }
                Err(error) => {
                    let stop = match &error {
                        RateError::Budget(e) => Some(e.to_string()),
                        RateError::Decider(e) if e.fatal() => Some(e.message.clone()),
                        _ => None,
                    };
                    if let Some(message) = stop {
                        rating_stopped = Some(message);
                    } else {
                        log_error(
                            &format!("토픽 평가 실패: {} × {}", topic.name, relation.item),
                            &error,
                        );
                    }
                    if was_attached(stock.id, topic.id) {
                        add_to(&mut wanted, stock.id, topic.id);
                    }
                }
            }
        }
    }
    summary.rating_stopped = rating_stopped;
    // 끝내지 않고 둔 토픽은 AI가 관련 종목을 다시 내지 않았으므로 붙은 가지를 그대로 둔다.
    for (stock_id, topic_ids) in &attached_before {
        for topic_id in topic_ids {
            if carried_ids.contains(topic_id) {
                add_to(&mut wanted, *stock_id, *topic_id);
            }
        }
    }
    {
        let conn = db.lock().expect("DB 잠금이 깨졌습니다");
        for (stock_id, topic_ids) in &attached_before {
            for topic_id in topic_ids {
                if wanted.get(stock_id).is_some_and(|s| s.contains(topic_id)) {
                    continue;
                }
                if detach_topic_from_stock(&conn, *stock_id, *topic_id)? {
                    summary.detached += 1;
                }
            }
        }
    }

    // 새 토픽 항목과 새로 붙인 가지에 지난 24시간 기사를 건다(11단계 결정).
    let mut link_targets = new_lenses;
    link_targets.extend(newly_attached);
    let mut conn = db.lock().expect("DB 잠금이 깨졌습니다");
    if !link_targets.is_empty() {
        match link_stored_headlines(&mut conn, &link_targets, at - BACKFILL_MS, at) {
            Ok((_, matches)) => summary.linked = matches,
            Err(error) => log_error("지난 기사 걸기 실패", &error),
        }
    }
    write_num(&conn, &TOPICS_REFRESHED_AT, at as f64, at)?;
    Ok(summary)
}

/// AI가 목록에서 뺀 진행 중 토픽 가운데 최근 24시간 걸린 기사가 `KEEP_MIN_ARTICLES`건 이상이고 화면 기준을 통과한 기사가
/// 있는 것을 끝내지 않고 둔다.
/// 자리(`room`)가 모자라면 화면 기준을 통과한 기사가 적은 것(같으면 걸린 기사가 적은 것, 같으면 오래된 것)부터 끝낸다.
pub fn retain_dropped(
    dropped: &[(Topic, TopicArticleCounts)],
    room: i64,
) -> (Vec<Topic>, Vec<Topic>) {
    let mut eligible: Vec<&(Topic, TopicArticleCounts)> = dropped
        .iter()
        .filter(|(_, c)| c.matched >= KEEP_MIN_ARTICLES && c.shown >= 1)
        .collect();
    eligible.sort_by(|a, b| {
        b.1.shown
            .cmp(&a.1.shown)
            .then_with(|| b.1.matched.cmp(&a.1.matched))
            .then_with(|| b.0.id.cmp(&a.0.id))
    });
    let slots = usize::try_from(room.max(0)).unwrap_or(0);
    let carried_ids: HashSet<i64> = eligible.iter().take(slots).map(|(t, _)| t.id).collect();
    let carried = dropped
        .iter()
        .filter(|(t, _)| carried_ids.contains(&t.id))
        .map(|(t, _)| t.clone())
        .collect();
    let ended = dropped
        .iter()
        .filter(|(t, _)| !carried_ids.contains(&t.id))
        .map(|(t, _)| t.clone())
        .collect();
    (carried, ended)
}

/// AI가 이전 토픽의 id를 돌려주지 않을 때가 있어(2026-10-22 실측: 이어진 9개를 모두 새 토픽으로 냄),
/// id가 없는 토픽은 이름이 같거나 검색어가 하나라도 같은 이전 토픽으로 잇는다.
pub fn continue_topics(refreshed: Vec<RefreshedTopic>, previous: &[Topic]) -> Vec<RefreshedTopic> {
    fn key(text: &str) -> String {
        crate::server::topics::refresh::whitespace_free_lowercase(text)
    }
    let mut used: HashSet<String> = refreshed.iter().filter_map(|t| t.id.clone()).collect();
    refreshed
        .into_iter()
        .map(|topic| {
            if topic.id.is_some() {
                return topic;
            }
            let terms: HashSet<String> = topic.terms.iter().map(|t| key(t)).collect();
            let found = previous.iter().find(|prev| {
                !used.contains(&prev.id.to_string())
                    && (key(&prev.name) == key(&topic.name)
                        || prev.terms.iter().any(|term| terms.contains(&key(term))))
            });
            match found {
                None => topic,
                Some(prev) => {
                    used.insert(prev.id.to_string());
                    RefreshedTopic {
                        id: Some(prev.id.to_string()),
                        ..topic
                    }
                }
            }
        })
        .collect()
}

type NowFn = Arc<dyn Fn() -> EpochMs + Send + Sync>;

/// 스케줄러·"지금 갱신"용: 실패해도 던지지 않고 로그만 남긴다. 성공하면 요약 한 줄.
/// 시작·실패 시각과 이유를 설정에 남겨 상태 화면이 보이고, 실패하면 30분 뒤 다시 한다(topicPlan).
pub async fn refresh_topics_and_log<L: LlmClient, D: Decider>(
    db: &Db,
    llm: &L,
    decider: Option<&D>,
    now: Option<&NowFn>,
) -> bool {
    let started = Instant::now();
    let now_ref: NowRef<'_> = now.map(|f| &**f as &(dyn Fn() -> EpochMs + Send + Sync));
    let at = || now_ref.map_or_else(now_ms, |f| f());
    {
        let conn = db.lock().expect("DB 잠금이 깨졌습니다");
        if let Err(error) = write_num(&conn, &TOPICS_STARTED_AT, at() as f64, at()) {
            log_error("토픽 갱신 시작 기록 실패", &error);
        }
    }
    match refresh_topics(db, llm, decider, now_ref).await {
        Ok(s) => {
            let seconds = js_to_fixed(started.elapsed().as_secs_f64(), 1);
            if let Some(skipped) = &s.skipped {
                log(&format!("토픽 갱신 건너뜀: {skipped}"));
            } else {
                log(&format!(
                    "토픽 갱신: 제목 {}개 → 토픽 {}개(이어짐 {}(되살림 {}), 유지 {}, 새로 {}, 끝남 {}), 종목에 붙임 {}·뗌 {}, 지난 기사 {}건 연결{}, {seconds}초",
                    s.titles,
                    s.topics,
                    s.continued,
                    s.revived,
                    s.kept,
                    s.created,
                    s.ended,
                    s.attached,
                    s.detached,
                    s.linked,
                    s.rating_stopped
                        .as_ref()
                        .map_or_else(String::new, |r| format!(", 관련 종목 평가 못 함: {r}"))
                ));
            }
            true
        }
        Err(error) => {
            log_error("토픽 갱신 실패", &error);
            let conn = db.lock().expect("DB 잠금이 깨졌습니다");
            let failed = write_num(&conn, &TOPICS_FAILED_AT, at() as f64, at()).and_then(|()| {
                write_str(&conn, &TOPICS_ERROR, &describe_refresh_error(&error), at())
            });
            if let Err(e) = failed {
                log_error("토픽 갱신 실패 기록 실패", &e);
            }
            false
        }
    }
}

/// 상태 화면에 보일 실패 이유
pub fn describe_refresh_error(error: &RefreshError) -> String {
    match error {
        RefreshError::Budget(_) => "이번 달 AI 예산을 모두 썼습니다.",
        RefreshError::Llm(e) => match e.kind {
            LlmErrorKind::NoApiKey => "OpenRouter API 키가 없습니다.",
            LlmErrorKind::PaymentRequired => {
                "OpenRouter 크레딧이 부족하거나 키의 사용 한도에 걸렸습니다."
            }
            LlmErrorKind::RateLimited => "OpenRouter 요청이 많아 잠시 막혔습니다.",
            LlmErrorKind::Timeout => "AI 응답이 늦어 멈췄습니다.",
            LlmErrorKind::InvalidOutput => "AI 응답 형식이 맞지 않았습니다.",
            LlmErrorKind::Upstream => "OpenRouter 오류가 났습니다.",
        },
        RefreshError::Db(_) => "알 수 없는 오류가 났습니다(worker 로그 확인).",
    }
    .to_string()
}

struct RefresherInner<L, D> {
    db: Db,
    llm: Arc<L>,
    decider: Option<Arc<D>>,
    now: Option<NowFn>,
    state: Mutex<RefresherState>,
    notify: Notify,
}

#[derive(Default)]
struct RefresherState {
    running: bool,
    last_result: bool,
}

/// 정기 갱신(스케줄러)과 "지금 갱신"(상태 화면)이 겹쳐 돌지 않게 하나로 묶는다. tokio 런타임 안에서만 쓴다.
pub struct TopicRefresher<L, D> {
    inner: Arc<RefresherInner<L, D>>,
}

impl<L, D> Clone for TopicRefresher<L, D> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
        }
    }
}

impl<L, D> TopicRefresher<L, D>
where
    L: LlmClient + 'static,
    D: Decider + 'static,
{
    pub fn new(db: Db, llm: Arc<L>, decider: Option<Arc<D>>, now: Option<NowFn>) -> Self {
        Self {
            inner: Arc::new(RefresherInner {
                db,
                llm,
                decider,
                now,
                state: Mutex::new(RefresherState::default()),
                notify: Notify::new(),
            }),
        }
    }

    /// 갱신을 시작한다. 이미 하고 있으면 그 갱신을 기다린다. 던지지 않는다(성공하면 true).
    pub async fn refresh(&self) -> bool {
        let start = {
            let mut state = self
                .inner
                .state
                .lock()
                .expect("토픽 갱신 잠금이 깨졌습니다");
            !std::mem::replace(&mut state.running, true)
        };
        if start {
            let inner = self.inner.clone();
            tokio::spawn(async move {
                let ok = refresh_topics_and_log(
                    &inner.db,
                    &*inner.llm,
                    inner.decider.as_deref(),
                    inner.now.as_ref(),
                )
                .await;
                let mut state = inner.state.lock().expect("토픽 갱신 잠금이 깨졌습니다");
                state.running = false;
                state.last_result = ok;
                drop(state);
                inner.notify.notify_waiters();
            });
        }
        loop {
            let notified = self.inner.notify.notified();
            tokio::pin!(notified);
            notified.as_mut().enable();
            {
                let state = self
                    .inner
                    .state
                    .lock()
                    .expect("토픽 갱신 잠금이 깨졌습니다");
                if !state.running {
                    return state.last_result;
                }
            }
            notified.await;
        }
    }

    pub fn running(&self) -> bool {
        self.inner
            .state
            .lock()
            .expect("토픽 갱신 잠금이 깨졌습니다")
            .running
    }
}
