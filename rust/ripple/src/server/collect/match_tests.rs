use super::*;

fn keyword(
    level: i64,
    text_ko: &str,
    text_en: Option<&str>,
    path: &str,
    reason: &str,
) -> MatchKeyword {
    MatchKeyword {
        level,
        text_ko: text_ko.into(),
        text_en: text_en.map(str::to_string),
        path: path.into(),
        reason: reason.into(),
    }
}

fn samsung() -> MatchStock {
    MatchStock {
        id: 1,
        kind: ItemKind::Stock,
        market: Market::Kr,
        code: "005930".into(),
        name: "삼성전자".into(),
        name_ko: Some("삼성전자".into()),
        aliases: vec![],
        anchors: vec!["hbm".into()],
        keywords: vec![
            keyword(
                1,
                "HBM 메모리",
                None,
                "삼성전자 → HBM 메모리",
                "주력 메모리 제품",
            ),
            keyword(
                1,
                "파운드리 수주",
                None,
                "삼성전자 → 파운드리 수주",
                "위탁 생산 사업",
            ),
            keyword(
                2,
                "엔비디아",
                None,
                "삼성전자 → HBM 메모리 → 엔비디아",
                "HBM 주요 고객",
            ),
        ],
    }
}

fn nvidia() -> MatchStock {
    MatchStock {
        id: 2,
        kind: ItemKind::Stock,
        market: Market::Us,
        code: "NVDA".into(),
        name: "NVIDIA Corporation - Common Stock".into(),
        name_ko: Some("엔비디아".into()),
        aliases: vec![],
        anchors: vec!["accelerator".into(), "가속기".into(), "hbm".into()],
        keywords: vec![
            keyword(
                1,
                "AI 가속기",
                Some("AI accelerator"),
                "엔비디아 → AI 가속기",
                "주력 제품",
            ),
            keyword(
                2,
                "HBM 메모리",
                Some("HBM memory"),
                "엔비디아 → AI 가속기 → HBM 메모리",
                "가속기 핵심 부품",
            ),
        ],
    }
}

fn matcher() -> Matcher {
    create_matcher(&[samsung(), nvidia()])
}

fn by_name(stock_id: i64, path: &str) -> StockMatch {
    StockMatch {
        stock_id,
        keyword_path: path.into(),
        keyword_reason: None,
        match_kind: MatchKind::Name,
        level: 0,
        gated: false,
    }
}

fn by_keyword(stock_id: i64, path: &str, reason: &str, level: i64) -> StockMatch {
    StockMatch {
        stock_id,
        keyword_path: path.into(),
        keyword_reason: Some(reason.into()),
        match_kind: MatchKind::Keyword,
        level,
        gated: false,
    }
}

// describe("createMatcher (4단계 결정: 낱말이 모두 들어 있으면 연결)")
#[test]
fn stock_name_matches_as_the_stock_itself_level_0() {
    // it("종목 이름이 있으면 종목 자체(0단계)로 건다")
    assert_eq!(
        matcher().find("삼성전자, 3분기 실적 발표", ""),
        [by_name(1, "삼성전자")]
    );
}

#[test]
fn keyword_words_may_be_apart_or_have_particles_and_carry_the_reason() {
    // it("키워드의 낱말이 떨어져 있거나 조사가 붙어도 모두 있으면 걸고, 그 키워드의 이유를 함께 준다")
    assert_eq!(
        matcher().find("메모리 가격 급등에 HBM이 견인", ""),
        [
            by_keyword(1, "삼성전자 → HBM 메모리", "주력 메모리 제품", 1),
            by_keyword(
                2,
                "엔비디아 → AI 가속기 → HBM 메모리",
                "가속기 핵심 부품",
                2
            ),
        ]
    );
    // 낱말 하나만 있으면 걸지 않는다.
    assert_eq!(matcher().find("메모리 시장 전망", ""), []);
}

