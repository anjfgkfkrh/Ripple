//! `timeline/read.test.ts`를 옮긴 것(TS `it` 설명을 주석으로 단다).
use std::sync::atomic::{AtomicU64, Ordering};

use rusqlite::{Connection, params};

use super::*;
use crate::server::db::schema::Market;
use crate::server::settings::{EVALUATION_MIN_MAGNITUDE, write_num};
use crate::server::stocks::repository::{
    RegisterInput, RegisterResult, delete_stock, register_stock,
};
use crate::server::timeline::reads::set_articles_read;
use crate::server::topics::store::{
    AddTopicResult, StockRef, TopicLink, add_user_topic, attach_topic_to_stock,
    ensure_default_market, find_topic, set_market_enabled, sync_topic_lenses,
};
use crate::test_support::{TestDb, ms, seed_listings, test_db};

const HOUR: i64 = 60 * 60 * 1000;

fn now() -> EpochMs {
    ms("2026-10-21T03:00:00Z")
}

fn ago(hours: f64) -> EpochMs {
    now() - (hours * HOUR as f64) as i64
}

fn test_db_with_listings() -> TestDb {
    let mut t = test_db();
    seed_listings(&mut t.conn);
    t
}

struct Ids {
    samsung: i64,
    hynix: i64,
    kakao: i64,
}

fn add_stocks(conn: &Connection) -> Ids {
    let ids = ["005930", "000660", "035720"].map(|code| {
        let result = register_stock(
            conn,
            &RegisterInput {
                market: Market::Kr,
                code,
                user_input: code,
                name_ko: None,
            },
            now(),
        )
        .unwrap();
        let RegisterResult::Created(stock) = result else {
            panic!("표본에 없음");
        };
        stock.id
    });
    Ids {
        samsung: ids[0],
        hynix: ids[1],
        kakao: ids[2],
    }
}

static SERIAL: AtomicU64 = AtomicU64::new(0);

/// `tests/fixtures.ts`의 `insertLink` 인자
struct Link<'a> {
    stock_id: i64,
    title: Option<&'a str>,
    published_at: EpochMs,
    article_id: Option<i64>,
    publisher: &'a str,
    kind: &'a str,
    path: &'a str,
    relevance: Option<f64>,
    magnitude: Option<f64>,
    impact: &'a str,
}

impl<'a> Link<'a> {
    fn new(stock_id: i64, published_at: EpochMs) -> Self {
        Self {
            stock_id,
            title: None,
            published_at,
            article_id: None,
            publisher: "가경제",
            kind: "name",
            path: "종목",
            relevance: None,
            magnitude: None,
            impact: "neutral",
        }
    }

    fn titled(mut self, title: &'a str) -> Self {
        self.title = Some(title);
        self
    }

    fn score(mut self, relevance: f64) -> Self {
        self.relevance = Some(relevance);
        self
    }

    fn by(mut self, publisher: &'a str) -> Self {
        self.publisher = publisher;
        self
    }

    fn impact(mut self, impact: &'a str) -> Self {
        self.impact = impact;
        self
    }

    fn keyword(mut self, path: &'a str) -> Self {
        self.kind = "keyword";
        self.path = path;
        self
    }

    fn magnitude(mut self, magnitude: f64) -> Self {
        self.magnitude = Some(magnitude);
        self
    }

    fn on(mut self, article_id: i64) -> Self {
        self.article_id = Some(article_id);
        self
    }

    /// (match_id, article_id)
    fn insert(self, conn: &Connection) -> (i64, i64) {
        let serial = SERIAL.fetch_add(1, Ordering::SeqCst) + 1;
        let article_id = match self.article_id {
            Some(id) => id,
            None => {
                conn.execute(
                    "INSERT INTO articles (url_key, url, title, summary, publisher, feed, published_at, collected_at) \
                     VALUES (?1, ?2, ?3, NULL, ?4, 'rss:a:economy', ?5, ?5)",
                    params![
                        format!("fixture.example/{serial}"),
                        format!("https://fixture.example/{serial}"),
                        self.title.map_or_else(|| format!("기사 {serial}"), str::to_string),
                        self.publisher,
                        self.published_at
                    ],
                )
                .unwrap();
                conn.last_insert_rowid()
            }
        };
        conn.execute(
            "INSERT INTO article_matches (article_id, stock_id, keyword_path, keyword_reason, match_kind, level, created_at) \
             VALUES (?1, ?2, ?3, NULL, ?4, ?5, ?6)",
            params![
                article_id,
                self.stock_id,
                self.path,
                self.kind,
                i64::from(self.kind == "keyword"),
                self.published_at
            ],
        )
        .unwrap();
        let match_id = conn.last_insert_rowid();
        if let Some(relevance) = self.relevance {
            conn.execute(
                "INSERT INTO evaluations (match_id, article_id, stock_id, relevance, relevance_confidence, magnitude, \
                 magnitude_confidence, impact, impact_confidence, impact_probabilities, model, evaluated_at) \
                 VALUES (?1, ?2, ?3, ?4, 0.7, ?5, ?6, ?7, 0.7, \
                 '{\"positive\":0.1,\"negative\":0.1,\"neutral\":0.8}', 'jev-1.13.0', ?8)",
                params![
                    match_id,
                    article_id,
                    self.stock_id,
                    relevance,
                    self.magnitude,
                    self.magnitude.map(|_| 0.7),
                    self.impact,
                    self.published_at
                ],
            )
            .unwrap();
        }
        (match_id, article_id)
    }
}

