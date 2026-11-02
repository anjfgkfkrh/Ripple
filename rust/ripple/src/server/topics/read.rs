use rusqlite::Connection;

use crate::server::db::schema::Market;
use crate::server::stocks::repository::{STOCK_COLUMNS, Stock, row_to_stock};
use crate::server::topics::store::{
    TOPIC_COLUMNS, Topic, list_user_topics, market_item, row_to_topic,
};

/// 시장 항목(한국·미국 증시)과 켜져 있으면 그 항목. web은 읽기만 한다(11단계).
#[derive(Debug, Clone, PartialEq)]
pub struct MarketItem {
    pub market: Market,
    pub name: &'static str,
    pub stock: Option<Stock>,
}

pub fn list_market_items(conn: &Connection) -> rusqlite::Result<Vec<MarketItem>> {
    let mut items = Vec::new();
    for market in Market::ALL {
        let info = market_item(*market);
        let row = conn
            .query_row(
                &format!("SELECT {STOCK_COLUMNS} FROM stocks WHERE kind = 'market' AND market = ?1 AND code = ?2"),
                rusqlite::params![market.as_str(), info.code],
                row_to_stock,
            )
            .map(Some)
            .or_else(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => Ok(None),
                other => Err(other),
            })?;
        items.push(MarketItem {
            market: *market,
            name: info.name,
            stock: row.filter(|r| r.deleted_at.is_none()),
        });
    }
    Ok(items)
}

/// 사용자 주제와 켜진 증시마다 하나씩인 항목(한국 먼저)
#[derive(Debug, Clone, PartialEq)]
pub struct UserTopicItem {
    pub topic: Topic,
    pub lenses: Vec<Stock>,
}

pub fn list_user_topic_items(conn: &Connection) -> rusqlite::Result<Vec<UserTopicItem>> {
    let list = list_user_topics(conn)?;
    if list.is_empty() {
        return Ok(Vec::new());
    }
    let marks = vec!["?"; list.len()].join(",");
    let mut stmt = conn.prepare(&format!(
        "SELECT {STOCK_COLUMNS} FROM stocks WHERE kind = 'topic' AND topic_id IN ({marks}) AND deleted_at IS NULL \
         ORDER BY market ASC, id ASC"
    ))?;
    let lenses: Vec<Stock> = stmt
        .query_map(
            rusqlite::params_from_iter(list.iter().map(|t| t.id)),
            row_to_stock,
        )?
        .collect::<rusqlite::Result<_>>()?;
    Ok(list
        .into_iter()
        .map(|topic| UserTopicItem {
            lenses: lenses
                .iter()
                .filter(|l| l.topic_id == Some(topic.id))
                .cloned()
                .collect(),
            topic,
        })
        .collect())
}

/// 주소의 topic 값으로 받을 수 있는 토픽: 지우지 않은 사용자 주제와 자동 토픽(끝난 것 포함)
pub fn filterable_topic_ids(conn: &Connection) -> rusqlite::Result<Vec<i64>> {
    let mut stmt = conn.prepare("SELECT id FROM topics WHERE deleted_at IS NULL")?;
    stmt.query_map([], |r| r.get(0))?.collect()
}

/// 토픽 이름(필터 안내용)
pub fn topic_name(conn: &Connection, topic_id: i64) -> rusqlite::Result<Option<String>> {
    match conn.query_row(
        &format!("SELECT {TOPIC_COLUMNS} FROM topics WHERE id = ?1"),
        rusqlite::params![topic_id],
        row_to_topic,
    ) {
        Ok(topic) => Ok(Some(topic.name)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(e),
    }
}
