use rusqlite::Connection;

use crate::server::collect::feed_headlines::stored_headlines_since;
use crate::server::collect::r#match::create_matcher;
use crate::server::collect::run::{CollectedItem, KeepItem, load_match_stocks, save_items};
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
        let matches = matcher.find(&format!("{} {}", headline.title, summary));
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

#[cfg(test)]
#[path = "backfill_tests.rs"]
mod tests;
