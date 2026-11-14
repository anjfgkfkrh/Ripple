use super::*;
use crate::test_support::{LinkSpec, insert_link, ms, seed_listings, shared_test_db};

fn set(words: &[&str]) -> HashSet<String> {
    words.iter().map(|w| (*w).to_string()).collect()
}

fn titles(items: &[&str]) -> Vec<String> {
    items.iter().map(|t| (*t).to_string()).collect()
}

// describe("tokenize")
#[test]
fn tokenize_splits_strips_edges_and_trailing_particle() {
    // it("구분 글자로 쪼개고, 앞뒤 기호를 떼고, 3자 이상이면 끝의 조사를 뗀다")
    assert_eq!(
        tokenize("삼성전자, 엔비디아에 HBM4 공급… [단독] ‘반도체’는 호조"),
        set(&[
            "삼성전자",
            "엔비디아",
            "hbm4",
            "공급",
            "단독",
            "반도체",
            "호조"
        ])
    );
    // 2자 낱말은 조사를 떼지 않는다('국가'의 '가'는 그대로).
    assert_eq!(tokenize("국가 부채"), set(&["국가", "부채"]));
    // 긴 조사가 먼저 떨어진다.
    assert_eq!(
        tokenize("반도체로부터 시장으로"),
        set(&["반도체로", "시장"])
    );
}

#[test]
fn tokenize_drops_short_and_numeric_words_and_dedupes() {
    // it("2자 미만과 숫자만인 것은 버리고, 같은 낱말은 하나로 센다")
    assert_eq!(tokenize("A 5 2026 삼성 삼성 12.5"), set(&["삼성", "12.5"]));
}

// describe("scoreWords")
#[test]
fn threshold_is_3_articles_or_3_percent() {
    // it("문턱은 3번 또는 기사 수의 3% 가운데 큰 쪽")
    let background = titles(&["경제 소식 하나", "경제 소식 둘", "경제 소식 셋"]);
    let two = titles(&["하이닉스 공급 확대", "하이닉스 실적 개선"]);
    assert!(score_words(&two, &background).words.is_empty());
    let three = titles(&[
        "하이닉스 공급 확대",
        "하이닉스 실적 개선",
        "하이닉스 증설 발표",
    ]);
    assert_eq!(
        score_words(&three, &background).words,
        [("하이닉스".to_string(), 3)]
    );
    // 기사 200개면 문턱은 6(3%)이다: 5번 나온 낱말은 걸러지고 6번은 남는다.
    let mut many: Vec<String> = (0..200).map(|i| format!("기사 번호{i}")).collect();
    for (i, title) in many.iter_mut().enumerate().take(6) {
        title.push_str(" 주력사업");
        if i < 5 {
            title.push_str(" 신제품출시");
        }
    }
    let words = score_words(&many, &background).words;
    assert!(words.iter().any(|(w, c)| w == "주력사업" && *c == 6));
    assert!(!words.iter().any(|(w, _)| w == "신제품출시"));
}

#[test]
fn words_are_scored_by_lift_over_background_and_ordered() {
    // it("배경에서 흔한 낱말은 점수가 낮고, 점수 순서로 돌려준다")
    let background: Vec<String> = (0..50)
        .map(|i| format!("시장 동향 {i}번 소식{}", if i < 25 { " 공급" } else { "" }))
        .collect();
    let articles = titles(&[
        "한화 공급 계약",
        "한화 공급 확대",
        "한화 공급 수주",
        "한화 방산 수출",
        "한화 방산 확대",
        "한화 방산 증설",
    ]);
    let result = score_words(&articles, &background);
    assert_eq!(result.articles, 6);
    // 한화(6/6, 배경 0) > 방산(3) > 공급(3, 배경 25)
    let order: Vec<&str> = result.words.iter().map(|(w, _)| w.as_str()).collect();
    assert_eq!(order, ["한화", "방산", "공급"]);
    assert_eq!(result.words[0].1, 6);
}

#[test]
fn only_top_30_words_are_kept() {
    // it("위에서 30개만")
    let background = titles(&["배경 제목 하나"]);
    let articles: Vec<String> = (0..3)
        .map(|_| {
            (0..40)
                .map(|n| format!("낱말{n:02}가"))
                .collect::<Vec<_>>()
                .join(" ")
        })
        .collect();
    assert_eq!(score_words(&articles, &background).words.len(), STAT_TOP);
}

#[test]
fn no_articles_or_no_background_gives_an_empty_list() {
    // it("기사가 없으면 빈 목록")
    assert_eq!(score_words(&[], &titles(&["배경"])), StatWords::default());
    let none = score_words(&titles(&["가나다 라마바"]), &[]);
    assert!(none.words.is_empty());
}

