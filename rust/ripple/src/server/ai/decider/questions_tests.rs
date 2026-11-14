use serde_json::{Value, json};

use super::*;
use crate::server::ai::decider::types::{DecisionArticle, DecisionLink, LeadSubject};

const FIXTURE: &str = include_str!("questions_fixture.json");

fn samsung() -> JudgeItem {
    JudgeItem {
        kind: JudgeTarget::Stock,
        name: "삼성전자".into(),
        official_name: None,
        market: Market::Kr,
        exchange: "KOSPI".into(),
        code: "005930".into(),
        summary: None,
    }
}

fn samsung_keyword() -> DecisionInput {
    DecisionInput {
        stock: samsung(),
        link: DecisionLink {
            kind: MatchKind::Keyword,
            path: "삼성전자 → HBM 메모리".into(),
            reason: Some("주력 메모리 제품".into()),
        },
        article: DecisionArticle {
            title: "HBM 증설 경쟁".into(),
            summary: Some("메모리 업계가 증설에 나섰다.".into()),
        },
    }
}

fn kr_market() -> JudgeItem {
    JudgeItem {
        kind: JudgeTarget::Market,
        name: "한국 증시".into(),
        official_name: None,
        market: Market::Kr,
        exchange: "KOSPI·KOSDAQ".into(),
        code: "MARKET".into(),
        summary: None,
    }
}

fn instructions(question: &Value) -> &str {
    question["instructions"].as_str().unwrap()
}

fn criteria_len(question: &Value) -> usize {
    question["criteria"].as_array().unwrap().len()
}

// describe("buildArticleQuestions (SPEC FR-5, 11단계)")
#[test]
fn article_questions_are_six_level_scores_and_a_choice() {
    // it.each(JUDGE_TARGETS)("%s: 관련도·영향도는 상황 6단계 점수, 영향은 호재·악재·중립 선택")
    for target in JudgeTarget::ALL {
        let questions = build_article_questions(target);
        assert_eq!(questions["relevance"]["type"], "score", "{target:?}");
        assert_eq!(criteria_len(&questions["relevance"]), 6);
        assert_eq!(questions["magnitude"]["type"], "score");
        assert_eq!(criteria_len(&questions["magnitude"]), 6);
        assert_eq!(questions["impact"]["type"], "choice");
        let keys: Vec<&String> = questions["impact"]["criteria"]
            .as_object()
            .unwrap()
            .keys()
            .collect();
        assert_eq!(keys, ["positive", "negative", "neutral"]);
    }
}

#[test]
fn stock_asks_bystander_yes_no_after_impact_only_for_stock() {
    let stock = build_article_questions(JudgeTarget::Stock);
    let keys: Vec<&String> = stock.as_object().unwrap().keys().collect();
    assert_eq!(keys, ["relevance", "magnitude", "impact", "bystander"]);
    assert_eq!(stock["bystander"]["type"], "noul");
    assert_eq!(
        instructions(&stock["bystander"]),
        "Does the `article` mainly report how something else (a stock index, another company, or an institution) is affected, with the company in `stock` appearing only as a cause or in passing?"
    );
    assert!(stock["bystander"].get("criteria").is_none());
    for target in [JudgeTarget::Market, JudgeTarget::Topic] {
        assert!(build_article_questions(target).get("bystander").is_none());
    }
}

#[test]
fn stock_asks_about_company_market_and_topic_about_whole_market() {
    // it("종목은 그 종목을, 시장·주제는 증시 전체를 움직이는지 묻는다(주제는 증시 기준)")
    let stock = build_article_questions(JudgeTarget::Stock);
    assert!(instructions(&stock["magnitude"]).contains("company in `stock`"));
    for target in [JudgeTarget::Market, JudgeTarget::Topic] {
        let questions = build_article_questions(target);
        assert!(instructions(&questions["magnitude"]).contains("overall stock market"));
        assert!(instructions(&questions["impact"]).contains("overall stock market"));
    }
    let topic = build_article_questions(JudgeTarget::Topic);
    assert!(instructions(&topic["relevance"]).contains("topic in `topic`"));
}

