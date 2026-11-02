use std::collections::{HashMap, HashSet};
use std::sync::LazyLock;

use regex::Regex;
use rusqlite::{Connection, OptionalExtension, Row, params};

use crate::server::db::client::atomically;
use crate::server::db::schema::{KeywordStatus, Market, TopicSource, TopicStatus};
use crate::server::keywords::tree::PATH_SEPARATOR;
use crate::server::stocks::repository::{
    NEW_STOCK_BACKFILL_MS, STOCK_COLUMNS, Stock, row_to_stock,
};
use crate::server::text::{js_len, js_slice, js_trim};
use crate::server::time::EpochMs;

/// 시장 항목(한국·미국 증시)·주제(사용자 주제·자동 토픽)를 stocks 표에 둔다(11단계 결정: 같은 표, 종류로 구분).
/// 주제는 켜진 증시마다 항목을 하나씩 두고 그 증시 기준으로 평가한다(둘 다 켜져 있으면 둘 다).
pub struct MarketItemInfo {
    pub code: &'static str,
    pub name: &'static str,
    pub exchange: &'static str,
}

pub fn market_item(market: Market) -> MarketItemInfo {
    match market {
        Market::Kr => MarketItemInfo {
            code: "MARKET",
            name: "한국 증시",
            exchange: "KOSPI·KOSDAQ",
        },
        Market::Us => MarketItemInfo {
            code: "MARKET",
            name: "미국 증시",
            exchange: "S&P 500·NASDAQ",
        },
    }
}

const TOPIC_NAME_MAX: usize = 20;
const MAX_TERM_LENGTH: usize = 40;
const TOPIC_REASON: &str = "토픽 검색어";

/// 새로 켜거나 되살린 시장 항목·사용자 주제 항목은 종목 등록과 같이(2026-10-21 결정) 트리를 만든 뒤
/// 다음 수집 회차에서 지난 24시간 기사까지 건다. 자동 토픽 항목은 보관한 제목으로 바로 건다(11단계 결정).
fn backfill_from(now: EpochMs) -> EpochMs {
    now - NEW_STOCK_BACKFILL_MS
}

static WHITESPACE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"[\t\n\x0B\x0C\r \x{A0}\x{1680}\x{2000}-\x{200A}\x{2028}\x{2029}\x{202F}\x{205F}\x{3000}\x{FEFF}]+")
        .expect("정규식이 올바르다")
});

fn collapse(text: &str) -> String {
    js_trim(&WHITESPACE.replace_all(text, " ")).to_string()
}

fn name_key(text: &str) -> String {
    WHITESPACE.replace_all(text, "").to_lowercase()
}

/// `topics` 표의 한 줄
#[derive(Debug, Clone, PartialEq)]
pub struct Topic {
    pub id: i64,
    pub source: TopicSource,
    pub name: String,
    pub summary: Option<String>,
    pub terms: Vec<String>,
    pub status: TopicStatus,
    pub created_at: EpochMs,
    pub last_seen_at: Option<EpochMs>,
    pub ended_at: Option<EpochMs>,
    pub deleted_at: Option<EpochMs>,
}

pub const TOPIC_COLUMNS: &str =
    "id, source, name, summary, terms, status, created_at, last_seen_at, ended_at, deleted_at";

pub fn row_to_topic(row: &Row<'_>) -> rusqlite::Result<Topic> {
    let bad = |index: usize, message: String| {
        rusqlite::Error::FromSqlConversionFailure(
            index,
            rusqlite::types::Type::Text,
            message.into(),
        )
    };
    let source: String = row.get(1)?;
    let terms: String = row.get(4)?;
    let status: String = row.get(5)?;
    Ok(Topic {
        id: row.get(0)?,
        source: source.parse().map_err(|e: String| bad(1, e))?,
        name: row.get(2)?,
        summary: row.get(3)?,
        terms: serde_json::from_str(&terms).map_err(|e| bad(4, e.to_string()))?,
        status: status.parse().map_err(|e: String| bad(5, e))?,
        created_at: row.get(6)?,
        last_seen_at: row.get(7)?,
        ended_at: row.get(8)?,
        deleted_at: row.get(9)?,
    })
}

