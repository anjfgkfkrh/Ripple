use super::*;
use crate::server::collect::feed_headlines::{HeadlineItem, save_feed_headlines};
use crate::server::db::schema::{KeywordKind, Market};
use crate::server::keywords::generate::replace_keywords;
use crate::server::keywords::tree::KeywordNode;
use crate::server::stocks::repository::{RegisterInput, RegisterResult, register_stock};
use crate::test_support::{TestDb, ms, seed_listings, test_db};

fn now() -> EpochMs {
    ms("2026-10-22T06:00:00Z")
}

const HOUR: i64 = 60 * 60 * 1000;

fn ago(hours: i64) -> EpochMs {
    now() - hours * HOUR
}

fn stock_with_keyword(db: &mut TestDb, code: &str, keyword: &str) -> i64 {
    let registered = register_stock(
        &db.conn,
        &RegisterInput {
            market: Market::Kr,
            code,
            user_input: code,
            name_ko: None,
        },
        now(),
    )
    .unwrap();
    let RegisterResult::Created(stock) = registered else {
        panic!("표본")
    };
    replace_keywords(
        &mut db.conn,
        stock.id,
        &[KeywordNode {
            key: 1,
            parent_key: None,
            level: 1,
            kind: KeywordKind::Structure,
            text_ko: keyword.into(),
            text_en: None,
            reason: "이유".into(),
            path: format!("{} → {keyword}", stock.name),
            relevance: None,
            magnitude: None,
        }],
        now(),
    )
    .unwrap();
    stock.id
}

fn headline(title: &str, hours: i64, url: Option<&str>, summary: Option<&str>) -> HeadlineItem {
    HeadlineItem {
        title: title.into(),
        publisher: "가경제".into(),
        published_at: Some(ago(hours)),
        url: url.map(str::to_string),
        summary: summary.map(str::to_string),
    }
}

// describe("linkStoredHeadlines (11단계)")
#[test]
fn links_stored_titles_with_urls_to_given_items_only_within_period_and_is_idempotent() {
    // it("보관한 제목(원문 주소가 있는 것)을 주어진 항목에만, 기간 안의 것만 건다. 다시 해도 늘지 않는다")
    let mut db = test_db();
    seed_listings(&mut db.conn);
    let samsung = stock_with_keyword(&mut db, "005930", "HBM");
    let hynix = stock_with_keyword(&mut db, "000660", "HBM");
    save_feed_headlines(
        &mut db.conn,
        "rss:test:economy",
        &[
            headline(
                "HBM 증설 경쟁",
                2,
                Some("https://news.example/hbm"),
                Some("메모리 업계"),
            ),
            // 요약에만 키워드가 있어도 건다(제목+요약 앞 300자).
            headline(
                "메모리 업계 투자",
                3,
                Some("https://news.example/memory"),
                Some("HBM 라인을 늘린다"),
            ),
            // 주소가 없는 제목(11단계 전에 보관)은 걸 수 없다.
            headline("HBM 수출 호조", 1, None, None),
            // 기간 밖
            headline("HBM 지난 소식", 30, Some("https://news.example/old"), None),
        ],
        now(),
    )
    .unwrap();

    assert_eq!(
        link_stored_headlines(&mut db.conn, &[samsung], ago(24), now()).unwrap(),
        (2, 2)
    );
    let matches: Vec<(i64, String)> = db
        .conn
        .prepare("SELECT stock_id, keyword_path FROM article_matches ORDER BY id")
        .unwrap()
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    assert_eq!(
        matches,
        [
            (samsung, "삼성전자 → HBM".to_string()),
            (samsung, "삼성전자 → HBM".to_string())
        ]
    );
    let feeds: Vec<String> = db
        .conn
        .prepare("SELECT feed FROM articles")
        .unwrap()
        .query_map([], |r| r.get(0))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    assert_eq!(feeds, ["rss:test:economy", "rss:test:economy"]);

    // 같은 기사를 다시 걸지 않고, 다른 항목에는 기사를 새로 만들지 않고 연결만 더한다.
    assert_eq!(
        link_stored_headlines(&mut db.conn, &[samsung], ago(24), now()).unwrap(),
        (0, 0)
    );
    assert_eq!(
        link_stored_headlines(&mut db.conn, &[hynix], ago(24), now()).unwrap(),
        (0, 2)
    );
    assert_eq!(
        link_stored_headlines(&mut db.conn, &[], ago(24), now()).unwrap(),
        (0, 0)
    );
}

#[test]
fn stored_headline_links_are_gated_by_title_anchor_and_aliases_link_by_name() {
    let mut db = test_db();
    seed_listings(&mut db.conn);
    let samsung = stock_with_keyword(&mut db, "005930", "HBM");
    db.conn
        .execute(
            "UPDATE stocks SET aliases = '[\"삼전\"]', anchors = '[\"삼성\"]' WHERE id = ?1",
            [samsung],
        )
        .unwrap();
    save_feed_headlines(
        &mut db.conn,
        "rss:test:economy",
        &[
            headline("HBM 증설 경쟁", 2, Some("https://news.example/a"), None),
            headline("삼성 HBM 증설", 3, Some("https://news.example/b"), None),
            headline("삼전 반등", 4, Some("https://news.example/c"), None),
            headline(
                "메모리 투자",
                5,
                Some("https://news.example/d"),
                Some("삼성 HBM 라인"),
            ),
        ],
        now(),
    )
    .unwrap();

    assert_eq!(
        link_stored_headlines(&mut db.conn, &[samsung], ago(24), now()).unwrap(),
        (4, 4)
    );
    let rows: Vec<(String, String, i64)> = db
        .conn
        .prepare(
            "SELECT articles.title, article_matches.match_kind, article_matches.gated FROM article_matches \
             INNER JOIN articles ON articles.id = article_matches.article_id ORDER BY articles.title",
        )
        .unwrap()
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    assert_eq!(
        rows,
        [
            ("HBM 증설 경쟁".to_string(), "keyword".to_string(), 1),
            ("메모리 투자".to_string(), "keyword".to_string(), 1),
            ("삼성 HBM 증설".to_string(), "keyword".to_string(), 0),
            ("삼전 반등".to_string(), "name".to_string(), 0),
        ]
    );
}
