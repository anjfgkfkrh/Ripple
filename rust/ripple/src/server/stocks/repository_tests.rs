use super::*;
use crate::test_support::{TestDb, ms, seed_listings, test_db};

fn seeded() -> TestDb {
    let mut db = test_db();
    seed_listings(&mut db.conn);
    db
}

fn input<'a>(
    market: Market,
    code: &'a str,
    user_input: &'a str,
    name_ko: Option<&'a str>,
) -> RegisterInput<'a> {
    RegisterInput {
        market,
        code,
        user_input,
        name_ko,
    }
}

const NOW: EpochMs = 1_792_000_000_000;

// describe("종목 등록 (SPEC FR-2)")
#[test]
fn saves_listing_values_and_keeps_user_input() {
    // it("공식 목록 값으로 저장하고 사용자 입력을 남긴다")
    let db = seeded();
    let result = register_stock(
        &db.conn,
        &input(Market::Us, "nvda", " 엔비디아 ", Some("엔비디아")),
        NOW,
    )
    .unwrap();
    assert!(matches!(result, RegisterResult::Created(_)));
    let stocks = list_active_stocks(&db.conn).unwrap();
    assert_eq!(stocks.len(), 1);
    let stock = &stocks[0];
    assert_eq!(stock.market, Market::Us);
    assert_eq!(stock.code, "NVDA");
    assert_eq!(stock.name, "NVIDIA Corporation - Common Stock");
    assert_eq!(stock.name_ko.as_deref(), Some("엔비디아"));
    assert_eq!(stock.exchange, "NASDAQ");
    assert_eq!(stock.user_input, "엔비디아");
    assert_eq!(stock.deleted_at, None);
    // 열 기본값(마이그레이션)도 그대로
    assert_eq!(stock.keywords_status, KeywordStatus::Pending);
    assert_eq!(stock.kind, ItemKind::Stock);
}

#[test]
fn korean_name_ko_is_the_official_name() {
    // it("한국 종목의 한글 이름은 정식 이름을 쓴다")
    let db = seeded();
    register_stock(
        &db.conn,
        &input(Market::Kr, "005930", "삼전", Some("삼성")),
        NOW,
    )
    .unwrap();
    assert_eq!(
        list_active_stocks(&db.conn).unwrap()[0].name_ko.as_deref(),
        Some("삼성전자")
    );
}

#[test]
fn does_not_register_unlisted_stock() {
    // it("목록에 없으면 등록하지 않는다(D-12)")
    let db = seeded();
    let result = register_stock(&db.conn, &input(Market::Kr, "123456", "x", None), NOW).unwrap();
    assert_eq!(result, RegisterResult::NotListed);
    assert!(list_active_stocks(&db.conn).unwrap().is_empty());
}

#[test]
fn new_or_restored_stock_backfills_24_hours_before_registration() {
    // it("새로 등록하거나 되살리면 다음 회차에서 등록 24시간 전 기사까지 연결하게 표시한다")
    let db = seeded();
    let at = ms("2026-10-21T03:00:00Z");
    let RegisterResult::Created(created) =
        register_stock(&db.conn, &input(Market::Kr, "005930", "삼전", None), at).unwrap()
    else {
        panic!("등록 실패");
    };
    assert_eq!(created.backfill_from, Some(ms("2026-10-20T03:00:00Z")));

    delete_stock(&db.conn, created.id, at).unwrap();
    let later = ms("2026-10-25T03:00:00Z");
    let RegisterResult::Restored(restored) =
        register_stock(&db.conn, &input(Market::Kr, "005930", "삼전", None), later).unwrap()
    else {
        panic!("되살리기 실패");
    };
    assert_eq!(restored.backfill_from, Some(ms("2026-10-24T03:00:00Z")));
}

#[test]
fn already_registered_is_exists() {
    // it("이미 등록했으면 exists")
    let db = seeded();
    register_stock(&db.conn, &input(Market::Kr, "005930", "삼전", None), NOW).unwrap();
    let again = register_stock(&db.conn, &input(Market::Kr, "005930", "삼성", None), NOW).unwrap();
    assert!(matches!(again, RegisterResult::Exists(_)));
    assert_eq!(list_active_stocks(&db.conn).unwrap().len(), 1);
}

// describe("종목 삭제 (SPEC D-11: 숨기고 보존)")
#[test]
fn delete_hides_and_registering_again_restores_same_stock() {
    // it("지우지 않고 숨기며, 다시 등록하면 같은 종목을 되살린다")
    let db = seeded();
    let RegisterResult::Created(created) =
        register_stock(&db.conn, &input(Market::Kr, "005930", "삼전", None), NOW).unwrap()
    else {
        panic!("등록 실패");
    };
    let at = ms("2026-10-20T00:00:00Z");

    assert!(delete_stock(&db.conn, created.id, at).unwrap());
    assert!(list_active_stocks(&db.conn).unwrap().is_empty());
    assert!(!delete_stock(&db.conn, created.id, NOW).unwrap());

    let RegisterResult::Restored(restored) = register_stock(
        &db.conn,
        &input(Market::Kr, "005930", "삼성전자", None),
        NOW,
    )
    .unwrap() else {
        panic!("되살리기 실패");
    };
    assert_eq!(restored.id, created.id);
    assert_eq!(restored.user_input, "삼성전자");
    assert_eq!(list_active_stocks(&db.conn).unwrap().len(), 1);
}