// describe("buildLeadQuestions (11단계)")
#[test]
fn lead_questions_ask_only_two_scores() {
    // it.each(JUDGE_TARGETS)("%s: 관련도·영향도 두 점수만 묻는다")
    for target in JudgeTarget::ALL {
        let questions = build_lead_questions(target);
        let keys: Vec<&String> = questions.as_object().unwrap().keys().collect();
        assert_eq!(keys, ["relevance", "magnitude"]);
        assert_eq!(criteria_len(&questions["relevance"]), 6);
        assert_eq!(criteria_len(&questions["magnitude"]), 6);
    }
}

// describe("buildArticleState (D-21)")
#[test]
fn article_state_has_only_stock_link_path_reason_title_summary() {
    // it("종목·걸린 경위(경로와 이유)·제목과 요약만 넣는다")
    let state = build_article_state(&samsung_keyword());
    assert_eq!(
        state["stock"],
        json!({
            "name": "삼성전자", "market": "Korea", "exchange": "KOSPI", "code": "005930",
        })
    );
    let how = state["link"]["how"].as_str().unwrap();
    assert!(how.contains("keyword"));
    assert_eq!(state["link"]["path"], "삼성전자 → HBM 메모리");
    assert_eq!(state["link"]["reason"], "주력 메모리 제품");
    assert_eq!(
        state["article"],
        json!({
            "title": "HBM 증설 경쟁", "summary": "메모리 업계가 증설에 나섰다.",
        })
    );
    assert_eq!(state.as_object().unwrap().len(), 3);
}

#[test]
fn us_stock_adds_official_name_and_drops_missing_reason_and_summary() {
    // it("미국 종목은 공식 영문 이름을 함께 넣고, 없는 이유·요약은 뺀다")
    let state = build_article_state(&DecisionInput {
        stock: JudgeItem {
            kind: JudgeTarget::Stock,
            name: "엔비디아".into(),
            official_name: Some("NVIDIA Corporation - Common Stock".into()),
            market: Market::Us,
            exchange: "NASDAQ".into(),
            code: "NVDA".into(),
            summary: None,
        },
        link: DecisionLink {
            kind: MatchKind::Direct,
            path: "엔비디아".into(),
            reason: None,
        },
        article: DecisionArticle {
            title: "Nvidia climbs".into(),
            summary: None,
        },
    });
    assert_eq!(
        state["stock"],
        json!({
            "name": "엔비디아",
            "official_name": "NVIDIA Corporation - Common Stock",
            "market": "United States",
            "exchange": "NASDAQ",
            "code": "NVDA",
        })
    );
    assert_eq!(
        state["link"],
        json!({
            "how": "The news provider tagged this article as news about this company.",
            "path": "엔비디아",
        })
    );
    assert_eq!(state["article"], json!({ "title": "Nvidia climbs" }));
}

#[test]
fn market_state_has_market_name_and_topic_state_has_topic_and_market() {
    // it("시장은 증시 이름만, 주제는 주제(설명)와 기준 증시를 넣는다(11단계)")
    let article = || DecisionArticle {
        title: "기준금리 동결".into(),
        summary: None,
    };
    let link = || DecisionLink {
        kind: MatchKind::Keyword,
        path: "한국 증시 → 기준금리".into(),
        reason: None,
    };
    let market = build_article_state(&DecisionInput {
        stock: kr_market(),
        link: link(),
        article: article(),
    });
    assert_eq!(
        market["market"],
        json!({ "name": "Korean stock market (KOSPI, KOSDAQ)" })
    );
    assert_eq!(market["link"]["path"], "한국 증시 → 기준금리");
    assert!(market["link"]["how"].is_string());
    assert_eq!(market["article"], json!({ "title": "기준금리 동결" }));

    let topic = build_article_state(&DecisionInput {
        stock: JudgeItem {
            kind: JudgeTarget::Topic,
            name: "중동 긴장".into(),
            official_name: None,
            market: Market::Us,
            exchange: "미국 증시".into(),
            code: "TOPIC-1".into(),
            summary: Some("사우디 시설 공격 위협".into()),
        },
        link: link(),
        article: article(),
    });
    assert_eq!(
        topic["topic"],
        json!({ "name": "중동 긴장", "summary": "사우디 시설 공격 위협" })
    );
    assert_eq!(
        topic["market"],
        json!({ "name": "US stock market (S&P 500, Nasdaq)" })
    );
    assert!(topic.get("stock").is_none());
}

