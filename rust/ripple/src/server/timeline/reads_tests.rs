use super::*;
use crate::server::db::schema::Market;
use crate::server::stocks::repository::{RegisterInput, RegisterResult, register_stock};
use crate::test_support::{LinkSpec, SharedTestDb, insert_link, ms, seed_listings, shared_test_db};

/// 기사 n개가 걸린 종목 하나를 가진 시험 DB
fn seed(count: usize) -> (SharedTestDb, Vec<i64>) {
    let shared = shared_test_db();
    let article_ids = {
        let mut conn = shared.db.lock().unwrap();
        seed_listings(&mut conn);
        let result = register_stock(
            &conn,
            &RegisterInput {
                market: Market::Kr,
                code: "005930",
                user_input: "삼성전자",
                name_ko: None,
            },
            ms("2026-10-21T00:00:00Z"),
        )
        .unwrap();
        let RegisterResult::Created(stock) = result else {
            panic!("표본에 없음");
        };
        (0..count)
            .map(|i| {
                // TS의 insertLink는 점수를 주지 않으면 평가를 넣지 않는다.
                let mut link = LinkSpec::new(
                    stock.id,
                    "기사",
                    ms("2026-10-21T00:00:00Z") + i as i64 * 60_000,
                );
                link.relevance = None;
                insert_link(&conn, &link).1
            })
            .collect::<Vec<_>>()
    };
    (shared, article_ids)
}

fn all_reads(conn: &Connection) -> Vec<(i64, i64)> {
    let mut stmt = conn
        .prepare("SELECT article_id, read_at FROM article_reads ORDER BY article_id")
        .unwrap();
    stmt.query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
        .unwrap()
        .map(Result::unwrap)
        .collect()
}

// describe("setArticlesRead (12단계)")
#[test]
fn saves_read_with_time_and_keeps_first_time_on_repeat() {
    // it("읽음으로 저장하고 읽은 시각을 남기며, 다시 읽음으로 해도 처음 시각을 지킨다")
    let (shared, ids) = seed(2);
    let conn = shared.db.lock().unwrap();
    let first = ms("2026-10-28T01:00:00Z");
    assert_eq!(set_articles_read(&conn, &[ids[0]], true, first).unwrap(), 1);
    assert_eq!(
        set_articles_read(&conn, &[ids[0]], true, ms("2026-11-01T00:00:00Z")).unwrap(),
        0
    );
    assert_eq!(all_reads(&conn), vec![(ids[0], first)]);
    assert_eq!(
        read_article_ids(&conn, &ids).unwrap(),
        HashSet::from([ids[0]])
    );
}

#[test]
fn unmarking_deletes_and_leaves_unread_alone() {
    // it("안 읽음으로 되돌리면 지우고, 읽지 않은 기사는 그대로 둔다")
    let (shared, ids) = seed(3);
    let conn = shared.db.lock().unwrap();
    let now = ms("2026-10-28T01:00:00Z");
    set_articles_read(&conn, &ids, true, now).unwrap();
    assert_eq!(
        set_articles_read(&conn, &[ids[0], ids[1]], false, now).unwrap(),
        2
    );
    assert_eq!(
        read_article_ids(&conn, &ids).unwrap(),
        HashSet::from([ids[2]])
    );
    assert_eq!(set_articles_read(&conn, &[ids[0]], false, now).unwrap(), 0);
}

#[test]
fn dedupes_ids_and_skips_missing_articles() {
    // it("같은 id가 겹쳐도 한 번만 저장하고, 없는 기사 id는 건너뛴다")
    let (shared, ids) = seed(1);
    let conn = shared.db.lock().unwrap();
    let now = ms("2026-10-28T01:00:00Z");
    assert_eq!(
        set_articles_read(&conn, &[ids[0], ids[0], 9999], true, now).unwrap(),
        1
    );
    assert_eq!(set_articles_read(&conn, &[9999], true, now).unwrap(), 0);
    assert_eq!(set_articles_read(&conn, &[], true, now).unwrap(), 0);
    assert_eq!(all_reads(&conn).len(), 1);
}

#[test]
fn deleting_article_removes_read_record() {
    // it("기사를 지우면 읽음 기록도 함께 지워진다")
    let (shared, ids) = seed(1);
    let conn = shared.db.lock().unwrap();
    set_articles_read(&conn, &ids, true, ms("2026-10-28T01:00:00Z")).unwrap();
    conn.execute("DELETE FROM article_matches", []).unwrap();
    conn.execute("DELETE FROM evaluations", []).unwrap();
    conn.execute("DELETE FROM articles", []).unwrap();
    assert_eq!(all_reads(&conn), vec![]);
}