pub fn find_topic(conn: &Connection, id: i64) -> rusqlite::Result<Option<Topic>> {
    conn.query_row(
        &format!("SELECT {TOPIC_COLUMNS} FROM topics WHERE id = ?1"),
        params![id],
        row_to_topic,
    )
    .optional()
}

fn market_row(conn: &Connection, market: Market) -> rusqlite::Result<Option<Stock>> {
    conn.query_row(
        &format!(
            "SELECT {STOCK_COLUMNS} FROM stocks WHERE kind = 'market' AND market = ?1 AND code = ?2"
        ),
        params![market.as_str(), market_item(market).code],
        row_to_stock,
    )
    .optional()
}

/// 켜진 증시
pub fn enabled_markets(conn: &Connection) -> rusqlite::Result<Vec<Market>> {
    let mut enabled = Vec::new();
    for market in Market::ALL {
        if market_row(conn, *market)?.is_some_and(|row| row.deleted_at.is_none()) {
            enabled.push(*market);
        }
    }
    Ok(enabled)
}

/// 한국 증시는 켠 채로 시작한다(11단계 결정). 한 번도 만든 적이 없을 때만 만들고(사용자가 끈 것은 그대로),
/// 새로 만들었으면 키워드를 만들 항목 id를 돌려준다.
pub fn ensure_default_market(conn: &Connection, now: EpochMs) -> rusqlite::Result<Option<i64>> {
    if market_row(conn, Market::Kr)?.is_some() {
        return Ok(None);
    }
    create_market_row(conn, Market::Kr, now).map(Some)
}

fn create_market_row(conn: &Connection, market: Market, now: EpochMs) -> rusqlite::Result<i64> {
    let item = market_item(market);
    conn.execute(
        "INSERT INTO stocks (market, code, name, name_ko, exchange, user_input, kind, keywords_status, backfill_from, created_at) \
         VALUES (?1, ?2, ?3, ?3, ?4, ?3, 'market', 'pending', ?5, ?6)",
        params![market.as_str(), item.code, item.name, item.exchange, backfill_from(now), now],
    )?;
    Ok(conn.last_insert_rowid())
}

/// 증시를 켜거나 끈다: 시장 항목을 만들거나 되살리거나 숨기고, 모든 주제·토픽의 항목을 켜진 증시에 맞춘다.
/// 키워드를 새로 만들 항목(시장 항목, 사용자 주제 항목)과 지난 기사를 걸 항목(자동 토픽 항목)을 돌려준다.
pub fn set_market_enabled(
    conn: &Connection,
    market: Market,
    enabled: bool,
    now: EpochMs,
) -> rusqlite::Result<(Vec<i64>, Vec<i64>)> {
    atomically(conn, || {
        let (mut generate, mut created) = (Vec::new(), Vec::new());
        let row = market_row(conn, market)?;
        if enabled {
            match row {
                None => generate.push(create_market_row(conn, market, now)?),
                Some(row) if row.deleted_at.is_some() => {
                    conn.execute(
                        "UPDATE stocks SET deleted_at = NULL, keywords_status = 'pending', backfill_from = ?1 WHERE id = ?2",
                        params![backfill_from(now), row.id],
                    )?;
                    generate.push(row.id);
                }
                Some(_) => {}
            }
        } else if let Some(row) = row.filter(|r| r.deleted_at.is_none()) {
            conn.execute(
                "UPDATE stocks SET deleted_at = ?1 WHERE id = ?2",
                params![now, row.id],
            )?;
        }
        for topic in live_topics(conn)? {
            for lens in sync_topic_lenses(conn, &topic, now)? {
                if topic.source == TopicSource::User {
                    generate.push(lens);
                } else {
                    created.push(lens);
                }
            }
        }
        Ok((generate, created))
    })
}

