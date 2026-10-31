//! Jev에 묻는 질문(SPEC FR-5, 11단계 결정). 판단을 Jev 한 곳에 모은다. 문구는 `questions.ts`와 글자까지 같아야 한다
//! (테스트가 TS로 만든 값을 파일로 뽑아 비교한다).
//! - 기사: 관련도·영향도·호재/악재를 한 요청에 묻는다.
//! - 키워드·토픽(트리 후보, 토픽×종목 쌍): 관련도·영향도를 묻는다.
//!
//! 점수는 상황 6단계(0~5)로 묻고 ×2 해서 0~10으로 쓴다(Jev Score는 최대 10단계, 2026-10-20 결정).
//! 대상은 종목(stock)·시장(market)·주제(topic: 증시 기준으로 판단)다.
use serde_json::{Map, Value, json};

use crate::server::ai::decider::types::{
    DecisionInput, JudgeItem, JudgeTarget, LeadInput, LeadSubjectType,
};
use crate::server::db::schema::{Market, MatchKind};

/// 0~5단계 기댓값을 화면 점수 0~10으로 바꾸는 배수
pub const RELEVANCE_SCALE: f64 = 2.0;

/// 판단 대상을 질문에서 부르는 말
fn target_text(target: JudgeTarget) -> &'static str {
    match target {
        JudgeTarget::Stock => "the company in `stock`",
        JudgeTarget::Market => "the overall stock market in `market`",
        JudgeTarget::Topic => "the topic in `topic`",
    }
}

/// 영향도는 종목이면 그 종목, 시장·주제면 증시 전체(주제는 증시 기준, 11단계 결정)
fn moves_text(target: JudgeTarget) -> &'static str {
    match target {
        JudgeTarget::Stock => "the earnings or share price of the company in `stock`",
        JudgeTarget::Market | JudgeTarget::Topic => "the overall stock market in `market`",
    }
}

fn article_relevance_levels(target: JudgeTarget) -> [&'static str; 6] {
    match target {
        JudgeTarget::Stock => [
            "Unrelated to this company; the name or keyword match is a coincidence.",
            "Touches this company's field only in passing.",
            "About this company's industry or the wider economy, not this company in particular.",
            "About something connected to this company's business: its products, customers, competitors, suppliers, materials, or rules that apply to it.",
            "About this company together with other companies.",
            "Mainly about this company itself.",
        ],
        JudgeTarget::Market => [
            "Unrelated to the stock market.",
            "About a single company or a narrow niche, with little to do with the market as a whole.",
            "About one industry or sector.",
            "About an economy-wide factor (interest rates, currency, prices, trade, policy, geopolitics) or several large sectors.",
            "About the direction of the market itself, or a factor the whole market is watching right now.",
            "Central to the whole market right now.",
        ],
        JudgeTarget::Topic => [
            "Unrelated to the topic; the keyword match is a coincidence.",
            "Mentions the topic only in passing.",
            "Background or commentary about the topic.",
            "A concrete development in the topic.",
            "An important development in the topic.",
            "The central news of the topic.",
        ],
    }
}

const STOCK_MAGNITUDE_LEVELS: [&str; 6] = [
    "No effect on this company's earnings or share price.",
    "A negligible effect.",
    "A small or short-lived effect.",
    "A noticeable effect, such as a move of a few percent or a change to one quarter's results.",
    "A significant effect that changes the outlook, such as a major order, a guidance change, or a new policy.",
    "A major effect likely to move the share price substantially.",
];

const MARKET_MAGNITUDE_LEVELS: [&str; 6] = [
    "No effect on the overall market.",
    "A negligible effect.",
    "A small or short-lived effect on some sectors.",
    "A noticeable effect on the market or several large sectors for a day or two.",
    "A significant effect that changes market expectations, such as an interest-rate decision or a major policy or geopolitical event.",
    "A major effect likely to move the whole market substantially.",
];

fn magnitude_levels(target: JudgeTarget) -> [&'static str; 6] {
    match target {
        JudgeTarget::Stock => STOCK_MAGNITUDE_LEVELS,
        JudgeTarget::Market | JudgeTarget::Topic => MARKET_MAGNITUDE_LEVELS,
    }
}

/// 트리 후보 키워드·토픽이 대상과 얼마나 이어지는가
const LEAD_RELEVANCE_LEVELS: [&str; 6] = [
    "Not connected.",
    "Connected only through several unlikely steps.",
    "Indirectly connected through the wider economy or industry.",
    "Clearly connected through the business, customers, competitors, inputs, or rules involved.",
    "A direct driver.",
    "Core to it, or a direct catalyst.",
];

fn link_kind_text(kind: MatchKind) -> &'static str {
    match kind {
        MatchKind::Direct => "The news provider tagged this article as news about this company.",
        MatchKind::Name => "The article mentions the company name.",
        MatchKind::Keyword => {
            "The article mentions a search keyword. path is the chain from the target to the keyword; reason explains how the keyword relates to the previous step."
        }
    }
}

fn score(instructions: String, criteria: &[&str]) -> Value {
    json!({ "type": "score", "instructions": instructions, "criteria": criteria })
}

