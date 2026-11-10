use reqwest::header::{HeaderMap, HeaderValue};
use serde_json::{Value, json};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use super::*;
use crate::server::ai::decider::types::{
    DecisionArticle, DecisionLink, JudgeItem, LeadSubject, LeadSubjectType,
};
use crate::server::db::schema::{Market, MatchKind};

fn stock() -> JudgeItem {
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

fn input() -> DecisionInput {
    DecisionInput {
        stock: stock(),
        link: DecisionLink {
            kind: MatchKind::Name,
            path: "삼성전자".into(),
            reason: None,
        },
        article: DecisionArticle {
            title: "삼성전자 3분기 실적".into(),
            summary: Some("영업이익이 늘었다.".into()),
        },
    }
}

fn lead() -> LeadInput {
    LeadInput {
        stock: stock(),
        subject: LeadSubject {
            kind: LeadSubjectType::Keyword,
            text: "HBM".into(),
            path: Some("삼성전자 → HBM".into()),
            summary: None,
            terms: None,
            reason: Some("AI 서버용 고대역폭 메모리".into()),
        },
    }
}

fn score_answer(score: f64, confidence: f64) -> Value {
    json!({
        "type": "score", "score": score, "confidence": confidence, "legend": {},
        "probabilities": { "0": 0, "1": 0, "2": 0.1, "3": 0.3, "4": 0.5, "5": 0.1 },
    })
}

/// 공식 문서(API reference)의 응답 모양
fn answer() -> Value {
    json!({
        "model": "jev-1.13.0",
        "answers": {
            "relevance": score_answer(3.6, 0.7),
            "magnitude": score_answer(2.5, 0.6),
            "impact": {
                "type": "choice", "choice": "positive", "confidence": 0.8,
                "probabilities": { "positive": 0.85, "negative": 0.05, "neutral": 0.1 },
            },
        },
        "usage": { "input_tokens": 900, "output_tokens": 50 },
    })
}

fn json_response(status: u16, body: Value) -> ResponseTemplate {
    ResponseTemplate::new(status).set_body_json(body)
}

/// 받은 순서대로 차례 응답하는 가짜 서버(마지막 응답은 계속 쓴다)
async fn server_with(responses: Vec<ResponseTemplate>) -> MockServer {
    let server = MockServer::start().await;
    let last = responses.len() - 1;
    for (index, response) in responses.into_iter().enumerate() {
        let mock = Mock::given(method("POST"))
            .and(path("/v1/systemone"))
            .respond_with(response);
        let mock = if index < last {
            mock.up_to_n_times(1)
        } else {
            mock
        };
        server.register(mock).await;
    }
    server
}

fn decider(server: &MockServer) -> TypeSafeDecider {
    TypeSafeDecider::new(
        Some("ts-test".into()),
        Some(server.uri()),
        reqwest::Client::new(),
    )
}

async fn call_count(server: &MockServer) -> usize {
    server.received_requests().await.unwrap().len()
}

// describe("createTypeSafeDecider")
#[tokio::test(flavor = "current_thread")]
async fn sends_three_questions_and_returns_scores_times_two() {
    // it("한 요청에 세 질문(관련도·영향도·호재/악재)을 보내고, 점수는 ×2 해서 0~10으로 돌려준다")
    let server = server_with(vec![json_response(200, answer())]).await;
    let decision = decider(&server)
        .decide(&input(), "jev-1.13.0")
        .await
        .unwrap();

    assert_eq!(
        decision,
        Decision {
            relevance: 7.2,
            relevance_confidence: 0.7,
            magnitude: 5.0,
            magnitude_confidence: 0.6,
            impact: Impact::Positive,
            impact_confidence: 0.8,
            impact_probabilities: [0.85, 0.05, 0.1],
            model: "jev-1.13.0".into(),
            usage: TokenUsage {
                input_tokens: 900,
                output_tokens: 50
            },
        }
    );
    let requests = server.received_requests().await.unwrap();
    assert_eq!(requests.len(), 1);
    let call = &requests[0];
    assert_eq!(call.url.path(), "/v1/systemone");
    assert_eq!(
        call.headers.get("authorization").unwrap().to_str().unwrap(),
        "Bearer ts-test"
    );
    // 압축하지 않고 보낸다(요청이 작음).
    assert!(call.headers.get("content-encoding").is_none());
    let body: Value = serde_json::from_slice(&call.body).unwrap();
    assert_eq!(
        body,
        json!({
            "model": "jev-1.13.0",
            "state": build_article_state(&input()),
            "questions": build_article_questions(JudgeTarget::Stock),
        })
    );
}

#[tokio::test(flavor = "current_thread")]
async fn rate_sends_two_questions() {
    // it("키워드·토픽 평가(rate)는 관련도·영향도 두 질문을 보낸다(11단계)")
    let server = server_with(vec![json_response(
        200,
        json!({
            "model": "jev-1.13.0",
            "answers": { "relevance": score_answer(4.0, 0.8), "magnitude": score_answer(3.1, 0.5) },
            "usage": { "input_tokens": 400, "output_tokens": 20 },
        }),
    )])
    .await;
    let rating = decider(&server).rate(&lead(), "jev-1.13.0").await.unwrap();
    assert_eq!(
        rating,
        LeadRating {
            relevance: 8.0,
            relevance_confidence: 0.8,
            magnitude: 6.2,
            magnitude_confidence: 0.5,
            model: "jev-1.13.0".into(),
            usage: TokenUsage {
                input_tokens: 400,
                output_tokens: 20
            },
        }
    );
    let requests = server.received_requests().await.unwrap();
    let body: Value = serde_json::from_slice(&requests[0].body).unwrap();
    assert_eq!(
        body,
        json!({
            "model": "jev-1.13.0",
            "state": build_lead_state(&lead()),
            "questions": build_lead_questions(JudgeTarget::Stock),
        })
    );
}

#[tokio::test(flavor = "current_thread")]
async fn market_item_uses_market_questions() {
    // it("시장 항목은 시장 질문으로 묻는다")
    let server = server_with(vec![json_response(200, answer())]).await;
    let mut market = input();
    market.stock = JudgeItem {
        kind: JudgeTarget::Market,
        name: "한국 증시".into(),
        official_name: None,
        market: Market::Kr,
        exchange: "KOSPI·KOSDAQ".into(),
        code: "MARKET".into(),
        summary: None,
    };
    decider(&server)
        .decide(&market, "jev-1.13.0")
        .await
        .unwrap();
    let requests = server.received_requests().await.unwrap();
    let body: Value = serde_json::from_slice(&requests[0].body).unwrap();
    assert_eq!(
        body["questions"],
        build_article_questions(JudgeTarget::Market)
    );
}

#[tokio::test(flavor = "current_thread")]
async fn missing_token_counts_are_zero() {
    // it("토큰 수가 없으면 0으로 센다")
    let mut body = answer();
    body["usage"] = json!({ "input_tokens": null, "output_tokens": null });
    let server = server_with(vec![json_response(200, body)]).await;
    let decision = decider(&server)
        .decide(&input(), "jev-1.13.0")
        .await
        .unwrap();
    assert_eq!(
        decision.usage,
        TokenUsage {
            input_tokens: 0,
            output_tokens: 0
        }
    );
}

#[tokio::test(flavor = "current_thread")]
async fn wrong_shape_is_upstream_error() {
    // it("응답 모양이 다르면(영향도 없음) upstream 오류")
    let mut body = answer();
    body["answers"].as_object_mut().unwrap().remove("magnitude");
    let server = server_with(vec![json_response(200, body)]).await;
    let error = decider(&server)
        .decide(&input(), "jev-1.13.0")
        .await
        .unwrap_err();
    assert_eq!(error.kind, DeciderErrorKind::Upstream);
    assert_eq!(error.message, "Jev 응답 형식이 다릅니다.");
}

#[tokio::test(flavor = "current_thread")]
async fn missing_key_makes_no_request_and_is_fatal() {
    // it("키가 없으면 부르지 않고 no_api_key(이번 회차를 멈추는 오류)")
    let server = server_with(vec![json_response(200, answer())]).await;
    let keyless = TypeSafeDecider::new(None, Some(server.uri()), reqwest::Client::new());
    let error = keyless.decide(&input(), "jev-1.13.0").await.unwrap_err();
    assert_eq!(error.kind, DeciderErrorKind::NoApiKey);
    assert!(error.fatal());
    assert_eq!(call_count(&server).await, 0);
}

#[tokio::test(flavor = "current_thread")]
async fn http_status_maps_to_kind_fatal_and_attempts() {
    // it.each([[401,"unauthorized",true,1],[403,..,true,1],[400,"invalid_request",false,1],
    //          [429,"rate_limited",false,3],[503,"upstream",false,3]])("HTTP %i → %s (멈춤 %s, 시도 %i번)")
    // 429·5xx는 SDK처럼 두 번 더 시도한 뒤 포기한다.
    let cases = [
        (401, DeciderErrorKind::Unauthorized, true, 1),
        (403, DeciderErrorKind::Unauthorized, true, 1),
        (400, DeciderErrorKind::InvalidRequest, false, 1),
        (429, DeciderErrorKind::RateLimited, false, 3),
        (503, DeciderErrorKind::Upstream, false, 3),
    ];
    for (status, kind, fatal, attempts) in cases {
        let server = server_with(vec![
            json_response(status, json!({ "detail": "x" })).insert_header("retry-after", "0"),
        ])
        .await;
        let error = decider(&server)
            .decide(&input(), "jev-1.13.0")
            .await
            .unwrap_err();
        assert_eq!((error.kind, error.fatal()), (kind, fatal), "HTTP {status}");
        assert_eq!(call_count(&server).await, attempts, "HTTP {status}");
    }
}

#[tokio::test(flavor = "current_thread")]
async fn success_after_429_uses_that_response() {
    // it("429 뒤에 성공하면 그 응답을 쓴다")
    let server = server_with(vec![
        json_response(429, json!({})).insert_header("retry-after", "0"),
        json_response(200, answer()),
    ])
    .await;
    let decision = decider(&server)
        .decide(&input(), "jev-1.13.0")
        .await
        .unwrap();
    assert_eq!(decision.impact, Impact::Positive);
    let requests = server.received_requests().await.unwrap();
    assert_eq!(requests.len(), 2);
    // 다시 시도하는 요청에는 시도 횟수를 알린다(SDK와 같은 X-TypeSafe-Retry-Count).
    assert!(requests[0].headers.get("x-typesafe-retry-count").is_none());
    assert_eq!(
        requests[1]
            .headers
            .get("x-typesafe-retry-count")
            .unwrap()
            .to_str()
            .unwrap(),
        "1"
    );
}

// Rust에서만 생기는 경우: SDK의 재시도 정책을 직접 옮겼으므로 값과 경계를 고정한다.
fn headers(pairs: &[(&'static str, &'static str)]) -> HeaderMap {
    let mut map = HeaderMap::new();
    for (name, value) in pairs {
        map.insert(*name, HeaderValue::from_static(value));
    }
    map
}

#[test]
fn retry_delay_uses_server_hint_then_backoff() {
    let now = 1_792_000_000_000;
    // retry-after-ms가 retry-after보다 먼저
    let both = headers(&[("retry-after-ms", "1500"), ("retry-after", "9")]);
    assert_eq!(retry_delay_ms(0, Some(&both), now, 0.0), 1500);
    assert_eq!(
        retry_delay_ms(0, Some(&headers(&[("retry-after", "3")])), now, 0.0),
        3000
    );
    assert_eq!(
        retry_delay_ms(0, Some(&headers(&[("retry-after", "0")])), now, 0.0),
        0
    );
    // 60초를 넘는 요청은 무시하고 백오프
    assert_eq!(
        retry_delay_ms(0, Some(&headers(&[("retry-after", "61")])), now, 0.0),
        500
    );
    // 올바르지 않은 값은 무시
    assert_eq!(
        retry_delay_ms(0, Some(&headers(&[("retry-after", "-5")])), now, 0.0),
        500
    );
    assert_eq!(
        retry_delay_ms(0, Some(&headers(&[("retry-after-ms", "abc")])), now, 0.0),
        500
    );
    // HTTP 날짜: 지금부터 2초 뒤
    let date = chrono::DateTime::from_timestamp_millis(now + 2000)
        .unwrap()
        .to_rfc2822()
        .replace("+0000", "GMT");
    let value: &'static str = Box::leak(date.into_boxed_str());
    assert_eq!(
        retry_delay_ms(0, Some(&headers(&[("retry-after", value)])), now, 0.0),
        2000
    );
}

#[test]
fn backoff_doubles_up_to_cap_and_jitter_shrinks_it() {
    assert_eq!(retry_delay_ms(0, None, 0, 0.0), 500);
    assert_eq!(retry_delay_ms(1, None, 0, 0.0), 1000);
    assert_eq!(retry_delay_ms(2, None, 0, 0.0), 2000);
    assert_eq!(retry_delay_ms(4, None, 0, 0.0), 5000);
    assert_eq!(retry_delay_ms(9, None, 0, 0.0), 5000);
    // 500 × (1 − 0.5 × 0.25) = 437.5 → JS Math.round는 438
    assert_eq!(retry_delay_ms(0, None, 0, 0.5), 438);
    assert_eq!(retry_delay_ms(1, None, 0, 0.999_999), 750);
}

#[test]
fn only_408_429_and_5xx_are_retried() {
    for status in [408, 429, 500, 502, 503, 599] {
        assert!(is_retryable_status(status), "{status}");
    }
    for status in [200, 400, 401, 403, 404, 422, 600] {
        assert!(!is_retryable_status(status), "{status}");
    }
}

#[tokio::test(flavor = "current_thread")]
async fn trailing_slashes_of_base_url_are_removed() {
    let server = server_with(vec![json_response(200, answer())]).await;
    let slashed = TypeSafeDecider::new(
        Some("k".into()),
        Some(format!("{}//", server.uri())),
        reqwest::Client::new(),
    );
    slashed.decide(&input(), "jev-1.13.0").await.unwrap();
    assert_eq!(
        server.received_requests().await.unwrap()[0].url.path(),
        "/v1/systemone"
    );
}

#[tokio::test(flavor = "current_thread")]
async fn unreachable_server_is_upstream_after_retries() {
    let dead = TypeSafeDecider::new(
        Some("k".into()),
        Some("http://127.0.0.1:1".into()),
        reqwest::Client::new(),
    );
    let error = dead.decide(&input(), "jev-1.13.0").await.unwrap_err();
    assert_eq!(error.kind, DeciderErrorKind::Upstream);
    assert_eq!(error.message, "TypeSafe에 연결하지 못했습니다.");
}

#[tokio::test(flavor = "current_thread")]
async fn non_json_success_body_is_upstream_error() {
    let server = server_with(vec![ResponseTemplate::new(200).set_body_string("ok")]).await;
    let error = decider(&server)
        .decide(&input(), "jev-1.13.0")
        .await
        .unwrap_err();
    assert_eq!(error.message, "Jev 응답 형식이 다릅니다.");
}

// 17단계 eval-merge: 합친 응답 해석
fn merged_input() -> crate::server::ai::decider::types::MergedInput {
    crate::server::ai::decider::types::MergedInput {
        topics: vec![crate::server::ai::decider::types::MergedTopic {
            name: "금리".into(),
            summary: None,
        }],
        market_items: vec![Market::Kr],
        markets: vec![Market::Kr, Market::Us],
        article: DecisionArticle {
            title: "금리 동결".into(),
            summary: None,
        },
    }
}

fn choice_answer(choice: &str) -> Value {
    json!({
        "type": "choice", "choice": choice, "confidence": 0.8,
        "probabilities": { "positive": 0.1, "negative": 0.7, "neutral": 0.2 },
    })
}

fn merged_answer() -> Value {
    json!({
        "model": "jev-1.13.0",
        "answers": {
            "relevance_t1": score_answer(3.6, 0.7),
            "relevance_kr": score_answer(2.0, 0.5),
            "magnitude_kr": score_answer(2.5, 0.6),
            "magnitude_us": score_answer(1.0, 0.9),
            "impact_kr": choice_answer("positive"),
            "impact_us": choice_answer("negative"),
        },
        "usage": { "input_tokens": 1200, "output_tokens": 80 },
    })
}

#[tokio::test(flavor = "current_thread")]
async fn merged_decision_reads_every_key_with_scores_times_two() {
    let server = server_with(vec![json_response(200, merged_answer())]).await;
    let result = decider(&server)
        .decide_merged(&merged_input(), "jev-1.13.0")
        .await
        .unwrap();
    assert_eq!(
        result.topics,
        [Scored {
            score: 7.2,
            confidence: 0.7
        }]
    );
    assert_eq!(result.markets.len(), 2);
    let (kr, us) = (&result.markets[0], &result.markets[1]);
    assert_eq!(kr.market, Market::Kr);
    assert_eq!(
        kr.relevance,
        Some(Scored {
            score: 4.0,
            confidence: 0.5
        })
    );
    assert_eq!(kr.magnitude.score, 5.0);
    assert_eq!(kr.impact, Impact::Positive);
    assert_eq!(kr.impact_confidence, 0.8);
    assert_eq!(kr.impact_probabilities, [0.1, 0.7, 0.2]);
    assert_eq!(us.relevance, None);
    assert_eq!(us.magnitude.score, 2.0);
    assert_eq!(us.impact, Impact::Negative);
    assert_eq!(
        (result.model.as_str(), result.usage),
        (
            "jev-1.13.0",
            TokenUsage {
                input_tokens: 1200,
                output_tokens: 80
            }
        )
    );
    let requests = server.received_requests().await.unwrap();
    let body: Value = serde_json::from_slice(&requests[0].body).unwrap();
    assert_eq!(body["state"], build_merged_state(&merged_input()));
    assert_eq!(body["questions"], build_merged_questions(&merged_input()));
}

#[tokio::test(flavor = "current_thread")]
async fn merged_decision_with_a_missing_key_is_a_format_error() {
    for key in ["relevance_t1", "relevance_kr", "magnitude_us", "impact_kr"] {
        let mut body = merged_answer();
        body["answers"].as_object_mut().unwrap().remove(key);
        let server = server_with(vec![json_response(200, body)]).await;
        let error = decider(&server)
            .decide_merged(&merged_input(), "jev-1.13.0")
            .await
            .unwrap_err();
        assert_eq!(error.kind, DeciderErrorKind::Upstream, "{key}");
        assert_eq!(error.message, "Jev 응답 형식이 다릅니다.");
    }
}