/// 지우지 않은 토픽(자동 토픽은 끝난 것도 포함, 3일 동안 화면에 남으므로)
fn live_topics(conn: &Connection) -> rusqlite::Result<Vec<Topic>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {TOPIC_COLUMNS} FROM topics WHERE deleted_at IS NULL ORDER BY id ASC"
    ))?;
    stmt.query_map([], row_to_topic)?.collect()
}

/// 토픽의 항목(증시별)
pub fn topic_lenses(conn: &Connection, topic_id: i64) -> rusqlite::Result<Vec<Stock>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {STOCK_COLUMNS} FROM stocks WHERE kind = 'topic' AND topic_id = ?1"
    ))?;
    stmt.query_map(params![topic_id], row_to_stock)?.collect()
}

/// 토픽의 항목을 켜진 증시에 맞춘다: 없는 것은 만들고 숨긴 것은 되살리며, 꺼진 증시의 것은 숨긴다.
/// 새로 만들었거나 되살린 항목 id를 돌려준다. 자동 토픽 항목은 검색어 키워드도 맞춘다.
pub fn sync_topic_lenses(
    conn: &Connection,
    topic: &Topic,
    now: EpochMs,
) -> rusqlite::Result<Vec<i64>> {
    let markets: HashSet<Market> = enabled_markets(conn)?.into_iter().collect();
    let lenses = topic_lenses(conn, topic.id)?;
    let mut added = Vec::new();
    let is_auto = topic.source == TopicSource::Auto;
    for market in Market::ALL {
        let lens = lenses.iter().find(|row| row.market == *market);
        if markets.contains(market) {
            match lens {
                None => {
                    conn.execute(
                        "INSERT INTO stocks (market, code, name, name_ko, exchange, user_input, kind, topic_id, keywords_status, \
                         keywords_generated_at, backfill_from, created_at) \
                         VALUES (?1, ?2, ?3, ?3, ?4, ?3, 'topic', ?5, ?6, ?7, ?8, ?9)",
                        params![
                            market.as_str(),
                            format!("TOPIC-{}", topic.id),
                            topic.name,
                            market_item(*market).name,
                            topic.id,
                            if is_auto { KeywordStatus::Ready } else { KeywordStatus::Pending }.as_str(),
                            if is_auto { Some(now) } else { None },
                            if is_auto { None } else { Some(backfill_from(now)) },
                            now
                        ],
                    )?;
                    let id = conn.last_insert_rowid();
                    added.push(id);
                    if is_auto {
                        set_auto_topic_terms(conn, id, topic, now)?;
                    }
                }
                Some(lens) if lens.deleted_at.is_some() => {
                    if topic.source == TopicSource::User {
                        conn.execute(
                            "UPDATE stocks SET deleted_at = NULL, keywords_status = 'pending', backfill_from = ?1 WHERE id = ?2",
                            params![backfill_from(now), lens.id],
                        )?;
                    } else {
                        conn.execute(
                            "UPDATE stocks SET deleted_at = NULL WHERE id = ?1",
                            params![lens.id],
                        )?;
                    }
                    added.push(lens.id);
                    if is_auto {
                        set_auto_topic_terms(conn, lens.id, topic, now)?;
                    }
                }
                Some(_) => {}
            }
        } else if let Some(lens) = lens.filter(|l| l.deleted_at.is_none()) {
            conn.execute(
                "UPDATE stocks SET deleted_at = ?1 WHERE id = ?2",
                params![now, lens.id],
            )?;
        }
    }
    Ok(added)
}