#[test]
fn closest_path_per_stock_name_then_level_1_then_level_2() {
    // it("종목마다 가장 가까운 경로 하나: 종목 이름 > 1단계 > 2단계")
    // '엔비디아'는 삼성전자의 2단계이자 엔비디아 종목 이름이다.
    assert_eq!(
        matcher().find("엔비디아 HBM 메모리 주문", ""),
        [
            by_keyword(1, "삼성전자 → HBM 메모리", "주력 메모리 제품", 1),
            by_name(2, "엔비디아")
        ]
    );
}

#[test]
fn english_keywords_match_with_word_boundaries() {
    // it("영어 키워드도 찾고, 영문 낱말은 낱말 경계를 지킨다")
    assert_eq!(
        matcher().find("Demand for the new AI accelerator soars", ""),
        [by_keyword(2, "엔비디아 → AI 가속기", "주력 제품", 1)]
    );
    // 'ai'가 'said', 'accelerators'의 일부로 걸리면 안 된다.
    assert_eq!(matcher().find("He said accelerators are hot", ""), []);
    // 한글 조사가 붙은 영문은 찾는다.
    assert_eq!(
        matcher().find("AI가 가속기 시장을 키운다", ""),
        [by_keyword(2, "엔비디아 → AI 가속기", "주력 제품", 1)]
    );
}

#[test]
fn case_and_whitespace_are_ignored() {
    // it("대소문자와 공백은 무시한다")
    assert_eq!(matcher().find("hbm   메모리", "").len(), 2);
}

#[test]
fn stock_without_keywords_matches_by_name_only() {
    // it("키워드가 없는 종목은 이름으로만 건다")
    let mut only_name = samsung();
    only_name.keywords.clear();
    let matcher = create_matcher(&[only_name]);
    assert_eq!(matcher.find("HBM 메모리", ""), []);
    assert_eq!(matcher.find("삼성전자", "").len(), 1);
}

#[test]
fn market_and_topic_items_match_by_keywords_only() {
    // it("시장·주제 항목은 이름으로 걸지 않고 키워드로만 건다(11단계)")
    let market = create_matcher(&[MatchStock {
        id: 7,
        kind: ItemKind::Market,
        market: Market::Kr,
        code: "MARKET".into(),
        name: "한국 증시".into(),
        name_ko: Some("한국 증시".into()),
        aliases: vec![],
        anchors: vec![],
        keywords: vec![keyword(
            1,
            "기준금리",
            None,
            "한국 증시 → 기준금리",
            "시장 금리",
        )],
    }]);
    assert_eq!(market.find("한국 증시 마감 시황", ""), []);
    assert_eq!(
        market.find("한은 기준금리 동결", ""),
        [by_keyword(7, "한국 증시 → 기준금리", "시장 금리", 1)]
    );
}

#[test]
fn word_boundary_checks_each_occurrence_and_non_ascii_words_use_plain_contains() {
    // Rust에서만 생기는 경우: 첫 번째 위치가 경계에 걸려도 다음 위치를 본다(JS 정규식의 탐색), 숫자·구두점 낱말
    assert!(contains_word("said ai here", "ai"));
    assert!(!contains_word("said maid", "ai"));
    assert!(contains_word("ai", "ai"));
    assert!(contains_word("3.5% growth", "3.5"));
    assert!(!contains_word("a35 growth", "35"));
    assert!(contains_word("c++ and c#", "c++"));
    // 한글 낱말은 포함 여부만 본다.
    assert!(contains_word("메모리가격", "메모리"));
    // 첫 위치가 경계에 걸려도 다음 위치를 본다.
    assert!(contains_word("xaa aa", "aa"));
    assert!(!contains_word("aaa b", "aa"));
    assert!(!contains_word("aaab", "aa"));
}

