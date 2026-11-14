//! Jev에 묻는 질문(SPEC FR-5, 11단계 결정). 판단을 Jev 한 곳에 모은다. 문구는 고정본 `questions_fixture.json`과 글자까지 같아야 한다
//! (테스트가 이 파일과 비교한다).
//! - 기사: 관련도·영향도·호재/악재를 한 요청에 묻는다.
//! - 키워드·토픽(트리 후보, 토픽×종목 쌍): 관련도·영향도를 묻는다.
//!
//! 점수는 상황 6단계(0~5)로 묻고 ×2 해서 0~10으로 쓴다(Jev Score는 최대 10단계, 2026-10-20 결정).
//! 대상은 종목(stock)·시장(market)·주제(topic: 증시 기준으로 판단)다.
use serde_json::{Map, Value, json};

use crate::server::ai::decider::types::{
    DecisionArticle, DecisionInput, JudgeItem, JudgeTarget, LeadInput, LeadSubjectType, MergedInput,
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
    let mut questions = json!({
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
        "impact": impact_choice(format!(
            "What effect does the `article` have on {}?",
            moves_text(target)
        )),
    });
    // 종목 대상일 때만: 다른 대상이 받는 영향을 보도하고 이 회사는 원인·지나가는 언급인가
    if target == JudgeTarget::Stock {
        questions["bystander"] = json!({
            "type": "noul",
            "instructions": BYSTANDER_INSTRUCTIONS,
        });
    }
    questions
}

/// 종목 기사 질문 `bystander`의 글자(예/아니오 질문, API type은 "noul")
pub const BYSTANDER_INSTRUCTIONS: &str = "Does the `article` mainly report how something else (a stock index, another company, or an institution) is affected, with the company in `stock` appearing only as a cause or in passing?";

/// 기사 성격 질문(예/아니오, API type은 "noul"). 이름·순서·글자가 고정이다(article-classify 계획).
pub const NATURE_QUESTIONS: [(&str, &str); 6] = [
    (
        "fact",
        "Does the `article` report facts: events, announcements, results, or market moves that have already happened?",
    ),
    (
        "analysis",
        "Does the `article` interpret the causes, meaning, or background of the news, beyond restating the facts?",
    ),
    (
        "forecast",
        "Does the `article` present a prediction or outlook, such as a target price, an earnings estimate, or an expected market direction?",
    ),
    (
        "opinion",
        "Is the `article` an editorial, column, or opinion piece arguing the writer's position?",
    ),
    (
        "media",
        "Is the `article` mainly a photo or video, with only a caption or short description?",
    ),
    (
        "roundup",
        "Is the `article` a roundup that lists several unrelated news items?",
    ),
];

/// 기사 성격 6문항
pub fn build_nature_questions() -> Value {
    let mut questions = Map::new();
    for (name, instructions) in NATURE_QUESTIONS {
        questions.insert(
            name.into(),
            json!({ "type": "noul", "instructions": instructions }),
        );
    }
    Value::Object(questions)
}

/// 기사 성격 state: 기사(제목, 있으면 요약)만
pub fn build_nature_state(article: &DecisionArticle) -> Value {
    let mut article_state = Map::new();
    article_state.insert("title".into(), json!(article.title));
    if let Some(summary) = article.summary.as_deref().filter(|s| !s.is_empty()) {
        article_state.insert("summary".into(), json!(summary));
    }
    json!({ "article": Value::Object(article_state) })
}

fn impact_choice(instructions: String) -> Value {
    json!({
        "type": "choice",
        "instructions": instructions,
        "criteria": {
            "positive": "Good news",
            "negative": "Bad news",
            "neutral": "No effect, or the direction is unclear",
        },
    })
}

fn market_name(market: Market) -> &'static str {
    match market {
        Market::Kr => "Korean stock market (KOSPI, KOSDAQ)",
        Market::Us => "US stock market (S&P 500, Nasdaq)",
    }
}

/// 합친 질문·응답에서 증시를 부르는 키(`kr`, `us`)
pub fn market_key(market: Market) -> &'static str {
    match market {
        Market::Kr => "kr",
        Market::Us => "us",
    }
}

/// 기사당 한 요청 state(17단계): 걸린 토픽(`t1`부터), 걸린 증시, 기사. 토픽이 없으면 `topics`를 뺀다.
pub fn build_merged_state(input: &MergedInput) -> Value {
    let mut state = Map::new();
    if !input.topics.is_empty() {
        let mut topics = Map::new();
        for (n, topic) in input.topics.iter().enumerate() {
            let mut entry = Map::new();
            entry.insert("name".into(), json!(topic.name));
            if let Some(summary) = topic.summary.as_deref().filter(|s| !s.is_empty()) {
                entry.insert("summary".into(), json!(summary));
            }
            topics.insert(format!("t{}", n + 1), Value::Object(entry));
        }
        state.insert("topics".into(), Value::Object(topics));
    }
    let mut markets = Map::new();
    for &market in &input.markets {
        markets.insert(
            market_key(market).into(),
            json!({ "name": market_name(market) }),
        );
    }
    state.insert("markets".into(), Value::Object(markets));
    let mut article = Map::new();
    article.insert("title".into(), json!(input.article.title));
    if let Some(summary) = input.article.summary.as_deref().filter(|s| !s.is_empty()) {
        article.insert("summary".into(), json!(summary));
    }
    state.insert("article".into(), Value::Object(article));
    Value::Object(state)
}

/// 기사당 한 요청 질문(17단계). 순서는 관련도(토픽 → 시장), 영향도, 호재/악재.
pub fn build_merged_questions(input: &MergedInput) -> Value {
    let mut questions = Map::new();
    for n in 1..=input.topics.len() {
        questions.insert(
            format!("relevance_t{n}"),
            score(
                format!("How directly is the `article` about the topic in `topics.t{n}`?"),
                &article_relevance_levels(JudgeTarget::Topic),
            ),
        );
    }
    for &market in &input.markets {
        let key = market_key(market);
        if input.market_items.contains(&market) {
            questions.insert(
                format!("relevance_{key}"),
                score(
                    format!(
                        "How directly is the `article` about the overall stock market in `markets.{key}`?"
                    ),
                    &article_relevance_levels(JudgeTarget::Market),
                ),
            );
        }
    }
    for &market in &input.markets {
        let key = market_key(market);
        questions.insert(
            format!("magnitude_{key}"),
            score(
                format!(
                    "How much could the news in the `article` move the overall stock market in `markets.{key}`?"
                ),
                &MARKET_MAGNITUDE_LEVELS,
            ),
        );
    }
    for &market in &input.markets {
        let key = market_key(market);
        questions.insert(
            format!("impact_{key}"),
            impact_choice(format!(
                "What effect does the `article` have on the overall stock market in `markets.{key}`?"
            )),
        );
    }
    Value::Object(questions)
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
    let market_name = market_name(item.market);
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