/// 자동 토픽 항목의 키워드 = 토픽 검색어(1단계, 시사). 토픽 갱신 때마다 바꾼다.
pub fn set_auto_topic_terms(
    conn: &Connection,
    lens_id: i64,
    topic: &Topic,
    now: EpochMs,
) -> rusqlite::Result<()> {
    atomically(conn, || {
        conn.execute("DELETE FROM keywords WHERE stock_id = ?1", params![lens_id])?;
        for term in &topic.terms {
            let text = js_slice(&collapse(term), 0, MAX_TERM_LENGTH);
            if text.is_empty() {
                continue;
            }
            conn.execute(
                "INSERT INTO keywords (stock_id, level, text_ko, reason, path, kind, created_at) \
                 VALUES (?1, 1, ?2, ?3, ?4, 'current', ?5)",
                params![
                    lens_id,
                    text,
                    topic.summary.as_deref().unwrap_or(TOPIC_REASON),
                    format!("{}{PATH_SEPARATOR}{text}", topic.name),
                    now
                ],
            )?;
        }
        conn.execute(
            "UPDATE stocks SET name = ?1, name_ko = ?1, keywords_generated_at = ?2 WHERE id = ?3",
            params![topic.name, now, lens_id],
        )?;
        Ok(())
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AddTopicResult {
    Added { topic_id: i64, lens_ids: Vec<i64> },
    Invalid,
    Exists,
}

/// 사용자 주제를 넣는다(적은 그대로 2~20자, 11단계 결정). 키워드를 만들 항목 id를 돌려준다.
pub fn add_user_topic(
    conn: &Connection,
    input: &str,
    now: EpochMs,
) -> rusqlite::Result<AddTopicResult> {
    let name = collapse(input);
    let length = js_len(&name);
    if !(2..=TOPIC_NAME_MAX).contains(&length) {
        return Ok(AddTopicResult::Invalid);
    }
    let key = name_key(&name);
    let mut stmt =
        conn.prepare("SELECT name FROM topics WHERE source = 'user' AND deleted_at IS NULL")?;
    let exists = stmt
        .query_map([], |r| r.get::<_, String>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?
        .iter()
        .any(|existing| name_key(existing) == key);
    if exists {
        return Ok(AddTopicResult::Exists);
    }
    atomically(conn, || {
        conn.execute(
            "INSERT INTO topics (source, name, created_at) VALUES ('user', ?1, ?2)",
            params![name, now],
        )?;
        let topic = find_topic(conn, conn.last_insert_rowid())?.expect("방금 넣은 줄이 있다");
        Ok(AddTopicResult::Added {
            topic_id: topic.id,
            lens_ids: sync_topic_lenses(conn, &topic, now)?,
        })
    })
}

/// 사용자 주제를 지운다(항목은 숨기고 보존, D-11과 같은 방식)
pub fn delete_user_topic(conn: &Connection, topic_id: i64, now: EpochMs) -> rusqlite::Result<bool> {
    let topic: Option<Topic> = conn
        .query_row(
            &format!("SELECT {TOPIC_COLUMNS} FROM topics WHERE id = ?1 AND source = 'user'"),
            params![topic_id],
            row_to_topic,
        )
        .optional()?;
    let Some(topic) = topic.filter(|t| t.deleted_at.is_none()) else {
        return Ok(false);
    };
    atomically(conn, || {
        conn.execute(
            "UPDATE topics SET deleted_at = ?1 WHERE id = ?2",
            params![now, topic.id],
        )?;
        conn.execute(
            "UPDATE stocks SET deleted_at = ?1 WHERE topic_id = ?2 AND deleted_at IS NULL",
            params![now, topic.id],
        )?;
        Ok(())
    })?;
    Ok(true)
}

/// 사용자 주제 목록(지우지 않은 것)
pub fn list_user_topics(conn: &Connection) -> rusqlite::Result<Vec<Topic>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {TOPIC_COLUMNS} FROM topics WHERE source = 'user' AND deleted_at IS NULL ORDER BY id ASC"
    ))?;
    stmt.query_map([], row_to_topic)?.collect()
}

/// 토픽 가지가 붙일 종목의 이름(`Pick<Stock, "id" | "name" | "nameKo">`)
pub struct StockRef<'a> {
    pub id: i64,
    pub name: &'a str,
    pub name_ko: Option<&'a str>,
}

impl<'a> From<&'a Stock> for StockRef<'a> {
    fn from(stock: &'a Stock) -> Self {
        Self {
            id: stock.id,
            name: &stock.name,
            name_ko: stock.name_ko.as_deref(),
        }
    }
}