/// 서로 겹치는 두 글자가 없는 제목(비슷한 제목 묶기에 걸리지 않게)
fn distinct(i: u32) -> String {
    let base = 0xac00 + i * 97;
    format!(
        "{}{}",
        char::from_u32(base).unwrap(),
        char::from_u32(base + 41).unwrap()
    )
}

const ALL: TimelineFilter = TimelineFilter {
    stock_id: None,
    topic_id: None,
    impact: None,
};

fn read(conn: &Connection, filter: &TimelineFilter, limit: usize) -> TimelinePage {
    read_timeline(conn, filter, limit).unwrap()
}

fn titles(conn: &Connection, filter: &TimelineFilter) -> Vec<String> {
    read(conn, filter, TIMELINE_PAGE_SIZE)
        .cards
        .into_iter()
        .map(|card| card.title)
        .collect()
}

fn names(list: &[&str]) -> Vec<String> {
    list.iter().map(|s| (*s).to_string()).collect()
}

// ---- readTimeline (SPEC FR-7) ----

#[test]
fn read_timeline_filters_by_relevance_orders_newest_and_skips_deleted() {
    // 관련도 기준 이상인 기사만 최신순으로, 삭제한 종목은 빼고
    let t = test_db_with_listings();
    let c = &t.conn;
    let ids = add_stocks(c);
    Link::new(ids.samsung, ago(1.0))
        .titled("삼성전자 실적")
        .score(8.0)
        .insert(c);
    Link::new(ids.samsung, ago(2.0))
        .titled("기준 미만")
        .score(5.9)
        .insert(c);
    Link::new(ids.hynix, ago(3.0))
        .titled("SK하이닉스 증설")
        .score(6.0)
        .insert(c);
    Link::new(ids.samsung, ago(0.5)).titled("평가 전").insert(c);
    Link::new(ids.kakao, ago(0.2))
        .titled("삭제한 종목 기사")
        .score(9.0)
        .insert(c);
    delete_stock(c, ids.kakao, now()).unwrap();

    let page = read(c, &ALL, TIMELINE_PAGE_SIZE);
    assert_eq!(
        page.cards
            .iter()
            .map(|c| c.title.as_str())
            .collect::<Vec<_>>(),
        ["삼성전자 실적", "SK하이닉스 증설"]
    );
    let first = &page.cards[0];
    assert_eq!(first.publisher, "가경제");
    assert_eq!(first.published_at, ago(1.0));
    assert!(first.others.is_empty());
    assert_eq!(first.stocks.len(), 1);
    let item = &first.stocks[0];
    assert_eq!(item.stock_id, ids.samsung);
    assert_eq!(item.name, "삼성전자");
    assert_eq!(item.relevance, 8.0);
    assert_eq!(item.impact, Impact::Neutral);
    assert_eq!(item.match_kind, MatchKind::Name);
    assert!(!page.has_more);
    assert_eq!(page.min_relevance, 6.0);
    assert_eq!(page.min_magnitude, 5.0);

    write_num(c, &EVALUATION_MIN_RELEVANCE, 7.0, now()).unwrap();
    assert_eq!(titles(c, &ALL), names(&["삼성전자 실적"]));
}