#[test]
fn words_are_trimmed_of_edge_punctuation_and_empty_terms_never_match() {
    // Rust에서만 생기는 경우: `toWords`가 낱말 끝 구두점을 떼고, 빈 낱말만 있는 항목은 걸리지 않는다.
    assert_eq!(to_words("(HBM) 메모리,"), ["hbm", "메모리"]);
    assert_eq!(to_words("  "), Vec::<String>::new());
    assert_eq!(to_words("---"), Vec::<String>::new());
    let weird = create_matcher(&[MatchStock {
        id: 9,
        kind: ItemKind::Topic,
        market: Market::Kr,
        code: "T".into(),
        name: "x".into(),
        name_ko: None,
        aliases: vec![],
        anchors: vec![],
        keywords: vec![keyword(1, "!!!", None, "p", "")],
    }]);
    assert_eq!(weird.find("anything !!!", ""), []);
}

#[test]
fn empty_reason_becomes_none() {
    // `keyword.reason || null`
    let matcher = create_matcher(&[MatchStock {
        id: 3,
        kind: ItemKind::Topic,
        market: Market::Kr,
        code: "T".into(),
        name: "주제".into(),
        name_ko: None,
        aliases: vec![],
        anchors: vec![],
        keywords: vec![keyword(1, "금리", None, "주제 → 금리", "")],
    }]);
    let found = matcher.find("금리 인상", "");
    assert_eq!(found[0].keyword_reason, None);
}

fn with_anchors(aliases: &[&str], anchors: &[&str]) -> Matcher {
    let mut stock = samsung();
    stock.aliases = aliases.iter().map(|a| a.to_string()).collect();
    stock.anchors = anchors.iter().map(|a| a.to_string()).collect();
    create_matcher(&[stock])
}

#[test]
fn alias_in_title_or_summary_links_by_name_with_stock_name_path() {
    let matcher = with_anchors(&["삼전", "Samsung Elec"], &[]);
    assert_eq!(matcher.find("삼전 급등", ""), [by_name(1, "삼성전자")]);
    assert_eq!(
        matcher.find("반도체 소식", "samsung elec 발표"),
        [by_name(1, "삼성전자")]
    );
}

#[test]
fn keyword_hit_without_anchor_in_title_is_gated() {
    let matcher = with_anchors(&[], &["삼성"]);
    let found = matcher.find("파운드리 수주 늘어", "");
    assert_eq!(found.len(), 1);
    assert!(found[0].gated);
    assert_eq!(found[0].match_kind, MatchKind::Keyword);
    // 요약에만 고유어가 있으면 걸러진다.
    let found = matcher.find("파운드리 수주 늘어", "삼성 관련");
    assert!(found[0].gated);
}

#[test]
fn keyword_hit_with_anchor_in_title_is_not_gated() {
    let matcher = with_anchors(&[], &["삼성"]);
    assert!(!matcher.find("삼성 파운드리 수주", "")[0].gated);
    // 대소문자·공백을 무시하고 낱말 경계 없이 포함으로 본다.
    let matcher = with_anchors(&[], &["HBM"]);
    assert!(!matcher.find("차세대 hbm4 양산, 파운드리 수주", "")[0].gated);
    let matcher = with_anchors(&[], &["삼 성"]);
    assert!(!matcher.find("삼성 파운드리 수주", "")[0].gated);
}

#[test]
fn empty_anchors_gate_keyword_hits_but_never_name_hits() {
    let matcher = with_anchors(&[], &[]);
    assert!(matcher.find("파운드리 수주", "")[0].gated);
    assert!(!matcher.find("삼성전자 파운드리 수주", "")[0].gated);
}

#[test]
fn market_and_topic_items_are_never_gated() {
    for kind in [ItemKind::Market, ItemKind::Topic] {
        let matcher = create_matcher(&[MatchStock {
            id: 7,
            kind,
            market: Market::Kr,
            code: "X".into(),
            name: "항목".into(),
            name_ko: None,
            aliases: vec![],
            anchors: vec![],
            keywords: vec![keyword(1, "기준금리", None, "항목 → 기준금리", "")],
        }]);
        assert!(!matcher.find("기준금리 동결", "")[0].gated);
    }
}