pub struct TopicLink<'a> {
    pub reason: &'a str,
    pub relevance: f64,
    pub magnitude: f64,
}

/// 종목에 토픽 가지를 붙이거나 고친다(11단계): 1단계에 토픽 이름(이유·Jev 점수), 그 아래에 토픽 검색어.
/// 트리를 다시 만들어도 남고(topic_id), 토픽 갱신이 뗀다. 새로 붙였으면 true.
pub fn attach_topic_to_stock(
    conn: &Connection,
    stock: &StockRef<'_>,
    topic: &Topic,
    link: &TopicLink<'_>,
    now: EpochMs,
) -> rusqlite::Result<bool> {
    let display = stock.name_ko.unwrap_or(stock.name);
    let had: bool = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM keywords WHERE stock_id = ?1 AND topic_id = ?2)",
        params![stock.id, topic.id],
        |r| r.get(0),
    )?;
    atomically(conn, || {
        conn.execute(
            "DELETE FROM keywords WHERE stock_id = ?1 AND topic_id = ?2",
            params![stock.id, topic.id],
        )?;
        let theme_path = format!("{display}{PATH_SEPARATOR}{}", topic.name);
        conn.execute(
            "INSERT INTO keywords (stock_id, level, text_ko, reason, path, kind, relevance, magnitude, topic_id, created_at) \
             VALUES (?1, 1, ?2, ?3, ?4, 'current', ?5, ?6, ?7, ?8)",
            params![
                stock.id,
                js_slice(&topic.name, 0, MAX_TERM_LENGTH),
                link.reason,
                theme_path,
                link.relevance,
                link.magnitude,
                topic.id,
                now
            ],
        )?;
        let theme_id = conn.last_insert_rowid();
        for term in &topic.terms {
            let text = js_slice(&collapse(term), 0, MAX_TERM_LENGTH);
            if text.is_empty() {
                continue;
            }
            conn.execute(
                "INSERT INTO keywords (stock_id, level, parent_id, text_ko, reason, path, kind, topic_id, created_at) \
                 VALUES (?1, 2, ?2, ?3, ?4, ?5, 'current', ?6, ?7)",
                params![stock.id, theme_id, text, TOPIC_REASON, format!("{theme_path}{PATH_SEPARATOR}{text}"), topic.id, now],
            )?;
        }
        Ok(())
    })?;
    Ok(!had)
}

/// 종목에서 토픽 가지를 뗀다. 떼었으면 true
pub fn detach_topic_from_stock(
    conn: &Connection,
    stock_id: i64,
    topic_id: i64,
) -> rusqlite::Result<bool> {
    Ok(conn.execute(
        "DELETE FROM keywords WHERE stock_id = ?1 AND topic_id = ?2",
        params![stock_id, topic_id],
    )? > 0)
}

/// 지금 종목마다 붙어 있는 토픽
pub fn attached_topics_by_stock(conn: &Connection) -> rusqlite::Result<HashMap<i64, HashSet<i64>>> {
    let mut stmt = conn.prepare(
        "SELECT keywords.stock_id, keywords.topic_id FROM keywords INNER JOIN stocks ON stocks.id = keywords.stock_id \
         WHERE keywords.topic_id IS NOT NULL AND stocks.kind = 'stock'",
    )?;
    let rows = stmt.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?)))?;
    let mut map: HashMap<i64, HashSet<i64>> = HashMap::new();
    for row in rows {
        let (stock_id, topic_id) = row?;
        map.entry(stock_id).or_default().insert(topic_id);
    }
    Ok(map)
}

/// 활성 종목(종목 종류만): 토픽을 붙일 후보
pub fn active_stocks(conn: &Connection) -> rusqlite::Result<Vec<Stock>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {STOCK_COLUMNS} FROM stocks WHERE kind = 'stock' AND deleted_at IS NULL ORDER BY id ASC"
    ))?;
    stmt.query_map([], row_to_stock)?.collect()
}

