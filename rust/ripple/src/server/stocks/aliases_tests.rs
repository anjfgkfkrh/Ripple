use super::*;
use crate::server::db::schema::Market;
use crate::server::stocks::repository::{
    RegisterInput, RegisterResult, register_stock, remove_alias, set_stock_terms,
};
use crate::test_support::{TestDb, test_db};

const NOW: i64 = 1_792_000_000_000;

/// 한국 종목 하나(현대건설)와 다른 종목 몇 개가 있는 DB
fn setup() -> (TestDb, Stock) {
    let db = test_db();
    for (code, name) in [
        ("000720", "현대건설"),
        ("294870", "HDC현대산업개발"),
        ("005930", "삼성전자"),
        ("028260", "삼성물산"),
    ] {
        db.conn
            .execute(
                "INSERT INTO listings (market, code, name, exchange) VALUES ('KR', ?1, ?2, 'KOSPI')",
                [code, name],
            )
            .unwrap();
    }
    let RegisterResult::Created(stock) = register_stock(
        &db.conn,
        &RegisterInput {
            market: Market::Kr,
            code: "000720",
            user_input: "현대건설",
            name_ko: None,
        },
        NOW,
    )
    .unwrap() else {
        panic!("created");
    };
    (db, stock)
}

fn check(db: &TestDb, stock: &Stock, candidates: &[&str]) -> Vec<String> {
    let list: Vec<String> = candidates.iter().map(|s| s.to_string()).collect();
    validate_aliases(&db.conn, stock, &list).unwrap()
}

#[test]
fn trims_and_keeps_good_aliases() {
    let (db, stock) = setup();
    assert_eq!(
        check(&db, &stock, &["  현대 E&C ", "Hyundai E&C"]),
        ["현대 E&C", "Hyundai E&C"]
    );
}

#[test]
fn drops_aliases_shorter_than_two_chars() {
    let (db, stock) = setup();
    assert_eq!(check(&db, &stock, &["현", " a ", "현건"]), ["현건"]);
}

#[test]
fn drops_own_name_and_code_ignoring_spaces_and_case() {
    let (db, mut stock) = setup();
    stock.name = "Hyundai Engineering".into();
    stock.name_ko = Some("현대 건설".into());
    assert_eq!(
        check(
            &db,
            &stock,
            &["hyundaiengineering", "현대건설", "000720", "현건"]
        ),
        ["현건"]
    );
}

#[test]
fn drops_names_equal_to_other_listings() {
    let (db, stock) = setup();
    assert_eq!(check(&db, &stock, &["hdc 현대산업개발", "현건"]), ["현건"]);
}

#[test]
fn drops_aliases_containing_another_listing_name() {
    let (db, stock) = setup();
    assert_eq!(check(&db, &stock, &["삼성물산그룹", "현건"]), ["현건"]);
}

#[test]
fn drops_aliases_contained_in_another_listing_name() {
    let (db, stock) = setup();
    assert_eq!(check(&db, &stock, &["삼성", "현건"]), ["현건"]);
}

#[test]
fn own_listing_row_is_not_compared() {
    let (db, stock) = setup();
    // 자기 줄 이름에 포함되는 말이라도 다른 종목에 걸리지 않으면 쓴다.
    assert_eq!(check(&db, &stock, &["대건설"]), ["대건설"]);
}

#[test]
fn drops_removed_aliases_and_duplicates() {
    let (db, stock) = setup();
    set_stock_terms(&db.conn, stock.id, &["현건".to_string()], &[]).unwrap();
    remove_alias(&db.conn, stock.id, "현건").unwrap();
    let stock = crate::server::stocks::repository::find_stock(&db.conn, stock.id)
        .unwrap()
        .unwrap();
    assert_eq!(
        check(&db, &stock, &["현 건", "Hyundai", "hyundai", "HYUNDAI"]),
        ["Hyundai"]
    );
}

#[test]
fn keeps_at_most_five() {
    let (db, stock) = setup();
    assert_eq!(
        check(
            &db,
            &stock,
            &["aa1", "aa2", "aa3", "aa4", "aa5", "aa6", "aa7"]
        )
        .len(),
        5
    );
}