#[test]
fn read_timeline_one_card_for_many_stocks_in_relevance_order() {
    // 여러 종목에 걸린 기사는 카드 하나에 기준 이상인 종목을 관련도 순으로(D-26)
    let t = test_db_with_listings();
    let c = &t.conn;
    let ids = add_stocks(c);
    let (_, article_id) = Link::new(ids.samsung, ago(1.0))
        .titled("삼성전자·SK하이닉스 HBM 경쟁")
        .keyword("삼성전자 → HBM 메모리")
        .score(7.0)
        .impact("negative")
        .insert(c);
    Link::new(ids.hynix, ago(1.0))
        .on(article_id)
        .score(9.0)
        .impact("positive")
        .insert(c);
    Link::new(ids.kakao, ago(1.0))
        .on(article_id)
        .score(4.0)
        .insert(c);

    let page = read(c, &ALL, TIMELINE_PAGE_SIZE);
    let card = &page.cards[0];
    let summary: Vec<(&str, f64, Impact)> = card
        .stocks
        .iter()
        .map(|s| (s.name.as_str(), s.relevance, s.impact))
        .collect();
    assert_eq!(
        summary,
        [
            ("SK하이닉스", 9.0, Impact::Positive),
            ("삼성전자", 7.0, Impact::Negative)
        ]
    );
    assert_eq!(card.stocks[1].match_kind, MatchKind::Keyword);
    assert_eq!(card.stocks[1].path, "삼성전자 → HBM 메모리");

    // 종목 필터: 그 종목이 카드에 있으면 통과하고, 카드에는 다른 종목도 그대로 보인다.
    let by_samsung = read(
        c,
        &TimelineFilter {
            stock_id: Some(ids.samsung),
            ..ALL
        },
        30,
    )
    .cards;
    assert_eq!(by_samsung.len(), 1);
    assert_eq!(by_samsung[0].stocks.len(), 2);
    assert!(
        titles(
            c,
            &TimelineFilter {
                stock_id: Some(ids.kakao),
                ..ALL
            }
        )
        .is_empty()
    );
    // 영향 필터: 그 영향인 종목이 하나라도 있으면 통과. 둘 다면 같은 종목이어야 한다.
    assert_eq!(
        titles(
            c,
            &TimelineFilter {
                impact: Some(Impact::Positive),
                ..ALL
            }
        )
        .len(),
        1
    );
    assert!(
        titles(
            c,
            &TimelineFilter {
                impact: Some(Impact::Neutral),
                ..ALL
            }
        )
        .is_empty()
    );
    assert!(
        titles(
            c,
            &TimelineFilter {
                stock_id: Some(ids.samsung),
                impact: Some(Impact::Positive),
                ..ALL
            }
        )
        .is_empty()
    );
    assert_eq!(
        titles(
            c,
            &TimelineFilter {
                stock_id: Some(ids.samsung),
                impact: Some(Impact::Negative),
                ..ALL
            }
        )
        .len(),
        1
    );
}

#[test]
fn read_timeline_merges_same_title_within_24_hours() {
    // 제목이 같고 24시간 안이면 최근 기사 하나로 묶고, 나머지는 다른 언론사로
    let t = test_db_with_listings();
    let c = &t.conn;
    let ids = add_stocks(c);
    let link = |publisher: &str, hours: f64, title: &str| {
        Link::new(ids.samsung, ago(hours))
            .titled(title)
            .by(publisher)
            .score(8.0)
            .insert(c);
    };
    link("가경제", 1.0, "삼성전자 실적");
    link("나일보", 5.0, " 삼성전자  실적 ");
    link("다신문", 3.0, "삼성전자 실적");
    // 가장 최근 것과 24시간 넘게 떨어지면 따로
    link("라뉴스", 30.0, "삼성전자 실적");

    let cards = read(c, &ALL, 30).cards;
    let shape: Vec<(&str, Vec<&str>)> = cards
        .iter()
        .map(|card| {
            (
                card.publisher.as_str(),
                card.others.iter().map(|o| o.publisher.as_str()).collect(),
            )
        })
        .collect();
    assert_eq!(
        shape,
        [("가경제", vec!["다신문", "나일보"]), ("라뉴스", vec![])]
    );
}

#[test]
fn read_timeline_more_button_counts_and_merges_across_the_edge() {
    // '더 보기': 개수만큼 보여 주고 더 있는지 알려 주며, 경계 너머의 같은 제목도 묶는다
    let t = test_db_with_listings();
    let c = &t.conn;
    let ids = add_stocks(c);
    for i in 1..=31u32 {
        Link::new(ids.samsung, ago(f64::from(i)))
            .titled(&distinct(i))
            .score(8.0)
            .insert(c);
    }
    // 30번째 카드와 제목이 같은 기사가 31번째보다 뒤에 있다.
    Link::new(ids.samsung, ago(31.5))
        .titled(&distinct(30))
        .by("나일보")
        .score(8.0)
        .insert(c);

    let first = read(c, &ALL, 30);
    assert_eq!(first.cards.len(), 30);
    assert!(first.has_more);
    assert_eq!(first.cards[29].title, distinct(30));
    assert_eq!(first.cards[29].others.len(), 1);
    assert_eq!(first.cards[29].others[0].publisher, "나일보");

    let more = read(c, &ALL, 60);
    assert_eq!(more.cards.len(), 31);
    assert!(!more.has_more);
}

// ---- readTimeline: 11단계 ----

#[test]
fn read_timeline_needs_both_scores_and_old_evaluations_use_relevance_only() {
    // 관련도와 영향도가 둘 다 기준 이상이어야 보이고, 영향도가 없는 예전 평가는 관련도만 본다
    let t = test_db_with_listings();
    let c = &t.conn;
    let ids = add_stocks(c);
    // 경계 시나리오는 옛 기본값(영향도 6)으로 지킨다.
    write_num(c, &EVALUATION_MIN_MAGNITUDE, 6.0, now()).unwrap();
    let add = |title: &str, relevance: f64, magnitude: Option<f64>| {
        let mut link = Link::new(ids.samsung, ago(title.chars().count() as f64))
            .titled(title)
            .score(relevance);
        link.magnitude = magnitude;
        link.insert(c);
    };
    add("가", 8.0, Some(6.0));
    add("나나", 8.0, Some(5.9));
    add("다다다", 6.0, Some(9.0));
    add("라라라라", 7.0, None); // 11단계 전 평가

    assert_eq!(titles(c, &ALL), names(&["가", "다다다", "라라라라"]));
    let page = read(c, &ALL, 30);
    assert_eq!(page.cards[0].stocks[0].magnitude, Some(6.0));
    assert_eq!(page.cards[2].stocks[0].magnitude, None);
    write_num(c, &EVALUATION_MIN_MAGNITUDE, 7.0, now()).unwrap();
    assert_eq!(titles(c, &ALL), names(&["다다다", "라라라라"]));
}