/// 자동 토픽(진행 중인 것)
pub fn active_auto_topics(conn: &Connection) -> rusqlite::Result<Vec<Topic>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {TOPIC_COLUMNS} FROM topics WHERE source = 'auto' AND status = 'active' AND deleted_at IS NULL ORDER BY id ASC"
    ))?;
    stmt.query_map([], row_to_topic)?.collect()
}

/// 끝난 지 `since` 이후인 자동 토픽(같은 주제가 다시 나오면 되살릴 후보)
pub fn recently_ended_auto_topics(
    conn: &Connection,
    since: EpochMs,
) -> rusqlite::Result<Vec<Topic>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {TOPIC_COLUMNS} FROM topics WHERE source = 'auto' AND status = 'ended' AND deleted_at IS NULL \
         AND ended_at >= ?1 ORDER BY id ASC"
    ))?;
    stmt.query_map(params![since], row_to_topic)?.collect()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct TopicArticleCounts {
    /// `since` 이후 발행된 기사 가운데 이 토픽의 항목에 걸린 수(화면 기준 통과 여부와 무관)
    pub matched: i64,
    /// 그 가운데 관련도·영향도가 기준 이상인(타임라인에 나오는) 수. 영향도 없는 옛 평가는 관련도만 본다
    pub shown: i64,
}

/// 토픽마다 `since` 이후 걸린 기사 수. 기사가 없는 토픽도 0으로 채운다.
pub fn topic_article_counts(
    conn: &Connection,
    topic_ids: &[i64],
    since: EpochMs,
    min_relevance: f64,
    min_magnitude: f64,
) -> rusqlite::Result<HashMap<i64, TopicArticleCounts>> {
    let mut counts: HashMap<i64, TopicArticleCounts> = topic_ids
        .iter()
        .map(|id| (*id, TopicArticleCounts::default()))
        .collect();
    if topic_ids.is_empty() {
        return Ok(counts);
    }
    let marks = vec!["?"; topic_ids.len()].join(",");
    let mut stmt = conn.prepare(&format!(
        "SELECT stocks.topic_id, count(distinct article_matches.article_id), \
         count(distinct case when evaluations.relevance >= ? and (evaluations.magnitude is null or evaluations.magnitude >= ?) \
              then article_matches.article_id end) \
         FROM article_matches \
         INNER JOIN articles ON articles.id = article_matches.article_id \
         INNER JOIN stocks ON stocks.id = article_matches.stock_id \
         LEFT JOIN evaluations ON evaluations.match_id = article_matches.id \
         WHERE stocks.kind = 'topic' AND stocks.topic_id IN ({marks}) AND articles.published_at >= ? \
         GROUP BY stocks.topic_id"
    ))?;
    let mut bindings: Vec<rusqlite::types::Value> =
        vec![min_relevance.into(), min_magnitude.into()];
    bindings.extend(topic_ids.iter().map(|id| rusqlite::types::Value::from(*id)));
    bindings.push(since.into());
    let rows = stmt.query_map(rusqlite::params_from_iter(bindings), |r| {
        Ok((
            r.get::<_, Option<i64>>(0)?,
            r.get::<_, i64>(1)?,
            r.get::<_, i64>(2)?,
        ))
    })?;
    for row in rows {
        let (topic_id, matched, shown) = row?;
        if let Some(topic_id) = topic_id {
            counts.insert(topic_id, TopicArticleCounts { matched, shown });
        }
    }
    Ok(counts)
}

/// 이 토픽들의 항목 id
pub fn lens_ids_of(conn: &Connection, topic_ids: &[i64]) -> rusqlite::Result<Vec<i64>> {
    if topic_ids.is_empty() {
        return Ok(Vec::new());
    }
    let marks = vec!["?"; topic_ids.len()].join(",");
    let mut stmt = conn.prepare(&format!(
        "SELECT id FROM stocks WHERE kind = 'topic' AND topic_id IN ({marks}) AND deleted_at IS NULL"
    ))?;
    stmt.query_map(rusqlite::params_from_iter(topic_ids), |r| r.get(0))?
        .collect()
}