/// 기사 질문: 관련도·영향도(점수)와 호재/악재(선택)
pub fn build_article_questions(target: JudgeTarget) -> Value {
    json!({
        "relevance": score(
            format!(
                "How directly is the `article` about {}? `link` explains how the article was matched.",
                target_text(target)
            ),
            &article_relevance_levels(target),
        ),
        "magnitude": score(
            format!("How much could the news in the `article` move {}?", moves_text(target)),
            &magnitude_levels(target),
        ),
        "impact": {
            "type": "choice",
            "instructions": format!("What effect does the `article` have on {}?", moves_text(target)),
            "criteria": {
                "positive": "Good news",
                "negative": "Bad news",
                "neutral": "No effect, or the direction is unclear",
            },
        },
    })
}

/// 키워드·토픽 질문: 트리에 남길지, 종목에 붙일지 판단하는 관련도·영향도(점수)
pub fn build_lead_questions(target: JudgeTarget) -> Value {
    json!({
        "relevance": score(
            format!(
                "How closely is news about the `subject` connected to {}? `subject.reason` explains the connection.",
                target_text(target)
            ),
            &LEAD_RELEVANCE_LEVELS,
        ),
        "magnitude": score(
            format!(
                "If there is important news about the `subject`, how much could it move {}?",
                moves_text(target)
            ),
            &magnitude_levels(target),
        ),
    })
}

/// 대상 항목을 state에 넣는 모양: 종목은 stock, 시장은 market, 주제는 topic과 기준 증시(market)
fn target_state(item: &JudgeItem) -> Map<String, Value> {
    let market_name = match item.market {
        Market::Kr => "Korean stock market (KOSPI, KOSDAQ)",
        Market::Us => "US stock market (S&P 500, Nasdaq)",
    };
    let mut state = Map::new();
    match item.kind {
        JudgeTarget::Market => {
            state.insert("market".into(), json!({ "name": market_name }));
        }
        JudgeTarget::Topic => {
            let mut topic = Map::new();
            topic.insert("name".into(), json!(item.name));
            if let Some(summary) = item.summary.as_deref().filter(|s| !s.is_empty()) {
                topic.insert("summary".into(), json!(summary));
            }
            state.insert("topic".into(), Value::Object(topic));
            state.insert("market".into(), json!({ "name": market_name }));
        }
        JudgeTarget::Stock => {
            let mut stock = Map::new();
            stock.insert("name".into(), json!(item.name));
            if let Some(official) = item
                .official_name
                .as_deref()
                .filter(|o| !o.is_empty() && *o != item.name)
            {
                stock.insert("official_name".into(), json!(official));
            }
            stock.insert(
                "market".into(),
                json!(match item.market {
                    Market::Kr => "Korea",
                    Market::Us => "United States",
                }),
            );
            stock.insert("exchange".into(), json!(item.exchange));
            stock.insert("code".into(), json!(item.code));
            state.insert("stock".into(), Value::Object(stock));
        }
    }
    state
}

/// 기사 평가 state: 대상, 걸린 경위, 기사(제목+요약). 판단에 필요 없는 것(날짜 등)은 넣지 않는다(D-21).
pub fn build_article_state(input: &DecisionInput) -> Value {
    let DecisionInput {
        stock,
        link,
        article,
    } = input;
    let mut state = target_state(stock);
    let mut link_state = Map::new();
    link_state.insert("how".into(), json!(link_kind_text(link.kind)));
    link_state.insert("path".into(), json!(link.path));
    if let Some(reason) = link.reason.as_deref().filter(|r| !r.is_empty()) {
        link_state.insert("reason".into(), json!(reason));
    }
    state.insert("link".into(), Value::Object(link_state));
    let mut article_state = Map::new();
    article_state.insert("title".into(), json!(article.title));
    if let Some(summary) = article.summary.as_deref().filter(|s| !s.is_empty()) {
        article_state.insert("summary".into(), json!(summary));
    }
    state.insert("article".into(), Value::Object(article_state));
    Value::Object(state)
}

/// 키워드·토픽 평가 state: 대상과 판단할 것(키워드면 경로, 토픽이면 설명·검색어)과 이어지는 이유
pub fn build_lead_state(input: &LeadInput) -> Value {
    let subject = &input.subject;
    let mut state = target_state(&input.stock);
    let mut subject_state = Map::new();
    subject_state.insert(
        "type".into(),
        json!(match subject.kind {
            LeadSubjectType::Keyword => "search keyword",
            LeadSubjectType::Topic => "market topic",
        }),
    );
    subject_state.insert("text".into(), json!(subject.text));
    if let Some(path) = subject.path.as_deref().filter(|p| !p.is_empty()) {
        subject_state.insert("path".into(), json!(path));
    }
    if let Some(summary) = subject.summary.as_deref().filter(|s| !s.is_empty()) {
        subject_state.insert("summary".into(), json!(summary));
    }
    if let Some(terms) = subject.terms.as_ref().filter(|t| !t.is_empty()) {
        subject_state.insert("search_terms".into(), json!(terms));
    }
    if let Some(reason) = subject.reason.as_deref().filter(|r| !r.is_empty()) {
        subject_state.insert("reason".into(), json!(reason));
    }
    state.insert("subject".into(), Value::Object(subject_state));
    Value::Object(state)
}

#[cfg(test)]
#[path = "questions_tests.rs"]
mod tests;