#[test]
fn read_timeline_merges_similar_titles_and_collects_items() {
    // 비슷한 제목(같은 소식)은 한 카드로 묶고, 묶인 기사들의 항목을 모은다
    let t = test_db_with_listings();
    let c = &t.conn;
    let ids = add_stocks(c);
    Link::new(ids.samsung, ago(1.0))
        .titled("한은 기준금리 동결…연 2.5% 유지")
        .by("가경제")
        .score(7.0)
        .impact("negative")
        .insert(c);
    Link::new(ids.hynix, ago(2.0))
        .titled("[속보] 한은, 기준금리 동결…연 2.5% 유지")
        .by("나일보")
        .score(8.0)
        .impact("neutral")
        .insert(c);
    // 같은 종목이 두 기사에 걸렸으면 관련도가 높은 것 하나만
    Link::new(ids.samsung, ago(3.0))
        .titled("한국은행 기준금리 동결…연 2.5% 유지 결정")
        .by("다신문")
        .score(9.0)
        .impact("positive")
        .insert(c);
    Link::new(ids.samsung, ago(4.0))
        .titled("카카오페이 새 서비스")
        .score(8.0)
        .insert(c);

    let cards = read(c, &ALL, 30).cards;
    assert_eq!(
        cards.iter().map(|c| c.title.as_str()).collect::<Vec<_>>(),
        ["한은 기준금리 동결…연 2.5% 유지", "카카오페이 새 서비스"]
    );
    assert_eq!(
        cards[0]
            .others
            .iter()
            .map(|o| o.publisher.as_str())
            .collect::<Vec<_>>(),
        ["나일보", "다신문"]
    );
    let items: Vec<(&str, f64, Impact)> = cards[0]
        .stocks
        .iter()
        .map(|s| (s.name.as_str(), s.relevance, s.impact))
        .collect();
    assert_eq!(
        items,
        [
            ("삼성전자", 9.0, Impact::Positive),
            ("SK하이닉스", 8.0, Impact::Neutral)
        ]
    );
}

#[test]
fn read_timeline_topic_filter_sees_every_market_lens() {
    // 주제 필터는 그 주제의 증시별 항목을 모두 보고, 카드에 항목 종류를 담는다
    let t = test_db_with_listings();
    let c = &t.conn;
    let ids = add_stocks(c);
    let market = ensure_default_market(c, now()).unwrap().unwrap();
    set_market_enabled(c, Market::Us, true, now()).unwrap();
    let AddTopicResult::Added { topic_id, lens_ids } =
        add_user_topic(c, "부동산 시장", now()).unwrap()
    else {
        panic!("주제");
    };
    let (kr, us) = (lens_ids[0], lens_ids[1]);
    let (_, article_id) = Link::new(kr, ago(1.0))
        .titled("주택 공급 대책 발표")
        .keyword("부동산 시장 → 주택 공급")
        .score(7.0)
        .magnitude(7.0)
        .insert(c);
    Link::new(us, ago(1.0))
        .on(article_id)
        .keyword("부동산 시장 → 주택 공급")
        .score(6.5)
        .magnitude(6.0)
        .impact("positive")
        .insert(c);
    Link::new(market, ago(2.0))
        .titled("외국인 순매도 확대")
        .keyword("한국 증시 → 외국인 수급")
        .score(8.0)
        .magnitude(8.0)
        .insert(c);
    Link::new(ids.samsung, ago(3.0))
        .titled("삼성전자 실적")
        .score(8.0)
        .insert(c);

    assert_eq!(
        titles(
            c,
            &TimelineFilter {
                topic_id: Some(topic_id),
                ..ALL
            }
        ),
        names(&["주택 공급 대책 발표"])
    );
    assert_eq!(
        titles(
            c,
            &TimelineFilter {
                stock_id: Some(market),
                ..ALL
            }
        ),
        names(&["외국인 순매도 확대"])
    );
    let page = read(
        c,
        &TimelineFilter {
            topic_id: Some(topic_id),
            ..ALL
        },
        30,
    );
    let kinds: Vec<_> = page.cards[0]
        .stocks
        .iter()
        .map(|s| (s.kind, s.market, s.topic_id, s.topic_source))
        .collect();
    assert_eq!(
        kinds,
        [
            (
                ItemKind::Topic,
                Market::Kr,
                Some(topic_id),
                Some(TopicSource::User)
            ),
            (
                ItemKind::Topic,
                Market::Us,
                Some(topic_id),
                Some(TopicSource::User)
            ),
        ]
    );
    let second = &read(c, &ALL, 30).cards[1].stocks[0];
    assert_eq!(second.kind, ItemKind::Market);
    assert_eq!(second.name, "한국 증시");
    assert_eq!(second.topic_id, None);
}

