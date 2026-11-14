use rusqlite::Connection;

use rusqlite::params;

use crate::server::collect::feed_headlines::stored_headlines_since;
use crate::server::collect::r#match::create_matcher;
use crate::server::collect::run::{CollectedItem, KeepItem, load_match_stocks, save_items};
use crate::server::db::schema::ItemKind;
use crate::server::evaluate::run::EVALUATION_WINDOW_MS;
use crate::server::settings::{MARKET_RELINKED_AT, read_num, write_num};
use crate::server::time::EpochMs;

/// 보관한 제목(원문 주소·요약 포함, 3일)으로 since 이후 기사를 주어진 항목에만 다시 건다(11단계 결정).
/// 새로 잡힌 토픽의 항목과, 토픽 가지를 붙인 종목에 지난 24시간 기사까지 걸 때 쓴다.
/// 이미 걸린 기사–항목은 그대로 두고, 평가는 다음 수집 회차에서 한다.
/// 돌려주는 값은 (새 기사 수, 새 연결 수)다.
pub fn link_stored_headlines(
    conn: &mut Connection,
    stock_ids: &[i64],
    since: EpochMs,
    now: EpochMs,
) -> rusqlite::Result<(usize, usize)> {
    let mut total = (0, 0);
    if stock_ids.is_empty() {
        return Ok(total);
    }
    let watched: Vec<_> = load_match_stocks(conn)?
        .into_iter()
        .filter(|s| stock_ids.contains(&s.id))
        .collect();
    if watched.is_empty() {
        return Ok(total);
    }
    let matcher = create_matcher(&watched);
    for headline in stored_headlines_since(conn, since)? {
        let (Some(url), Some(url_key)) = (&headline.url, &headline.url_key) else {
            continue;
        };
        let summary = headline.summary.clone().unwrap_or_default();
        let matches = matcher.find(&headline.title, &summary);
        if matches.is_empty() {
            continue;
        }
        let keep = KeepItem {
            item: CollectedItem {
                url: url.clone(),
                title: headline.title.clone(),
                summary,
                publisher: headline.publisher.clone(),
                published_at: Some(headline.published_at),
                direct_stock_ids: Vec::new(),
            },
            key: url_key.clone(),
            published_at: headline.published_at,
            matches,
        };
        let saved = save_items(conn, &headline.feed, &[keep], now)?;
        total.0 += saved.0;
        total.1 += saved.1;
    }
    Ok(total)
}

/// 저장된 최근 7일 기사(평가 대기 창과 같다)를 지우지 않은 시장 항목에만 다시 건다(결정 ⑮).
/// 이미 걸린 기사–항목은 건너뛰고, AI는 부르지 않는다. 평가는 다음 수집 회차가 한다.
/// 돌려주는 값은 (확인한 기사 수, 새 연결 수)다.
pub fn relink_recent_market_articles(
    conn: &mut Connection,
    now: EpochMs,
) -> rusqlite::Result<(usize, usize)> {
    let markets: Vec<_> = load_match_stocks(conn)?
        .into_iter()
        .filter(|s| s.kind == ItemKind::Market)
        .collect();
    let articles: Vec<(i64, String, Option<String>)> = conn
        .prepare("SELECT id, title, summary FROM articles WHERE published_at >= ?1 ORDER BY id")?
        .query_map(params![now - EVALUATION_WINDOW_MS], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?))
        })?
        .collect::<rusqlite::Result<_>>()?;
    let checked = articles.len();
    let mut added = 0;
    if !markets.is_empty() {
        let matcher = create_matcher(&markets);
        let tx = conn.transaction()?;
        for (article_id, title, summary) in &articles {
            for m in matcher.find(title, summary.as_deref().unwrap_or("")) {
                added += tx.execute(
                    "INSERT INTO article_matches (article_id, stock_id, keyword_path, keyword_reason, match_kind, level, gated, created_at) \
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8) ON CONFLICT DO NOTHING",
                    params![article_id, m.stock_id, m.keyword_path, m.keyword_reason, m.match_kind.as_str(), m.level, m.gated, now],
                )?;
            }
        }
        tx.commit()?;
    }
    Ok((checked, added))
}

/// worker 시작 때 한 번만 다시 건다. 표시(`classify.marketRelinkedAt`)가 있으면 하지 않고 None을 돌려준다.
/// 기사가 0건이어도 끝나면 표시를 남긴다.
pub fn relink_market_articles_once(
    conn: &mut Connection,
    now: EpochMs,
) -> rusqlite::Result<Option<(usize, usize)>> {
    if read_num(conn, &MARKET_RELINKED_AT)? != 0.0 {
        return Ok(None);
    }
    let result = relink_recent_market_articles(conn, now)?;
    write_num(conn, &MARKET_RELINKED_AT, now as f64, now)?;
    Ok(Some(result))
}

#[cfg(test)]
#[path = "backfill_tests.rs"]
mod tests;