// describe("buildLeadState (11단계)")
#[test]
fn lead_state_has_path_and_reason_for_keyword_and_summary_and_terms_for_topic() {
    // it("키워드는 경로와 이유를, 토픽은 설명과 검색어를 넣는다")
    let keyword = build_lead_state(&LeadInput {
        stock: samsung(),
        subject: LeadSubject {
            kind: LeadSubjectType::Keyword,
            text: "엔비디아".into(),
            path: Some("삼성전자 → HBM → 엔비디아".into()),
            summary: None,
            terms: None,
            reason: Some("HBM 주요 고객".into()),
        },
    });
    assert_eq!(
        keyword["subject"],
        json!({
            "type": "search keyword",
            "text": "엔비디아",
            "path": "삼성전자 → HBM → 엔비디아",
            "reason": "HBM 주요 고객",
        })
    );
    let topic = build_lead_state(&LeadInput {
        stock: samsung(),
        subject: LeadSubject {
            kind: LeadSubjectType::Topic,
            text: "미중 반도체 갈등".into(),
            path: None,
            summary: Some("미국이 수출 통제를 넓힘".into()),
            terms: Some(vec!["수출 통제".into(), "반도체 관세".into()]),
            reason: None,
        },
    });
    assert_eq!(
        topic["subject"],
        json!({
            "type": "market topic",
            "text": "미중 반도체 갈등",
            "summary": "미국이 수출 통제를 넓힘",
            "search_terms": ["수출 통제", "반도체 관세"],
        })
    );
}

// 설계 R2 완료 기준: 질문 문구가 글자 하나까지 같음(TS가 만든 값과 비교)
fn compare_with_fixture(fixture: &Value) {
    for target in JudgeTarget::ALL {
        let name = target.as_str();
        assert_eq!(
            build_article_questions(target).to_string(),
            fixture["articleQuestions"][name].to_string(),
            "기사 질문 {name}"
        );
        assert_eq!(
            build_lead_questions(target).to_string(),
            fixture["leadQuestions"][name].to_string(),
            "키워드·토픽 질문 {name}"
        );
    }
    for (index, entry) in fixture["articles"].as_array().unwrap().iter().enumerate() {
        let input: DecisionInput = serde_json::from_value(entry["input"].clone()).unwrap();
        assert_eq!(
            build_article_state(&input).to_string(),
            entry["state"].to_string(),
            "기사 state #{index}"
        );
    }
    for (index, entry) in fixture["leads"].as_array().unwrap().iter().enumerate() {
        let input: LeadInput = serde_json::from_value(entry["input"].clone()).unwrap();
        assert_eq!(
            build_lead_state(&input).to_string(),
            entry["state"].to_string(),
            "키워드·토픽 state #{index}"
        );
    }
}

#[test]
fn questions_and_states_equal_ts_output_character_by_character() {
    compare_with_fixture(&serde_json::from_str(FIXTURE).unwrap());
}

// 17단계 eval-merge: 기사당 한 요청
fn merged_input(
    topics: &[(&str, Option<&str>)],
    market_items: &[Market],
    markets: &[Market],
) -> MergedInput {
    MergedInput {
        topics: topics
            .iter()
            .map(
                |(name, summary)| crate::server::ai::decider::types::MergedTopic {
                    name: (*name).into(),
                    summary: summary.map(Into::into),
                },
            )
            .collect(),
        market_items: market_items.to_vec(),
        markets: markets.to_vec(),
        article: DecisionArticle {
            title: "금리 동결".into(),
            summary: Some("한은이 동결했다.".into()),
        },
    }
}

