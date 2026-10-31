use super::*;
use crate::test_support::{TestDb, ms, test_db};

fn now() -> EpochMs {
    ms("2026-10-22T01:00:00Z")
}

const HOUR: i64 = 60 * 60 * 1000;

fn ago(hours: i64) -> EpochMs {
    now() - hours * HOUR
}

fn item(title: &str, publisher: &str, published_at: Option<EpochMs>) -> HeadlineItem {
    HeadlineItem {
        title: title.into(),
        publisher: publisher.into(),
        published_at,
        url: None,
        summary: None,
    }
}

fn rows(db: &TestDb) -> Vec<StoredHeadline> {
    let mut stmt = db
        .conn
        .prepare("SELECT id, title_key, title, publisher, feed, published_at, seen_at, url, url_key, summary FROM feed_headlines ORDER BY id ASC")
        .unwrap();
    stmt.query_map([], row_to_headline)
        .unwrap()
        .map(Result::unwrap)
        .collect()
}

// describe("saveFeedHeadlines (10단계)")
#[test]
fn keeps_title_publisher_feed_time_and_dedupes_by_key_ignoring_space_and_case() {
    // it("제목·언론사·피드·시각을 남기고, 같은 제목(공백·대소문자 무시)은 하나")
    let mut db = test_db();
    let added = save_feed_headlines(
        &mut db.conn,
        "rss:sbs:international",
        &[
            item(" 트럼프,  중동 순방 ", "SBS", Some(ago(1))),
            item("트럼프, 중동 순방", "SBS", Some(ago(2))),
            item("Fed Holds Rates", "SBS", Some(ago(1))),
            item("fed holds rates", "SBS", Some(ago(1))),
            item("   ", "SBS", Some(ago(1))),
        ],
        now(),
    )
    .unwrap();
    assert_eq!(added, 2);
    let saved = rows(&db);
    assert_eq!(
        saved[0],
        StoredHeadline {
            id: 1,
            title_key: "트럼프,중동순방".into(),
            title: "트럼프, 중동 순방".into(),
            publisher: "SBS".into(),
            feed: "rss:sbs:international".into(),
            published_at: ago(1),
            seen_at: now(),
            url: None,
            url_key: None,
            summary: None,
        }
    );
    assert_eq!(saved[1].title, "Fed Holds Rates");

    // 다음 회차에 같은 제목이 또 와도 늘지 않는다.
    let again = save_feed_headlines(
        &mut db.conn,
        "rss:other:all",
        &[item("트럼프, 중동 순방", "다른 곳", Some(ago(0)))],
        now(),
    )
    .unwrap();
    assert_eq!(again, 0);
    assert_eq!(rows(&db).len(), 2);
}

#[test]
fn missing_time_is_seen_time_old_items_are_skipped_and_long_titles_are_cut() {
    // it("발행 시각이 없으면 받은 시각, 3일보다 오래된 기사는 넣지 않고, 긴 제목은 자른다")
    let mut db = test_db();
    save_feed_headlines(
        &mut db.conn,
        "rss:test:all",
        &[
            item("시각 없음", "p", None),
            item("오래된 기사", "p", Some(ago(73))),
            item(&"가".repeat(400), "p", Some(ago(1))),
        ],
        now(),
    )
    .unwrap();
    let summary: Vec<(usize, EpochMs)> = rows(&db)
        .iter()
        .map(|r| (r.title.chars().count(), r.published_at))
        .collect();
    assert_eq!(summary, [(5, now()), (300, ago(1))]);
}

#[test]
fn keeps_url_with_dedupe_key_and_first_300_chars_of_summary() {
    // it("원문 주소(중복 판정용 주소 포함)와 요약 앞 300자도 남긴다(11단계)")
    let mut db = test_db();
    save_feed_headlines(
        &mut db.conn,
        "rss:test:all",
        &[
            HeadlineItem {
                title: "주소 있는 기사".into(),
                publisher: "p".into(),
                published_at: Some(ago(1)),
                url: Some(" https://news.example/a?utm_source=x ".into()),
                summary: Some(format!("  요약  {}", "가".repeat(400))),
            },
            item("주소 없는 기사", "p", Some(ago(1))),
        ],
        now(),
    )
    .unwrap();
    let saved = rows(&db);
    assert_eq!(
        saved[0].url.as_deref(),
        Some("https://news.example/a?utm_source=x")
    );
    assert_eq!(saved[0].url_key.as_deref(), Some("news.example/a"));
    let summary = saved[0].summary.clone().unwrap();
    assert_eq!(summary.chars().count(), 300);
    assert!(summary.starts_with("요약 가"));
    // 다시 걸 때는 주소가 있는 제목만 쓴다.
    let titles: Vec<String> = stored_headlines_since(&db.conn, ago(24))
        .unwrap()
        .into_iter()
        .map(|h| h.title)
        .collect();
    assert_eq!(titles, ["주소 있는 기사"]);
}

#[test]
fn publish_time_more_than_ten_minutes_ahead_becomes_seen_time() {
    // it("받은 시각보다 10분 넘게 미래인 발행 시각은 받은 시각으로 둔다(2026-10-23 결정)")
    let mut db = test_db();
    save_feed_headlines(
        &mut db.conn,
        "rss:kmib:all",
        &[item("지면 기사", "국민일보", Some(ago(-19)))],
        now(),
    )
    .unwrap();
    assert_eq!(rows(&db)[0].published_at, now());
}

#[test]
fn headline_key_removes_all_whitespace_and_lowercases() {
    // it("headlineKey는 공백을 모두 빼고 소문자로")
    assert_eq!(headline_key(" A b\tC "), "abc");
}

// describe("pruneFeedHeadlines·recentFeedTitles")
#[test]
fn prunes_titles_older_than_three_days_and_lists_recent_ones_newest_first_up_to_limit() {
    // it("3일이 지난 제목을 지우고, 기간 안 제목을 최근순으로 limit개까지 준다")
    let mut db = test_db();
    let at = now() - HEADLINE_KEEP_MS;
    save_feed_headlines(
        &mut db.conn,
        "rss:test:all",
        &[
            item("가장 최근", "p", Some(ago(1))),
            item("두 번째", "p", Some(ago(2))),
            item("어제", "p", Some(ago(25))),
            item("사흘 전", "p", Some(ago(71))),
        ],
        now(),
    )
    .unwrap();
    // 받을 때는 3일 안이었지만 시간이 지나 3일을 넘긴 제목
    save_feed_headlines(
        &mut db.conn,
        "rss:test:all",
        &[item("지난 제목", "p", Some(at - HOUR))],
        at - HOUR,
    )
    .unwrap();

    assert_eq!(prune_feed_headlines(&db.conn, now()).unwrap(), 1);
    assert_eq!(
        recent_feed_titles(&db.conn, ago(24), 10).unwrap(),
        ["가장 최근", "두 번째"]
    );
    assert_eq!(
        recent_feed_titles(&db.conn, ago(100), 3).unwrap(),
        ["가장 최근", "두 번째", "어제"]
    );
}
