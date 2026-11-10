use super::*;
use crate::server::stocks::repository::{RegisterInput, RegisterResult, register_stock};
use crate::test_support::{
    LinkSpec, TestDb, insert_link, ms, seed_listings, set_link_path, test_db,
};

fn now() -> EpochMs {
    ms("2026-10-21T03:00:00Z")
}

const DAY: i64 = 24 * 60 * 60 * 1000;

fn testdb() -> (TestDb, i64, i64) {
    let mut db = test_db();
    seed_listings(&mut db.conn);
    let mut ids = Vec::new();
    for code in ["005930", "000660"] {
        match register_stock(
            &db.conn,
            &RegisterInput {
                market: crate::server::db::schema::Market::Kr,
                code,
                user_input: code,
                name_ko: None,
            },
            now(),
        )
        .unwrap()
        {
            RegisterResult::Created(stock) => ids.push(stock.id),
            other => panic!("{other:?}"),
        }
    }
    (db, ids[0], ids[1])
}

/// 키워드 연결 하나(경로, 관련도·영향도, 만든 시각, 걸러짐)
fn link(
    db: &TestDb,
    stock_id: i64,
    path: &str,
    scores: Option<(f64, f64)>,
    at: EpochMs,
    gated: bool,
) {
    let mut spec = LinkSpec::new(stock_id, "기사", at);
    spec.kind = "keyword";
    spec.relevance = scores.map(|s| s.0);
    spec.magnitude = scores.map(|s| s.1);
    let (match_id, _) = insert_link(&db.conn, &spec);
    set_link_path(&db.conn, match_id, path, gated);
}

#[test]
fn counts_by_last_word_of_the_path_across_regenerated_trees() {
    let (db, stock, _) = testdb();
    let at = now() - DAY;
    // 전체 재생성 전후로 경로가 달라져도 마지막 낱말이 같으면 한 키워드
    link(&db, stock, "삼성전자 → HBM", Some((8.0, 8.0)), at, false);
    link(
        &db,
        stock,
        "삼성전자 → 메모리 → HBM",
        Some((8.0, 8.0)),
        at + 1000,
        false,
    );
    link(&db, stock, "삼성전자 → 환율", Some((8.0, 8.0)), at, false);
    let table = keyword_performance(&db.conn, stock, now() - 28 * DAY).unwrap();
    assert_eq!(
        table["hbm"],
        KeywordPerformance {
            matched: 2,
            passed: 2,
            last_passed_at: Some(at + 1000)
        }
    );
    assert_eq!(table["환율"].matched, 1);
    assert_eq!(table.len(), 2);
}

#[test]
fn gated_links_count_as_matched_and_pass_follows_the_display_rule() {
    let (db, stock, _) = testdb();
    let at = now() - DAY;
    // 걸러진 연결(평가 없음)
    link(&db, stock, "삼성전자 → HBM", None, at, true);
    // 기준 미만(관련도 5), 영향도만 미만(관련도 8·영향도 5), 기준 통과
    link(&db, stock, "삼성전자 → HBM", Some((5.0, 8.0)), at, false);
    link(&db, stock, "삼성전자 → HBM", Some((8.0, 5.0)), at, false);
    link(
        &db,
        stock,
        "삼성전자 → HBM",
        Some((6.0, 6.0)),
        at + 5,
        false,
    );
    let table = keyword_performance(&db.conn, stock, now() - 28 * DAY).unwrap();
    assert_eq!(
        table["hbm"],
        KeywordPerformance {
            matched: 4,
            passed: 1,
            last_passed_at: Some(at + 5)
        }
    );
}

#[test]
fn ignores_other_periods_other_stocks_and_other_kinds() {
    let (db, stock, other) = testdb();
    link(
        &db,
        stock,
        "삼성전자 → HBM",
        Some((8.0, 8.0)),
        now() - 30 * DAY,
        false,
    );
    link(
        &db,
        other,
        "SK하이닉스 → HBM",
        Some((8.0, 8.0)),
        now() - DAY,
        false,
    );
    // 이름 연결은 키워드 실적이 아니다
    let mut spec = LinkSpec::new(stock, "이름", now() - DAY);
    spec.kind = "name";
    let (match_id, _) = insert_link(&db.conn, &spec);
    set_link_path(&db.conn, match_id, "삼성전자 → HBM", false);
    let table = keyword_performance(&db.conn, stock, now() - 28 * DAY).unwrap();
    assert!(table.is_empty());
    // 기간을 넓히면 옛 연결이 보인다
    let table = keyword_performance(&db.conn, stock, 0).unwrap();
    assert_eq!(table["hbm"].matched, 1);
}

#[test]
fn last_passed_time_is_the_latest_pass() {
    let (db, stock, _) = testdb();
    let first = now() - 10 * DAY;
    let second = now() - 3 * DAY;
    link(
        &db,
        stock,
        "삼성전자 → HBM",
        Some((8.0, 8.0)),
        second,
        false,
    );
    link(&db, stock, "삼성전자 → HBM", Some((8.0, 8.0)), first, false);
    // 더 늦은 연결이 기준 미만이면 마지막 통과가 아니다
    link(
        &db,
        stock,
        "삼성전자 → HBM",
        Some((2.0, 2.0)),
        now() - DAY,
        false,
    );
    let table = keyword_performance(&db.conn, stock, now() - 28 * DAY).unwrap();
    assert_eq!(table["hbm"].last_passed_at, Some(second));
}