fn merged_topic_rel() -> Value {
    json!([
        "Unrelated to the topic; the keyword match is a coincidence.",
        "Mentions the topic only in passing.",
        "Background or commentary about the topic.",
        "A concrete development in the topic.",
        "An important development in the topic.",
        "The central news of the topic."
    ])
}

fn merged_market_rel() -> Value {
    json!([
        "Unrelated to the stock market.",
        "About a single company or a narrow niche, with little to do with the market as a whole.",
        "About one industry or sector.",
        "About an economy-wide factor (interest rates, currency, prices, trade, policy, geopolitics) or several large sectors.",
        "About the direction of the market itself, or a factor the whole market is watching right now.",
        "Central to the whole market right now."
    ])
}

fn merged_market_mag() -> Value {
    json!([
        "No effect on the overall market.",
        "A negligible effect.",
        "A small or short-lived effect on some sectors.",
        "A noticeable effect on the market or several large sectors for a day or two.",
        "A significant effect that changes market expectations, such as an interest-rate decision or a major policy or geopolitical event.",
        "A major effect likely to move the whole market substantially."
    ])
}

fn merged_impact(key: &str) -> Value {
    json!({
        "type": "choice",
        "instructions": format!("What effect does the `article` have on the overall stock market in `markets.{key}`?"),
        "criteria": {
            "positive": "Good news",
            "negative": "Bad news",
            "neutral": "No effect, or the direction is unclear"
        }
    })
}

fn merged_score(instructions: String, criteria: Value) -> Value {
    json!({ "type": "score", "instructions": instructions, "criteria": criteria })
}

#[test]
fn merged_article_with_two_topics_and_a_kr_market_item_has_the_measured_shape() {
    let input = merged_input(
        &[("AI 투자", Some("빅테크 투자 확대")), ("금리", None)],
        &[Market::Kr],
        &[Market::Kr, Market::Us],
    );
    assert_eq!(
        build_merged_state(&input),
        json!({
            "topics": {
                "t1": { "name": "AI 투자", "summary": "빅테크 투자 확대" },
                "t2": { "name": "금리" }
            },
            "markets": {
                "kr": { "name": "Korean stock market (KOSPI, KOSDAQ)" },
                "us": { "name": "US stock market (S&P 500, Nasdaq)" }
            },
            "article": { "title": "금리 동결", "summary": "한은이 동결했다." }
        })
    );
    let mag = |key: &str| {
        merged_score(
            format!(
                "How much could the news in the `article` move the overall stock market in `markets.{key}`?"
            ),
            merged_market_mag(),
        )
    };
    let order: Vec<String> = build_merged_questions(&input)
        .as_object()
        .unwrap()
        .keys()
        .cloned()
        .collect();
    assert_eq!(
        order,
        [
            "relevance_t1",
            "relevance_t2",
            "relevance_kr",
            "magnitude_kr",
            "magnitude_us",
            "impact_kr",
            "impact_us"
        ]
    );
    assert_eq!(
        build_merged_questions(&input),
        json!({
            "relevance_t1": merged_score("How directly is the `article` about the topic in `topics.t1`?".into(), merged_topic_rel()),
            "relevance_t2": merged_score("How directly is the `article` about the topic in `topics.t2`?".into(), merged_topic_rel()),
            "relevance_kr": merged_score("How directly is the `article` about the overall stock market in `markets.kr`?".into(), merged_market_rel()),
            "magnitude_kr": mag("kr"),
            "magnitude_us": mag("us"),
            "impact_kr": merged_impact("kr"),
            "impact_us": merged_impact("us"),
        })
    );
}

#[test]
fn merged_article_with_one_market_asks_only_that_market_and_omits_empty_topics() {
    let input = merged_input(&[], &[Market::Us], &[Market::Us]);
    let state = build_merged_state(&input);
    assert!(state.get("topics").is_none());
    assert_eq!(state["markets"].as_object().unwrap().len(), 1);
    assert!(state["markets"]["us"].is_object());
    let questions = build_merged_questions(&input);
    let keys: Vec<&String> = questions.as_object().unwrap().keys().collect();
    assert_eq!(keys, ["relevance_us", "magnitude_us", "impact_us"]);
}