#[test]
fn deleting_unknown_stock_is_false() {
    // it("없는 종목은 false")
    assert!(!delete_stock(&seeded().conn, 999, NOW).unwrap());
}

#[test]
fn name_ko_is_trimmed_cut_to_50_units_and_empty_becomes_none() {
    // Rust에서만 생기는 경우: `nameKo?.trim().slice(0, 50) || null`
    let db = seeded();
    let long = "가".repeat(60);
    register_stock(
        &db.conn,
        &input(Market::Us, "NVDA", "x", Some(&format!("  {long}  "))),
        NOW,
    )
    .unwrap();
    assert_eq!(
        list_active_stocks(&db.conn).unwrap()[0].name_ko,
        Some("가".repeat(50))
    );
    let db = seeded();
    register_stock(&db.conn, &input(Market::Us, "NVDA", "x", Some("   ")), NOW).unwrap();
    assert_eq!(list_active_stocks(&db.conn).unwrap()[0].name_ko, None);
}

#[test]
fn active_checks_by_code_and_id() {
    let db = seeded();
    let RegisterResult::Created(stock) =
        register_stock(&db.conn, &input(Market::Kr, "005930", "삼전", None), NOW).unwrap()
    else {
        panic!("등록 실패");
    };
    assert!(is_active_stock(&db.conn, Market::Kr, "005930").unwrap());
    assert!(is_active_stock_id(&db.conn, stock.id).unwrap());
    delete_stock(&db.conn, stock.id, NOW).unwrap();
    assert!(!is_active_stock(&db.conn, Market::Kr, "005930").unwrap());
    assert!(!is_active_stock_id(&db.conn, stock.id).unwrap());
}

fn strings(items: &[&str]) -> Vec<String> {
    items.iter().map(|s| s.to_string()).collect()
}

#[test]
fn new_stock_has_empty_alias_lists_and_terms_round_trip() {
    let db = seeded();
    let RegisterResult::Created(stock) = register_stock(
        &db.conn,
        &input(Market::Kr, "005930", "삼성전자", None),
        NOW,
    )
    .unwrap() else {
        panic!("created");
    };
    assert!(
        stock.aliases.is_empty() && stock.anchors.is_empty() && stock.aliases_removed.is_empty()
    );

    assert!(
        set_stock_terms(
            &db.conn,
            stock.id,
            &strings(&["삼전", "Samsung"]),
            &strings(&["반도체"])
        )
        .unwrap()
    );
    let read = find_stock(&db.conn, stock.id).unwrap().unwrap();
    assert_eq!(read.aliases, strings(&["삼전", "Samsung"]));
    assert_eq!(read.anchors, strings(&["반도체"]));
    assert!(!set_stock_terms(&db.conn, 9999, &[], &[]).unwrap());
}

#[test]
fn removing_alias_moves_it_to_removed_list() {
    let db = seeded();
    let RegisterResult::Created(stock) = register_stock(
        &db.conn,
        &input(Market::Kr, "005930", "삼성전자", None),
        NOW,
    )
    .unwrap() else {
        panic!("created");
    };
    set_stock_terms(&db.conn, stock.id, &strings(&["삼전", "Samsung"]), &[]).unwrap();
    assert!(remove_alias(&db.conn, stock.id, "삼전").unwrap());
    let read = find_stock(&db.conn, stock.id).unwrap().unwrap();
    assert_eq!(read.aliases, strings(&["Samsung"]));
    assert_eq!(read.aliases_removed, strings(&["삼전"]));
    assert!(!remove_alias(&db.conn, stock.id, "없는별칭").unwrap());
    assert!(!remove_alias(&db.conn, stock.id, "삼전").unwrap());
}

#[test]
fn broken_json_lists_read_as_empty() {
    let db = seeded();
    let RegisterResult::Created(stock) = register_stock(
        &db.conn,
        &input(Market::Kr, "005930", "삼성전자", None),
        NOW,
    )
    .unwrap() else {
        panic!("created");
    };
    db.conn
        .execute(
            "UPDATE stocks SET aliases = 'oops' WHERE id = ?1",
            [stock.id],
        )
        .unwrap();
    assert!(
        find_stock(&db.conn, stock.id)
            .unwrap()
            .unwrap()
            .aliases
            .is_empty()
    );
}
