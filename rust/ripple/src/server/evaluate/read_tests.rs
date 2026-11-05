use super::*;
use crate::server::db::schema::Market;
use crate::server::stocks::repository::{
    RegisterInput, RegisterResult, delete_stock, register_stock,
};
use crate::test_support::{LinkSpec, insert_link, ms, seed_listings, test_db};

#[test]
fn counts_unevaluated_matches_in_the_last_seven_days_for_live_stocks() {
    // 아직 평가하지 않은 최근 7일 연결만, 삭제한 종목은 빼고
    let mut t = test_db();
    seed_listings(&mut t.conn);
    let now = ms("2026-10-21T03:00:00Z");
    let register = |code: &str| {
        let result = register_stock(
            &t.conn,
            &RegisterInput {
                market: Market::Kr,
                code,
                user_input: code,
                name_ko: None,
            },
            now,
        )
        .unwrap();
        let RegisterResult::Created(stock) = result else {
            panic!("표본에 없음")
        };
        stock.id
    };
    let (samsung, hynix) = (register("005930"), register("000660"));
    let day = 24 * 60 * 60 * 1000;
    let pending = |stock: i64, at: i64| {
        let mut link = LinkSpec::new(stock, "기사", at);
        link.relevance = None;
        insert_link(&t.conn, &link);
    };
    pending(samsung, now - day);
    pending(samsung, now - 8 * day); // 7일 밖
    pending(hynix, now - day);
    insert_link(&t.conn, &LinkSpec::new(samsung, "평가함", now - day)); // 평가함
    assert_eq!(count_pending_evaluations(&t.conn, now).unwrap(), 2);
    delete_stock(&t.conn, hynix, now).unwrap();
    assert_eq!(count_pending_evaluations(&t.conn, now).unwrap(), 1);
}