// ---- readTimeline: 12단계 읽음 표시 ----

#[test]
fn read_timeline_card_is_read_when_any_member_is_read() {
    // 카드에 묶인 기사 가운데 하나라도 읽었으면 읽은 카드, 읽음을 되돌리면 안 읽은 카드
    let t = test_db_with_listings();
    let c = &t.conn;
    let ids = add_stocks(c);
    let (_, lone) = Link::new(ids.samsung, ago(1.0))
        .titled("삼성전자 새 공장")
        .score(8.0)
        .insert(c);
    let (_, first) = Link::new(ids.samsung, ago(2.0))
        .titled("한은 기준금리 동결…연 2.5% 유지")
        .by("가경제")
        .score(7.0)
        .insert(c);
    let (_, second) = Link::new(ids.samsung, ago(3.0))
        .titled("[속보] 한은, 기준금리 동결…연 2.5% 유지")
        .by("나일보")
        .score(7.0)
        .insert(c);
    let unread = || -> Vec<(String, bool)> {
        read(c, &ALL, 30)
            .cards
            .into_iter()
            .map(|card| (card.title, card.read))
            .collect()
    };
    let expect = |a: bool, b: bool| {
        vec![
            ("삼성전자 새 공장".to_string(), a),
            ("한은 기준금리 동결…연 2.5% 유지".to_string(), b),
        ]
    };

    assert_eq!(unread(), expect(false, false));
    // 묶인 기사 중 대표가 아닌 것만 읽어도 그 카드는 읽은 카드
    set_articles_read(c, &[second], true, now()).unwrap();
    assert_eq!(unread(), expect(false, true));
    set_articles_read(c, &[lone], true, now()).unwrap();
    assert_eq!(unread(), expect(true, true));
    set_articles_read(c, &[first, second], false, now()).unwrap();
    assert_eq!(unread(), expect(true, false));
}

#[test]
fn read_timeline_late_similar_article_joins_a_read_card() {
    // 이미 읽은 기사와 같은 소식이 나중에 묶여 들어오면 읽은 카드로 보인다
    let t = test_db_with_listings();
    let c = &t.conn;
    let ids = add_stocks(c);
    let (_, early) = Link::new(ids.samsung, ago(5.0))
        .titled("한은 기준금리 동결…연 2.5% 유지")
        .score(7.0)
        .insert(c);
    set_articles_read(c, &[early], true, now()).unwrap();
    Link::new(ids.samsung, ago(1.0))
        .titled("한은, 기준금리 동결…연 2.5% 유지 결정")
        .by("나일보")
        .score(7.0)
        .insert(c);
    let cards = read(c, &ALL, 30).cards;
    assert_eq!(cards.len(), 1);
    assert!(cards[0].read);
}

// ---- majorityImpact ----

fn scored(impact: Impact, relevance: f64, magnitude: Option<f64>) -> ScoredImpact {
    ScoredImpact {
        impact,
        relevance,
        magnitude,
    }
}

#[test]
fn majority_impact_takes_the_larger_side_then_the_strongest() {
    // 많은 쪽, 같으면 점수(관련도+영향도)가 가장 높은 평가 쪽
    use Impact::{Negative, Positive};
    let e = |impact| scored(impact, 7.0, Some(7.0));
    assert_eq!(majority_impact(&[]), None);
    assert_eq!(
        majority_impact(&[e(Positive), e(Negative), e(Negative)]),
        Some(Negative)
    );
    assert_eq!(
        majority_impact(&[
            scored(Positive, 9.0, Some(9.0)),
            scored(Negative, 8.0, Some(8.0))
        ]),
        Some(Positive)
    );
    assert_eq!(
        majority_impact(&[
            scored(Positive, 6.0, None),
            scored(Negative, 6.0, Some(1.0))
        ]),
        Some(Negative)
    );
}

// ---- readTopicCards (11단계) ----

fn auto_topic(conn: &Connection, name: &str) -> (crate::server::topics::store::Topic, Vec<i64>) {
    conn.execute(
        "INSERT INTO topics (source, name, summary, terms, created_at) VALUES ('auto', ?1, ?2, ?3, ?4)",
        params![name, format!("{name} 설명"), serde_json::to_string(&[name]).unwrap(), ago(80.0)],
    )
    .unwrap();
    let topic = find_topic(conn, conn.last_insert_rowid()).unwrap().unwrap();
    let lenses = sync_topic_lenses(conn, &topic, ago(80.0)).unwrap();
    (topic, lenses)
}

