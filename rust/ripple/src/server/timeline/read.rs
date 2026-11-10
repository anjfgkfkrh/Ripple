//! 통합 타임라인 읽기(`timeline/read.ts`, SPEC FR-7). web은 읽기만 한다.
use std::collections::{HashMap, HashSet};

use rusqlite::types::Value;
use rusqlite::{Connection, Row, params_from_iter};

use crate::server::db::schema::{Impact, ItemKind, Market, MatchKind, TopicSource};
use crate::server::keywords::tree::PATH_SEPARATOR;
use crate::server::settings::{EVALUATION_MIN_MAGNITUDE, EVALUATION_MIN_RELEVANCE, read_num};
use crate::server::similar_title::{
    SAME_NEWS_WINDOW_MS, TitleFeatures, is_same_news, title_features,
};
use crate::server::text::js_parse_number;
use crate::server::time::EpochMs;
use crate::server::timeline::reads::read_article_ids;
use crate::server::topics::store::{TOPIC_COLUMNS, row_to_topic};

/// "더 보기" 한 번에 늘리는 카드 수(D-25)
pub const TIMELINE_PAGE_SIZE: usize = 30;
/// 주소를 손으로 바꿔도 조회가 지나치게 무거워지지 않게 하는 상한
pub const TIMELINE_MAX: usize = 600;
/// 손님(`GUEST_VIEW`)이 볼 수 있는 상한. 공개 주소로 오는 무거운 조회를 막는다.
pub const GUEST_TIMELINE_MAX: usize = 90;
/// 같은 소식(비슷한 제목)을 한 카드로 묶는 기간(11단계, 평가 복사와 같은 기준)
pub const SAME_TITLE_WINDOW_MS: i64 = SAME_NEWS_WINDOW_MS;
/// 기사를 이만큼씩 읽어 카드를 만든다.
const SCAN_BATCH: usize = 200;
const HOUR_MS: i64 = 60 * 60 * 1000;
/// 자동 토픽: 새 기사가 이만큼 없으면 '이전 이슈'로 접고, 이만큼 지나면 숨긴다(11단계 결정)
pub const TOPIC_FADE_MS: i64 = 12 * HOUR_MS;
pub const TOPIC_HIDE_MS: i64 = 72 * HOUR_MS;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct TimelineFilter {
    /// 종목·시장 항목 하나
    pub stock_id: Option<i64>,
    /// 주제 하나(사용자 주제·자동 토픽, 증시별 항목을 모두)
    pub topic_id: Option<i64>,
    pub impact: Option<Impact>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TimelineStock {
    pub stock_id: i64,
    pub kind: ItemKind,
    pub market: Market,
    /// 주제 항목: 토픽과 그 출처
    pub topic_id: Option<i64>,
    pub topic_source: Option<TopicSource>,
    pub name: String,
    pub relevance: f64,
    /// 영향도. 11단계 전에 평가한 기사는 없음
    pub magnitude: Option<f64>,
    pub impact: Impact,
    pub match_kind: MatchKind,
    pub path: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OtherArticle {
    pub article_id: i64,
    pub publisher: String,
    pub url: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TimelineCard {
    pub article_id: i64,
    pub title: String,
    pub url: String,
    pub publisher: String,
    pub published_at: EpochMs,
    /// 기준 이상인 항목(같은 소식으로 묶인 기사의 항목까지), 관련도 높은 순(D-26)
    pub stocks: Vec<TimelineStock>,
    /// 같은 소식(비슷한 제목)의 다른 언론사 기사(24시간 안, 최근 것부터)
    pub others: Vec<OtherArticle>,
    /// 12단계: 카드에 묶인 기사(대표 포함) 가운데 하나라도 읽었으면 읽은 카드
    pub read: bool,
}

/// 주소의 검색 값(stock, topic, impact, limit)을 읽는다. 개수는 `max_limit`(회원 `TIMELINE_MAX`, 손님 `GUEST_TIMELINE_MAX`)까지. 없거나 맞지 않는 값은 무시한다.
/// 같은 이름이 여러 번 나오면 첫 값을 쓴다.
pub fn parse_timeline_params(
    params: &[(String, String)],
    active_stock_ids: &[i64],
    topic_ids: &[i64],
    max_limit: usize,
) -> (TimelineFilter, usize) {
    let one = |key: &str| {
        params
            .iter()
            .find(|(name, _)| name == key)
            .map(|(_, value)| value.as_str())
    };
    // Number(undefined)는 NaN
    let number = |key: &str| one(key).map_or(f64::NAN, js_parse_number);
    let contains = |ids: &[i64], value: f64| ids.iter().find(|id| **id as f64 == value).copied();
    let limit = number("limit");
    let page = TIMELINE_PAGE_SIZE as f64;
    (
        TimelineFilter {
            stock_id: contains(active_stock_ids, number("stock")),
            topic_id: contains(topic_ids, number("topic")),
            impact: one("impact").and_then(|value| value.parse().ok()),
        },
        if limit.is_finite() {
            (max_limit as f64).min(page.max((limit / page).ceil() * page)) as usize
        } else {
            TIMELINE_PAGE_SIZE
        },
    )
}

/// 화면에 보일 평가: 관련도와 영향도가 둘 다 기준 이상(11단계). 11단계 전 평가(영향도 없음)는 관련도만 본다.
/// 조건 글자와 거기 들어갈 값(`?` 둘)을 돌려준다.
fn display_rule(conn: &Connection) -> rusqlite::Result<(&'static str, [Value; 2])> {
    Ok((
        "evaluations.relevance >= ? AND (evaluations.magnitude IS NULL OR evaluations.magnitude >= ?)",
        [
            Value::Real(read_num(conn, &EVALUATION_MIN_RELEVANCE)?),
            Value::Real(read_num(conn, &EVALUATION_MIN_MAGNITUDE)?),
        ],
    ))
}

fn enum_col<T: std::str::FromStr<Err = String>>(
    row: &Row<'_>,
    index: usize,
) -> rusqlite::Result<T> {
    let text: String = row.get(index)?;
    text.parse().map_err(|e: String| {
        rusqlite::Error::FromSqlConversionFailure(index, rusqlite::types::Type::Text, e.into())
    })
}

fn marks(count: usize) -> String {
    vec!["?"; count].join(", ")
}

pub struct TimelinePage {
    pub cards: Vec<TimelineCard>,
    pub has_more: bool,
    pub min_relevance: f64,
    pub min_magnitude: f64,
}

struct ArticleRow {
    id: i64,
    title: String,
    url: String,
    publisher: String,
    published_at: EpochMs,
}

/// 통합 타임라인(SPEC FR-7): 표시 기준 이상인 기사를 최신순으로, 기사 하나에 카드 하나(D-26).
/// 종목·주제 필터는 그 항목이, 영향 필터는 그 영향인 항목이 카드에 있으면 통과한다.
/// 같은 소식(비슷한 제목)을 24시간 안에 낸 기사는 최근 것 하나로 묶고 항목은 모은다(화면에서만, 11단계).
/// 삭제한 항목은 빼고(D-11), web은 읽기만 한다.
pub fn read_timeline(
    conn: &Connection,
    filter: &TimelineFilter,
    limit: usize,
) -> rusqlite::Result<TimelinePage> {
    let min_relevance = read_num(conn, &EVALUATION_MIN_RELEVANCE)?;
    let min_magnitude = read_num(conn, &EVALUATION_MIN_MAGNITUDE)?;
    let (rule, rule_values) = display_rule(conn)?;
    let mut conditions = vec!["stocks.deleted_at IS NULL".to_string(), rule.to_string()];
    let mut values: Vec<Value> = rule_values.to_vec();
    if let Some(stock_id) = filter.stock_id {
        conditions.push("evaluations.stock_id = ?".to_string());
        values.push(Value::Integer(stock_id));
    }
    if let Some(topic_id) = filter.topic_id {
        conditions.push("stocks.topic_id = ?".to_string());
        values.push(Value::Integer(topic_id));
    }
    if let Some(impact) = filter.impact {
        conditions.push("evaluations.impact = ?".to_string());
        values.push(Value::Text(impact.as_str().to_string()));
    }
    let page_sql = format!(
        "SELECT DISTINCT articles.id, articles.title, articles.url, articles.publisher, articles.published_at \
         FROM evaluations \
         INNER JOIN articles ON articles.id = evaluations.article_id \
         INNER JOIN stocks ON stocks.id = evaluations.stock_id \
         WHERE {} \
         ORDER BY articles.published_at DESC, articles.id DESC LIMIT {SCAN_BATCH} OFFSET ?",
        conditions.join(" AND ")
    );
    let mut page_stmt = conn.prepare(&page_sql)?;
    let mut page = |offset: usize| -> rusqlite::Result<Vec<ArticleRow>> {
        let mut params = values.clone();
        params.push(Value::Integer(offset as i64));
        let rows = page_stmt.query_map(params_from_iter(params), |row| {
            Ok(ArticleRow {
                id: row.get(0)?,
                title: row.get(1)?,
                url: row.get(2)?,
                publisher: row.get(3)?,
                published_at: row.get(4)?,
            })
        })?;
        rows.collect()
    };

    let mut cards: Vec<TimelineCard> = Vec::new();
    let mut features: Vec<TitleFeatures> = Vec::new();
    // 카드마다 묶인 기사 id(대표 포함): 항목을 모을 때 쓴다
    let mut members: Vec<Vec<i64>> = Vec::new();
    let mut offset = 0;
    'scan: loop {
        let rows = page(offset)?;
        for row in &rows {
            // 보여 줄 카드 중 가장 오래된 것과도 24시간 넘게 떨어지면 더 묶일 기사가 없다.
            if cards.len() > limit
                && cards[limit - 1].published_at - row.published_at > SAME_TITLE_WINDOW_MS
            {
                break 'scan;
            }
            let mine = title_features(&row.title);
            let mut group = None;
            for i in (0..cards.len()).rev() {
                if cards[i].published_at - row.published_at > SAME_TITLE_WINDOW_MS {
                    break;
                }
                if is_same_news(&mine, &features[i]) {
                    group = Some(i);
                    break;
                }
            }
            if let Some(i) = group {
                cards[i].others.push(OtherArticle {
                    article_id: row.id,
                    publisher: row.publisher.clone(),
                    url: row.url.clone(),
                });
                members[i].push(row.id);
                continue;
            }
            cards.push(TimelineCard {
                article_id: row.id,
                title: row.title.clone(),
                url: row.url.clone(),
                publisher: row.publisher.clone(),
                published_at: row.published_at,
                stocks: Vec::new(),
                others: Vec::new(),
                read: false,
            });
            features.push(mine);
            members.push(vec![row.id]);
        }
        if rows.len() < SCAN_BATCH {
            break;
        }
        offset += SCAN_BATCH;
    }

    let has_more = cards.len() > limit;
    cards.truncate(limit);
    if !cards.is_empty() {
        let mut card_of: HashMap<i64, usize> = HashMap::new();
        for (index, ids) in members.iter().take(cards.len()).enumerate() {
            for id in ids {
                card_of.insert(*id, index);
            }
        }
        let ids: Vec<i64> = card_of.keys().copied().collect();
        for id in read_article_ids(conn, &ids)? {
            if let Some(index) = card_of.get(&id) {
                cards[*index].read = true;
            }
        }
        let (rule, rule_values) = display_rule(conn)?;
        let sql = format!(
            "SELECT evaluations.article_id, stocks.id, stocks.kind, stocks.market, stocks.topic_id, topics.source, \
             stocks.name, stocks.name_ko, evaluations.relevance, evaluations.magnitude, evaluations.impact, \
             article_matches.match_kind, article_matches.keyword_path \
             FROM evaluations \
             INNER JOIN article_matches ON article_matches.id = evaluations.match_id \
             INNER JOIN stocks ON stocks.id = evaluations.stock_id \
             LEFT JOIN topics ON topics.id = stocks.topic_id \
             WHERE evaluations.article_id IN ({}) AND stocks.deleted_at IS NULL AND {rule}",
            marks(ids.len())
        );
        let mut params: Vec<Value> = ids.iter().map(|id| Value::Integer(*id)).collect();
        params.extend(rule_values);
        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt.query_map(params_from_iter(params), |row| {
            let article_id: i64 = row.get(0)?;
            let name: String = row.get(6)?;
            let name_ko: Option<String> = row.get(7)?;
            let source: Option<String> = row.get(5)?;
            Ok((
                article_id,
                TimelineStock {
                    stock_id: row.get(1)?,
                    kind: enum_col(row, 2)?,
                    market: enum_col(row, 3)?,
                    topic_id: row.get(4)?,
                    topic_source: match source {
                        Some(text) => Some(text.parse().map_err(|e: String| {
                            rusqlite::Error::FromSqlConversionFailure(
                                5,
                                rusqlite::types::Type::Text,
                                e.into(),
                            )
                        })?),
                        None => None,
                    },
                    name: name_ko.unwrap_or(name),
                    relevance: row.get(8)?,
                    magnitude: row.get(9)?,
                    impact: enum_col(row, 10)?,
                    match_kind: enum_col(row, 11)?,
                    path: row.get(12)?,
                },
            ))
        })?;
        for row in rows {
            let (article_id, item) = row?;
            let Some(index) = card_of.get(&article_id) else {
                continue;
            };
            let card = &mut cards[*index];
            // 같은 소식으로 묶인 기사들이 같은 항목에 걸렸으면 관련도가 높은 것 하나만
            match card.stocks.iter().position(|s| s.stock_id == item.stock_id) {
                None => card.stocks.push(item),
                Some(same) if card.stocks[same].relevance < item.relevance => {
                    card.stocks[same] = item;
                }
                Some(_) => {}
            }
        }
        for card in &mut cards {
            card.stocks.sort_by(|a, b| {
                b.relevance
                    .partial_cmp(&a.relevance)
                    .unwrap_or(std::cmp::Ordering::Equal)
                    .then(a.stock_id.cmp(&b.stock_id))
            });
        }
    }
    Ok(TimelinePage {
        cards,
        has_more,
        min_relevance,
        min_magnitude,
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MarketImpact {
    pub market: Market,
    pub impact: Impact,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RelatedStock {
    pub stock_id: i64,
    pub name: String,
    pub impact: Option<Impact>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TopicCard {
    pub topic_id: i64,
    pub name: String,
    pub summary: Option<String>,
    /// 12시간 넘게 새 기사가 없어 '이전 이슈'로 접은 토픽
    pub faded: bool,
    pub last_article_at: EpochMs,
    /// 표시 기준 이상인 기사 수(최근 24시간)와 전체
    pub shown_24h: usize,
    pub shown_total: usize,
    /// 켜진 증시마다 표시 기준 이상 기사들의 호재/악재(많은 쪽)
    pub impacts: Vec<MarketImpact>,
    /// 이 토픽 가지가 붙은 내 종목과, 그 가지로 걸린 기준 이상 기사들의 호재/악재(없으면 None)
    pub related: Vec<RelatedStock>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ScoredImpact {
    pub impact: Impact,
    pub relevance: f64,
    pub magnitude: Option<f64>,
}

/// 많은 쪽 호재/악재(같으면 점수가 가장 높은 평가 쪽, 11단계 결정)
pub fn majority_impact(items: &[ScoredImpact]) -> Option<Impact> {
    if items.is_empty() {
        return None;
    }
    // 처음 나온 순서를 지킨다(JS Map)
    let mut counts: Vec<(Impact, usize)> = Vec::new();
    for item in items {
        match counts.iter_mut().find(|(impact, _)| *impact == item.impact) {
            Some((_, n)) => *n += 1,
            None => counts.push((item.impact, 1)),
        }
    }
    let top = counts.iter().map(|(_, n)| *n).max().unwrap_or(0);
    let tied: Vec<Impact> = counts
        .iter()
        .filter(|(_, n)| *n == top)
        .map(|(impact, _)| *impact)
        .collect();
    if tied.len() == 1 {
        return Some(tied[0]);
    }
    let strength = |s: &ScoredImpact| s.relevance + s.magnitude.unwrap_or(0.0);
    let mut best: Option<&ScoredImpact> = None;
    for item in items.iter().filter(|s| tied.contains(&s.impact)) {
        // 안정 정렬 뒤 첫 원소: 같은 점수면 먼저 나온 것
        if best.is_none_or(|b| strength(item) > strength(b)) {
            best = Some(item);
        }
    }
    best.map(|s| s.impact)
}

struct Lens {
    id: i64,
    topic_id: i64,
    market: Market,
}

struct ShownRow {
    stock_id: i64,
    article_id: i64,
    published_at: EpochMs,
    scored: ScoredImpact,
}

struct Attached {
    stock_id: i64,
    name: String,
    topic_id: i64,
    path: String,
}

struct AttachedShown {
    stock_id: i64,
    path: String,
    scored: ScoredImpact,
}

fn values_of(ids: &[i64]) -> Vec<Value> {
    ids.iter().map(|id| Value::Integer(*id)).collect()
}

/// 타임라인 맨 위 '지금 시장 이슈'(11단계): 표시 기준 이상 기사가 하나라도 있는 자동 토픽을 그런 기사가 많은 순으로.
/// 12시간 동안 새 기사가 없으면 '이전 이슈'로 접고, 3일이 지나면 숨긴다.
pub fn read_topic_cards(conn: &Connection, now: EpochMs) -> rusqlite::Result<Vec<TopicCard>> {
    let auto_topics = {
        let mut stmt = conn.prepare(&format!(
            "SELECT {TOPIC_COLUMNS} FROM topics WHERE source = 'auto' AND deleted_at IS NULL AND scope <> 'company'"
        ))?;
        stmt.query_map([], row_to_topic)?
            .collect::<rusqlite::Result<Vec<_>>>()?
    };
    if auto_topics.is_empty() {
        return Ok(Vec::new());
    }
    let topic_ids: Vec<i64> = auto_topics.iter().map(|t| t.id).collect();
    let lenses = {
        let mut stmt = conn.prepare(&format!(
            "SELECT id, topic_id, market FROM stocks WHERE kind = 'topic' AND topic_id IN ({}) AND deleted_at IS NULL",
            marks(topic_ids.len())
        ))?;
        stmt.query_map(params_from_iter(values_of(&topic_ids)), |row| {
            Ok(Lens {
                id: row.get(0)?,
                topic_id: row.get(1)?,
                market: enum_col(row, 2)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?
    };
    if lenses.is_empty() {
        return Ok(Vec::new());
    }
    let lens_ids: Vec<i64> = lenses.iter().map(|l| l.id).collect();
    let lens_of: HashMap<i64, &Lens> = lenses.iter().map(|l| (l.id, l)).collect();

    let mut last_by_topic: HashMap<i64, EpochMs> = HashMap::new();
    {
        let mut stmt = conn.prepare(&format!(
            "SELECT article_matches.stock_id, MAX(articles.published_at) FROM article_matches \
             INNER JOIN articles ON articles.id = article_matches.article_id \
             WHERE article_matches.stock_id IN ({}) GROUP BY article_matches.stock_id",
            marks(lens_ids.len())
        ))?;
        let rows = stmt.query_map(params_from_iter(values_of(&lens_ids)), |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, Option<i64>>(1)?))
        })?;
        for row in rows {
            let (stock_id, at) = row?;
            let (Some(lens), Some(at)) = (lens_of.get(&stock_id), at) else {
                continue;
            };
            let last = last_by_topic.entry(lens.topic_id).or_insert(0);
            *last = (*last).max(at);
        }
    }

    let (rule, rule_values) = display_rule(conn)?;
    let shown_rows = {
        let mut stmt = conn.prepare(&format!(
            "SELECT evaluations.stock_id, evaluations.article_id, articles.published_at, evaluations.relevance, \
             evaluations.magnitude, evaluations.impact FROM evaluations \
             INNER JOIN articles ON articles.id = evaluations.article_id \
             WHERE evaluations.stock_id IN ({}) AND {rule}",
            marks(lens_ids.len())
        ))?;
        let mut params = values_of(&lens_ids);
        params.extend(rule_values.clone());
        stmt.query_map(params_from_iter(params), |row| {
            Ok(ShownRow {
                stock_id: row.get(0)?,
                article_id: row.get(1)?,
                published_at: row.get(2)?,
                scored: ScoredImpact {
                    relevance: row.get(3)?,
                    magnitude: row.get(4)?,
                    impact: enum_col(row, 5)?,
                },
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?
    };

    // 붙은 내 종목: 토픽 가지(1단계, topic_id)가 있는 종목
    let attached = {
        let mut stmt = conn.prepare(&format!(
            "SELECT stocks.id, stocks.name, stocks.name_ko, keywords.topic_id, keywords.path FROM keywords \
             INNER JOIN stocks ON stocks.id = keywords.stock_id \
             WHERE keywords.topic_id IN ({}) AND keywords.level = 1 AND stocks.kind = 'stock' AND stocks.deleted_at IS NULL",
            marks(topic_ids.len())
        ))?;
        stmt.query_map(params_from_iter(values_of(&topic_ids)), |row| {
            let name: String = row.get(1)?;
            let name_ko: Option<String> = row.get(2)?;
            Ok(Attached {
                stock_id: row.get(0)?,
                name: name_ko.unwrap_or(name),
                topic_id: row.get(3)?,
                path: row.get(4)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?
    };
    let attached_shown = if attached.is_empty() {
        Vec::new()
    } else {
        let stock_ids: Vec<i64> = attached.iter().map(|a| a.stock_id).collect();
        let mut stmt = conn.prepare(&format!(
            "SELECT evaluations.stock_id, article_matches.keyword_path, evaluations.relevance, \
             evaluations.magnitude, evaluations.impact FROM evaluations \
             INNER JOIN article_matches ON article_matches.id = evaluations.match_id \
             WHERE evaluations.stock_id IN ({}) AND {rule}",
            marks(stock_ids.len())
        ))?;
        let mut params = values_of(&stock_ids);
        params.extend(rule_values);
        stmt.query_map(params_from_iter(params), |row| {
            Ok(AttachedShown {
                stock_id: row.get(0)?,
                path: row.get(1)?,
                scored: ScoredImpact {
                    relevance: row.get(2)?,
                    magnitude: row.get(3)?,
                    impact: enum_col(row, 4)?,
                },
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?
    };

    let mut cards: Vec<TopicCard> = Vec::new();
    for topic in &auto_topics {
        let Some(&last) = last_by_topic.get(&topic.id) else {
            continue;
        };
        let age = now - last;
        if age > TOPIC_HIDE_MS {
            continue;
        }
        let mine: Vec<&ShownRow> = shown_rows
            .iter()
            .filter(|row| lens_of.get(&row.stock_id).map(|l| l.topic_id) == Some(topic.id))
            .collect();
        if mine.is_empty() {
            continue;
        }
        let article_ids: HashSet<i64> = mine.iter().map(|row| row.article_id).collect();
        let recent: HashSet<i64> = mine
            .iter()
            .filter(|row| now - row.published_at <= 24 * HOUR_MS)
            .map(|row| row.article_id)
            .collect();
        let mut markets: Vec<Market> = Vec::new();
        for row in &mine {
            let market = lens_of[&row.stock_id].market;
            if !markets.contains(&market) {
                markets.push(market);
            }
        }
        markets.sort_by_key(|market| market.as_str());
        let impacts = markets
            .into_iter()
            .filter_map(|market| {
                let scored: Vec<ScoredImpact> = mine
                    .iter()
                    .filter(|row| lens_of[&row.stock_id].market == market)
                    .map(|row| row.scored)
                    .collect();
                majority_impact(&scored).map(|impact| MarketImpact { market, impact })
            })
            .collect();
        let related = attached
            .iter()
            .filter(|row| row.topic_id == topic.id)
            .map(|row| {
                let prefix = format!("{}{PATH_SEPARATOR}", row.path);
                let scored: Vec<ScoredImpact> = attached_shown
                    .iter()
                    .filter(|e| {
                        e.stock_id == row.stock_id
                            && (e.path == row.path || e.path.starts_with(&prefix))
                    })
                    .map(|e| e.scored)
                    .collect();
                RelatedStock {
                    stock_id: row.stock_id,
                    name: row.name.clone(),
                    impact: majority_impact(&scored),
                }
            })
            .collect();
        cards.push(TopicCard {
            topic_id: topic.id,
            name: topic.name.clone(),
            summary: topic.summary.clone(),
            faded: age > TOPIC_FADE_MS,
            last_article_at: last,
            shown_24h: recent.len(),
            shown_total: article_ids.len(),
            impacts,
            related,
        });
    }
    cards.sort_by(|a, b| {
        a.faded
            .cmp(&b.faded)
            .then(b.shown_24h.cmp(&a.shown_24h))
            .then(b.last_article_at.cmp(&a.last_article_at))
    });
    Ok(cards)
}

#[cfg(test)]
#[path = "read_tests.rs"]
mod tests;
