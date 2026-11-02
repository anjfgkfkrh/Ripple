use super::*;
use crate::server::db::schema::Market;
use crate::server::settings::{EVALUATION_MIN_RELEVANCE, write_num};
use crate::server::stocks::repository::{RegisterInput, RegisterResult, register_stock};
use crate::test_support::{LinkSpec, TestDb, insert_link, ms, seed_listings, test_db};

const HOUR: i64 = 60 * 60 * 1000;

fn now() -> EpochMs {
    ms("2026-10-21T03:00:00Z")
}

fn ago(hours: i64) -> EpochMs {
    now() - hours * HOUR
}

fn seeded() -> TestDb {
    let mut db = test_db();
    seed_listings(&mut db.conn);
    db
}

fn add_stocks(db: &TestDb) -> (i64, i64) {
    let ids: Vec<i64> = ["005930", "000660"]
        .iter()
        .map(|code| {
            match register_stock(
                &db.conn,
                &RegisterInput {
                    market: Market::Kr,
                    code,
                    user_input: code,
                    name_ko: None,
                },
                now(),
            )
            .unwrap()
            {
                RegisterResult::Created(stock) => stock.id,
                _ => panic!("표본에 없음"),
            }
        })
        .collect();
    (ids[0], ids[1])
}

// describe("recentHeadlines (FR-3 키워드 재생성 입력, 2026-10-21 결정)")
#[test]
fn name_and_direct_links_with_relevance_at_least_threshold_within_7_days_newest_first() {
    // it("종목 이름·직접 연결이고 표시 기준(6점) 이상인 최근 7일 기사 제목을 최근순으로")
    let db = seeded();
    let (samsung, hynix) = add_stocks(&db);
    let link = |title: &str, hours: i64, kind: &'static str, relevance: Option<f64>| {
        let spec = LinkSpec {
            stock_id: samsung,
            title: Some(title),
            published_at: ago(hours),
            kind,
            relevance,
            magnitude: None,
        };
        insert_link(&db.conn, &spec);
    };
    link("삼성전자 실적 발표", 30, "name", Some(7.0));
    link("삼성전자 노조 협상", 2, "name", Some(7.0));
    link("Samsung shares rise", 5, "direct", Some(7.0));
    link("기준 딱 6점", 6, "name", Some(6.0));
    // 빠지는 것
    link("키워드로 걸린 기사", 1, "keyword", Some(7.0));
    link("관련도 낮은 기사", 1, "name", Some(5.9));
    link("평가 전 기사", 1, "name", None);
    link("8일 지난 기사", 8 * 24, "name", Some(7.0));
    insert_link(
        &db.conn,
        &LinkSpec {
            stock_id: hynix,
            title: Some("다른 종목 기사"),
            published_at: ago(1),
            kind: "name",
            relevance: Some(9.0),
            magnitude: None,
        },
    );

    assert_eq!(
        recent_headlines(&db.conn, samsung, now()).unwrap(),
        [
            "삼성전자 노조 협상",
            "Samsung shares rise",
            "기준 딱 6점",
            "삼성전자 실적 발표"
        ]
    );

    write_num(&db.conn, &EVALUATION_MIN_RELEVANCE, 6.5, now()).unwrap();
    assert!(
        !recent_headlines(&db.conn, samsung, now())
            .unwrap()
            .contains(&"기준 딱 6점".to_string())
    );
}

#[test]
fn same_title_once_even_with_whitespace_differences_and_at_most_20() {
    // it("같은 제목(공백 차이 포함)은 하나만, 최대 20개")
    let db = seeded();
    let (samsung, _) = add_stocks(&db);
    insert_link(
        &db.conn,
        &LinkSpec::new(samsung, "같은  제목 ", ago(1)).with_relevance(8.0),
    );
    insert_link(
        &db.conn,
        &LinkSpec::new(samsung, "같은 제목", ago(2)).with_relevance(8.0),
    );
    for i in 0..30 {
        insert_link(
            &db.conn,
            &LinkSpec::new(samsung, &format!("기사 {i}"), ago(3 + i)).with_relevance(8.0),
        );
    }
    let titles = recent_headlines(&db.conn, samsung, now()).unwrap();
    assert_eq!(titles.len(), MAX_HEADLINES);
    assert_eq!(&titles[..2], ["같은 제목", "기사 0"]);
}