// describe("statWords")
fn insert_feed(conn: &Connection, count: usize) {
    for i in 0..count {
        conn.execute(
            "INSERT INTO feed_headlines (title_key, title, publisher, feed, published_at, seen_at) \
             VALUES (?1, ?2, '테스트', 'rss:test:economy', ?3, ?3)",
            rusqlite::params![format!("k{i}"), format!("경제 소식 {i}번째 기사"), 0],
        )
        .unwrap();
    }
}

fn first_stock(conn: &Connection) -> i64 {
    use crate::server::db::schema::Market;
    use crate::server::stocks::repository::{RegisterInput, RegisterResult, register_stock};
    match register_stock(
        conn,
        &RegisterInput {
            market: Market::Kr,
            code: "005930",
            user_input: "삼전",
            name_ko: None,
        },
        ms("2026-10-20T03:00:00Z"),
    )
    .unwrap()
    {
        RegisterResult::Created(stock) => stock.id,
        _ => panic!("표본에 없는 종목"),
    }
}

#[test]
fn db_words_come_from_distinct_passed_recent_name_links_only() {
    // it("그 종목의 이름 연결, 최근 14일 평가, 표시 기준 통과, 서로 다른 제목만 센다")
    let shared = shared_test_db();
    seed_listings(&mut shared.db.lock().unwrap());
    let conn = shared.db.lock().unwrap();
    let id = first_stock(&conn);
    let now = ms("2026-11-10T03:00:00Z");
    insert_feed(&conn, 20);
    let day = 24 * 60 * 60 * 1000;
    fn pass(id: i64, title: &str, at: i64) -> LinkSpec<'_> {
        let mut link = LinkSpec::new(id, title, at);
        link.relevance = Some(8.0);
        link.magnitude = Some(8.0);
        link
    }
    // 통과 3건 + 같은 제목 중복(한 번만 센다)
    for title in [
        "삼성전자 엔비디아 납품",
        "삼성전자 엔비디아 승인",
        "삼성전자 엔비디아 협력",
    ] {
        insert_link(&conn, &pass(id, title, now - day));
    }
    insert_link(&conn, &pass(id, "삼성전자 엔비디아 협력", now - day));
    // 오래된 평가·키워드 연결·영향도 미달·다른 종목은 센다 하지 않는다.
    insert_link(&conn, &pass(id, "삼성전자 오래된 소식", now - 15 * day));
    let mut keyword = pass(id, "삼성전자 키워드 연결", now - day);
    keyword.kind = "keyword";
    insert_link(&conn, &keyword);
    let mut weak = pass(id, "삼성전자 영향도 미달", now - day);
    weak.magnitude = Some(1.0);
    insert_link(&conn, &weak);

    let result = stat_words(&conn, id, now).unwrap();
    assert_eq!(result.articles, 3);
    // 점수가 같으면 낱말이 큰 쪽이 먼저다(파이썬 실측 스크립트의 정렬과 같다).
    assert_eq!(
        result.words,
        [("엔비디아".to_string(), 3), ("삼성전자".to_string(), 3)]
    );
}

#[test]
fn db_without_passed_links_is_empty() {
    // it("통과한 이름 연결이 없으면 빈 목록")
    let shared = shared_test_db();
    seed_listings(&mut shared.db.lock().unwrap());
    let conn = shared.db.lock().unwrap();
    let id = first_stock(&conn);
    insert_feed(&conn, 5);
    assert_eq!(
        stat_words(&conn, id, ms("2026-11-10T03:00:00Z")).unwrap(),
        StatWords::default()
    );
}

#[test]
fn db_words_skip_hidden_stock_evaluations() {
    // 숨김 규칙: bystander 0.7인 평가의 제목은 대상에서 빠지고, 0.3·NULL은 남는다
    let shared = shared_test_db();
    seed_listings(&mut shared.db.lock().unwrap());
    let conn = shared.db.lock().unwrap();
    let id = first_stock(&conn);
    let now = ms("2026-11-10T03:00:00Z");
    insert_feed(&conn, 20);
    let day = 24 * 60 * 60 * 1000;
    let mut matches = Vec::new();
    for title in [
        "삼성전자 엔비디아 납품",
        "삼성전자 엔비디아 승인",
        "삼성전자 엔비디아 협력",
    ] {
        let mut link = LinkSpec::new(id, title, now - day);
        link.relevance = Some(8.0);
        link.magnitude = Some(8.0);
        matches.push(insert_link(&conn, &link).0);
    }
    for (m, value) in matches.iter().zip([Some(0.7), Some(0.3), None]) {
        conn.execute(
            "UPDATE evaluations SET bystander = ?1 WHERE match_id = ?2",
            rusqlite::params![value, m],
        )
        .unwrap();
    }
    assert_eq!(stat_words(&conn, id, now).unwrap().articles, 2);
}
