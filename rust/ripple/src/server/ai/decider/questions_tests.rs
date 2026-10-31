use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::{Value, json};
use tempfile::TempDir;

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

#[test]
fn fixture_file_equals_what_ts_dump_script_makes_now() {
    // 파일이 TS와 어긋나지 않는지 TS를 다시 돌려 확인한다. 질문 문구를 TS에서 고치면 여기서 알게 된다.
    let root: PathBuf = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap();
    let dir = TempDir::new().unwrap();
    let out = dir.path().join("fixture.json");
    let output = Command::new("pnpm")
        .args(["exec", "tsx", "rust/tools/dump-questions.ts"])
        .arg(&out)
        .current_dir(root)
        .output()
        .expect("pnpm을 실행할 수 없습니다");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(std::fs::read_to_string(out).unwrap(), FIXTURE);
}