#[test]
fn read_topic_cards_lists_shown_auto_topics_and_fades_old_ones() {
    // 기준 이상 기사가 있는 자동 토픽만, 그런 기사가 많은 순으로. 12시간 넘게 새 기사가 없으면 접고 3일이 지나면 숨긴다
    let t = test_db_with_listings();
    let c = &t.conn;
    let ids = add_stocks(c);
    ensure_default_market(c, ago(80.0)).unwrap();
    let (mideast, mideast_lenses) = auto_topic(c, "중동 긴장");
    let (_, rates) = auto_topic(c, "기준금리");
    let (_, faded) = auto_topic(c, "미중 갈등");
    let (_, hidden) = auto_topic(c, "옛 토픽");
    let (_, weak) = auto_topic(c, "약한 토픽");
    let link = |lens: i64, hours: f64, relevance: f64, impact: &str| {
        let title = distinct((hours * 10.0).round() as u32 + lens as u32 * 1000);
        Link::new(lens, ago(hours))
            .titled(&title)
            .keyword("종목")
            .score(relevance)
            .magnitude(7.0)
            .impact(impact)
            .insert(c);
    };
    link(mideast_lenses[0], 1.0, 8.0, "negative");
    link(rates[0], 2.0, 8.0, "positive");
    link(rates[0], 3.0, 8.0, "positive");
    link(rates[0], 4.0, 8.0, "negative");
    link(rates[0], 5.0, 3.0, "negative"); // 기준 미만은 세지 않음
    link(faded[0], 13.0, 8.0, "negative");
    link(hidden[0], 73.0, 8.0, "negative");
    link(weak[0], 1.0, 4.0, "negative");
    // 사용자 주제는 토픽 카드가 아니다(필터 칩).
    let AddTopicResult::Added { lens_ids, .. } =
        add_user_topic(c, "부동산 시장", ago(80.0)).unwrap()
    else {
        panic!("주제");
    };
    link(lens_ids[0], 1.0, 9.0, "negative");

    // 삼성전자에 중동 긴장 가지를 붙인다.
    let samsung = crate::server::stocks::repository::list_active_stocks(c)
        .unwrap()
        .into_iter()
        .find(|s| s.id == ids.samsung)
        .unwrap();
    attach_topic_to_stock(
        c,
        &StockRef::from(&samsung),
        &mideast,
        &TopicLink {
            reason: "중동 수주",
            relevance: 7.0,
            magnitude: 6.0,
        },
        ago(10.0),
    )
    .unwrap();
    Link::new(ids.samsung, ago(1.0))
        .titled("사우디 수주 기대")
        .keyword("삼성전자 → 중동 긴장 → 중동 긴장")
        .score(7.0)
        .magnitude(7.0)
        .impact("positive")
        .insert(c);

    let cards = read_topic_cards(c, now()).unwrap();
    let shape: Vec<(&str, bool, usize)> = cards
        .iter()
        .map(|card| (card.name.as_str(), card.faded, card.shown_total))
        .collect();
    assert_eq!(
        shape,
        [
            ("기준금리", false, 3),
            ("중동 긴장", false, 1),
            ("미중 갈등", true, 1)
        ]
    );
    assert_eq!(cards[0].summary.as_deref(), Some("기준금리 설명"));
    assert_eq!(cards[0].shown_24h, 3);
    assert_eq!(
        cards[0].impacts,
        [MarketImpact {
            market: Market::Kr,
            impact: Impact::Positive
        }]
    );
    assert!(cards[0].related.is_empty());
    assert_eq!(cards[0].last_article_at, ago(2.0));
    assert_eq!(
        cards[1].related,
        [RelatedStock {
            stock_id: ids.samsung,
            name: "삼성전자".to_string(),
            impact: Some(Impact::Positive)
        }]
    );
}

// ---- parseTimelineParams ----

fn query(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
    pairs
        .iter()
        .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
        .collect()
}

#[test]
fn parse_timeline_params_ignores_unknown_values_and_rounds_limit() {
    // 모르는 종목·주제·영향은 무시하고, 개수는 30개 단위로 30~600
    let (filter, limit) = parse_timeline_params(
        &query(&[
            ("stock", "2"),
            ("topic", "7"),
            ("impact", "negative"),
            ("limit", "45"),
        ]),
        &[1, 2],
        &[7],
        TIMELINE_MAX,
    );
    assert_eq!(
        filter,
        TimelineFilter {
            stock_id: Some(2),
            topic_id: Some(7),
            impact: Some(Impact::Negative)
        }
    );
    assert_eq!(limit, 60);

    let (filter, limit) = parse_timeline_params(
        &query(&[
            ("stock", "9"),
            ("topic", "8"),
            ("impact", "great"),
            ("limit", "0"),
        ]),
        &[1],
        &[7],
        TIMELINE_MAX,
    );
    assert_eq!(filter, ALL);
    assert_eq!(limit, 30);
    assert_eq!(
        parse_timeline_params(&query(&[("limit", "99999")]), &[], &[], TIMELINE_MAX).1,
        TIMELINE_MAX
    );
    // 같은 이름이 여러 번이면 첫 값
    assert_eq!(
        parse_timeline_params(
            &query(&[("limit", "60"), ("limit", "90")]),
            &[],
            &[],
            TIMELINE_MAX
        )
        .1,
        60
    );
    assert_eq!(
        parse_timeline_params(&[], &[], &[], TIMELINE_MAX).1,
        TIMELINE_PAGE_SIZE
    );
}

