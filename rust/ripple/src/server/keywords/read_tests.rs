use super::*;
use crate::server::db::schema::Market;
use crate::server::keywords::generate::replace_keywords;
use crate::server::keywords::tree::KeywordNode;
use crate::server::stocks::repository::{
    RegisterInput, RegisterResult, list_active_stocks, register_stock,
};
use crate::test_support::{
    TestDb, attach_topic, ms, seed_lens, seed_listings, seed_market, seed_topic, test_db,
};

fn now() -> EpochMs {
    ms("2026-10-22T06:00:00Z")
}

fn seeded() -> TestDb {
    let mut db = test_db();
    seed_listings(&mut db.conn);
    db
}

/// 깊이 2 트리(1단계 하나, 2단계 하나)
fn depth_two(name: &str) -> Vec<KeywordNode> {
    let node = |key, parent_key, level, text: &str, path: String| KeywordNode {
        key,
        parent_key,
        level,
        kind: crate::server::db::schema::KeywordKind::Structure,
        text_ko: text.into(),
        text_en: None,
        reason: String::new(),
        path,
        relevance: None,
        magnitude: None,
    };
    vec![
        node(1, None, 1, "가", format!("{name} → 가")),
        node(2, Some(1), 2, "나", format!("{name} → 가 → 나")),
    ]
}

fn register(db: &TestDb, code: &str) -> crate::server::stocks::repository::Stock {
    match register_stock(
        &db.conn,
        &RegisterInput {
            market: Market::Kr,
            code,
            user_input: "삼전",
            name_ko: None,
        },
        now(),
    )
    .unwrap()
    {
        RegisterResult::Created(stock) => stock,
        _ => panic!("표본"),
    }
}

// describe("stocksWithOtherDepth (11단계)")
#[test]
fn markets_and_user_topics_are_rebuilt_but_auto_topics_and_topic_branches_do_not_count() {
    // it("시장·사용자 주제도 다시 만들고, 자동 토픽 항목과 토픽 가지는 깊이에 넣지 않는다")
    let mut db = seeded();
    let samsung = register(&db, "005930");
    let market = seed_market(&db.conn, "KR", now());
    let user_topic = seed_topic(&db.conn, "user", "부동산 시장", &[], now());
    let user_lens = seed_lens(&db.conn, user_topic, "부동산 시장", "user", "KR", now());
    let auto = seed_topic(&db.conn, "auto", "중동 긴장", &["사우디"], now());
    seed_lens(&db.conn, auto, "중동 긴장", "auto", "KR", now());

    replace_keywords(&mut db.conn, samsung.id, &depth_two("삼성전자"), now()).unwrap();
    replace_keywords(&mut db.conn, market, &depth_two("한국 증시"), now()).unwrap();
    replace_keywords(
        &mut db.conn,
        user_lens,
        &depth_two("부동산 시장")[..1],
        now(),
    )
    .unwrap();
    // 토픽 가지(1·2단계)는 트리 깊이에 넣지 않는다(아래에서 깊이 1 트리로 확인).
    attach_topic(
        &db.conn,
        samsung.id,
        "삼성전자",
        auto,
        "중동 긴장",
        &["사우디"],
        now(),
    );

    // 기본 깊이 2: 깊이 1인 사용자 주제만 다시 만든다(자동 토픽 항목은 1단계뿐이어도 뺀다).
    assert_eq!(stocks_with_other_depth(&db.conn).unwrap(), [user_lens]);

    // 깊이 1 트리에 2단계까지인 토픽 가지가 붙어 있어도 깊이 1로 본다.
    replace_keywords(&mut db.conn, samsung.id, &depth_two("삼성전자")[..1], now()).unwrap();
    assert_eq!(
        stocks_with_other_depth(&db.conn).unwrap(),
        [samsung.id, user_lens]
    );
}

#[test]
fn list_active_stocks_returns_only_stocks() {
    // it("listActiveStocks는 종목만 돌려준다(시장·주제 항목은 뺀다)")
    let db = seeded();
    seed_market(&db.conn, "KR", now());
    let topic = seed_topic(&db.conn, "user", "부동산 시장", &[], now());
    seed_lens(&db.conn, topic, "부동산 시장", "user", "KR", now());
    assert!(list_active_stocks(&db.conn).unwrap().is_empty());
    let samsung = register(&db, "005930");
    assert_eq!(
        list_active_stocks(&db.conn)
            .unwrap()
            .iter()
            .map(|s| s.id)
            .collect::<Vec<_>>(),
        [samsung.id]
    );
    let kinds: i64 = db
        .conn
        .query_row(
            "SELECT count(*) FROM stocks WHERE kind = 'stock'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(kinds, 1);
}

#[test]
fn pending_or_generating_stocks_are_left_out() {
    // Rust에서만 생기는 경우: 만드는 중·대기 중인 종목은 뺀다(TS 필터의 status 조건)
    let mut db = seeded();
    let samsung = register(&db, "005930");
    // 새로 등록한 종목은 pending이라 키워드가 없어도 빠진다.
    assert!(stocks_with_other_depth(&db.conn).unwrap().is_empty());
    crate::server::keywords::generate::set_keyword_status(
        &db.conn,
        samsung.id,
        crate::server::db::schema::KeywordStatus::Ready,
        None,
    )
    .unwrap();
    assert_eq!(stocks_with_other_depth(&db.conn).unwrap(), [samsung.id]);
    replace_keywords(&mut db.conn, samsung.id, &depth_two("삼성전자"), now()).unwrap();
    assert!(stocks_with_other_depth(&db.conn).unwrap().is_empty());
}