#[test]
fn guest_max_caps_limit_and_members_keep_600() {
    let limit = |max: usize| parse_timeline_params(&query(&[("limit", "600")]), &[], &[], max).1;
    assert_eq!(limit(GUEST_TIMELINE_MAX), 90);
    assert_eq!(limit(TIMELINE_MAX), 600);
}

#[test]
fn parse_timeline_params_handles_odd_numbers() {
    // Rust에서만 생기는 경우: JS Number()가 읽는 모양과 못 읽는 모양
    let limit =
        |value: &str| parse_timeline_params(&query(&[("limit", value)]), &[], &[], TIMELINE_MAX).1;
    assert_eq!(limit("abc"), 30);
    assert_eq!(limit("-5"), 30);
    assert_eq!(limit("Infinity"), 30);
    assert_eq!(limit(" 31 "), 60);
    assert_eq!(limit(""), 30);
    let (filter, _) = parse_timeline_params(
        &query(&[("stock", "2.0"), ("topic", "")]),
        &[2],
        &[0],
        TIMELINE_MAX,
    );
    assert_eq!(filter.stock_id, Some(2));
    // Number("") === 0 이라 id 0이 있으면 통과한다(실제 id는 1부터라 일어나지 않음)
    assert_eq!(filter.topic_id, Some(0));
}

#[test]
fn read_topic_cards_leaves_out_company_topics() {
    let t = test_db_with_listings();
    let c = &t.conn;
    add_stocks(c);
    ensure_default_market(c, ago(80.0)).unwrap();
    let (_, market_lens) = auto_topic(c, "시장 토픽");
    let (company, company_lens) = auto_topic(c, "회사 토픽");
    c.execute(
        "UPDATE topics SET scope = 'company' WHERE id = ?1",
        params![company.id],
    )
    .unwrap();
    for (n, lens) in [(1u32, market_lens[0]), (2u32, company_lens[0])] {
        Link::new(lens, ago(1.0))
            .titled(&distinct(n))
            .keyword("종목")
            .score(8.0)
            .magnitude(7.0)
            .impact("positive")
            .insert(c);
    }
    let names: Vec<String> = read_topic_cards(c, now())
        .unwrap()
        .into_iter()
        .map(|card| card.name)
        .collect();
    assert_eq!(names, ["시장 토픽"]);
}

// ---- 숨김 규칙(18단계 display-rule) ----

fn set_bystander(conn: &Connection, match_id: i64, value: Option<f64>) {
    conn.execute(
        "UPDATE evaluations SET bystander = ?1 WHERE match_id = ?2",
        params![value, match_id],
    )
    .unwrap();
}

fn card_stock_ids(conn: &Connection, filter: &TimelineFilter, title: &str) -> Option<Vec<i64>> {
    read(conn, filter, TIMELINE_PAGE_SIZE)
        .cards
        .into_iter()
        .find(|card| card.title == title)
        .map(|card| card.stocks.iter().map(|s| s.stock_id).collect())
}

#[test]
fn hidden_rule_bystander_at_or_above_threshold_hides_the_stock_but_not_the_market() {
    // bystander 0.7 → 종목은 카드에서 빠지고 시장 항목이 기준 이상이면 카드는 시장으로 남음, 0.3 → 보임
    let t = test_db_with_listings();
    let c = &t.conn;
    let ids = add_stocks(c);
    let market = ensure_default_market(c, now()).unwrap().unwrap();
    let (m, article) = Link::new(ids.samsung, ago(1.0))
        .titled("숨길 기사")
        .score(7.0)
        .magnitude(7.0)
        .insert(c);
    set_bystander(c, m, Some(0.7));
    let (mm, _) = Link::new(market, ago(1.0))
        .on(article)
        .score(8.0)
        .magnitude(7.0)
        .insert(c);
    set_bystander(c, mm, Some(0.9)); // 시장 항목은 이 규칙을 받지 않음
    let (m2, _) = Link::new(ids.hynix, ago(2.0))
        .titled("보일 기사")
        .score(7.0)
        .magnitude(7.0)
        .insert(c);
    set_bystander(c, m2, Some(0.3));

    assert_eq!(card_stock_ids(c, &ALL, "숨길 기사"), Some(vec![market]));
    assert_eq!(card_stock_ids(c, &ALL, "보일 기사"), Some(vec![ids.hynix]));
    // 종목 필터에서도 같은 규칙
    let by_samsung = TimelineFilter {
        stock_id: Some(ids.samsung),
        ..ALL
    };
    assert!(titles(c, &by_samsung).is_empty());
    let by_hynix = TimelineFilter {
        stock_id: Some(ids.hynix),
        ..ALL
    };
    assert_eq!(titles(c, &by_hynix), names(&["보일 기사"]));
}

#[test]
fn hidden_rule_without_bystander_compares_with_the_market_relevance_of_the_same_article() {
    // bystander NULL: 시장 9 > 종목 7 → 빠짐, 시장 7 <= 종목 7 → 보임, 시장 평가 없음 → 보임
    let t = test_db_with_listings();
    let c = &t.conn;
    let ids = add_stocks(c);
    let market = ensure_default_market(c, now()).unwrap().unwrap();
    let (_, a) = Link::new(ids.samsung, ago(1.0))
        .titled("시장이 더 큼")
        .score(7.0)
        .magnitude(7.0)
        .insert(c);
    Link::new(market, ago(1.0))
        .on(a)
        .score(9.0)
        .magnitude(7.0)
        .insert(c);
    let (_, b) = Link::new(ids.hynix, ago(2.0))
        .titled("시장이 같음")
        .score(7.0)
        .magnitude(7.0)
        .insert(c);
    Link::new(market, ago(2.0))
        .on(b)
        .score(7.0)
        .magnitude(7.0)
        .insert(c);
    Link::new(ids.kakao, ago(3.0))
        .titled("시장 평가 없음")
        .score(7.0)
        .magnitude(7.0)
        .insert(c);

    assert_eq!(card_stock_ids(c, &ALL, "시장이 더 큼"), Some(vec![market]));
    assert_eq!(
        card_stock_ids(c, &ALL, "시장이 같음").map(|v| v.len()),
        Some(2)
    );
    assert_eq!(
        card_stock_ids(c, &ALL, "시장 평가 없음"),
        Some(vec![ids.kakao])
    );
    let by_samsung = TimelineFilter {
        stock_id: Some(ids.samsung),
        ..ALL
    };
    assert!(titles(c, &by_samsung).is_empty());
    let by_kakao = TimelineFilter {
        stock_id: Some(ids.kakao),
        ..ALL
    };
    assert_eq!(titles(c, &by_kakao), names(&["시장 평가 없음"]));
}

#[test]
fn hidden_rule_leaves_topic_items_alone_and_hides_the_stock_only_card_entirely() {
    // 숨겨진 종목 평가만 있는 기사는 카드가 없고, 주제 항목은 규칙을 받지 않는다
    let t = test_db_with_listings();
    let c = &t.conn;
    let ids = add_stocks(c);
    ensure_default_market(c, ago(80.0)).unwrap();
    let (m, _) = Link::new(ids.samsung, ago(1.0))
        .titled("종목만")
        .score(8.0)
        .magnitude(7.0)
        .insert(c);
    set_bystander(c, m, Some(0.8));
    let AddTopicResult::Added { lens_ids, .. } =
        add_user_topic(c, "부동산 시장", ago(80.0)).unwrap()
    else {
        panic!("주제");
    };
    let (tm, _) = Link::new(lens_ids[0], ago(2.0))
        .titled("주제 기사")
        .score(8.0)
        .magnitude(7.0)
        .insert(c);
    set_bystander(c, tm, Some(0.9));

    assert_eq!(titles(c, &ALL), names(&["주제 기사"]));
}

#[test]
fn read_topic_cards_apply_the_hidden_rule_to_attached_stocks_only() {
    // 토픽 항목 평가는 bystander 0.9여도 세고, 붙은 종목의 숨긴 평가(0.7)는 영향에서 빠진다
    let t = test_db_with_listings();
    let c = &t.conn;
    let ids = add_stocks(c);
    ensure_default_market(c, ago(80.0)).unwrap();
    let (mideast, lenses) = auto_topic(c, "중동 긴장");
    let (lm, _) = Link::new(lenses[0], ago(1.0))
        .titled(&distinct(1))
        .keyword("종목")
        .score(8.0)
        .magnitude(7.0)
        .impact("negative")
        .insert(c);
    set_bystander(c, lm, Some(0.9));
    let samsung = crate::server::stocks::repository::list_active_stocks(c)
        .unwrap()
        .into_iter()
        .find(|s| s.id == ids.samsung)
        .unwrap();
    attach_topic_to_stock(
        c,
        &StockRef::from(&samsung),
        &mideast,
        &TopicLink {
            reason: "중동 수주",
            relevance: 7.0,
            magnitude: 6.0,
        },
        ago(10.0),
    )
    .unwrap();
    let (sm, _) = Link::new(ids.samsung, ago(1.0))
        .titled("사우디 수주 기대")
        .keyword("삼성전자 → 중동 긴장 → 중동 긴장")
        .score(7.0)
        .magnitude(7.0)
        .impact("positive")
        .insert(c);
    set_bystander(c, sm, Some(0.7));

    let cards = read_topic_cards(c, now()).unwrap();
    assert_eq!(cards.len(), 1);
    assert_eq!(cards[0].shown_total, 1);
    assert_eq!(
        cards[0].related,
        [RelatedStock {
            stock_id: ids.samsung,
            name: "삼성전자".to_string(),
            impact: None
        }]
    );
}
