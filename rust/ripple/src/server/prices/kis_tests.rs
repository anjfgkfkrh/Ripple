use std::sync::Arc;
use std::sync::atomic::{AtomicI64, AtomicUsize, Ordering};

use serde_json::json;
use wiremock::matchers::any;
use wiremock::{Mock, MockServer, Request, ResponseTemplate};

use super::*;

const START: EpochMs = 1_793_322_000_000; // 2026-10-30T01:00:00Z

fn token_body(token: &str) -> Value {
    json!({
        "access_token": token,
        "access_token_token_expired": "2026-10-31 10:00:00",
        "token_type": "Bearer",
        "expires_in": 86400,
    })
}

fn price_body() -> Value {
    json!({
        "rt_cd": "0",
        "msg_cd": "MCA00000",
        "msg1": "정상처리 되었습니다.",
        "output": { "stck_prpr": "152300", "stck_sdpr": "150000", "prdy_vrss": "2300" },
    })
}

fn json_response(status: u16, body: Value) -> ResponseTemplate {
    ResponseTemplate::new(status).set_body_json(body)
}

/// 가짜 KIS 서버. 토큰 요청과 시세 요청에 `respond`가 답한다.
async fn fake_kis(
    respond: impl Fn(&Request) -> ResponseTemplate + Send + Sync + 'static,
) -> MockServer {
    let server = MockServer::start().await;
    server
        .register(Mock::given(any()).respond_with(respond))
        .await;
    server
}

fn is_token(request: &Request) -> bool {
    request.url.path() == "/oauth2/tokenP"
}

fn client(server: &MockServer, clock: &Arc<AtomicI64>) -> ReqwestKisClient {
    let clock = clock.clone();
    ReqwestKisClient::new(KisOptions {
        app_key: "app-key".into(),
        app_secret: "app-secret".into(),
        env: KisEnv::Demo,
        base_url: Some(server.uri()),
        now: Some(Arc::new(move || clock.load(Ordering::SeqCst))),
        min_interval_ms: Some(0),
        timeout: None,
    })
}

fn clock() -> Arc<AtomicI64> {
    Arc::new(AtomicI64::new(START))
}

fn header<'a>(request: &'a Request, name: &str) -> &'a str {
    request.headers.get(name).unwrap().to_str().unwrap()
}

// describe("createKisClient (13단계)")
#[tokio::test(flavor = "current_thread")]
async fn asks_token_once_then_unified_market_quote_with_official_headers() {
    // it("토큰을 한 번 받아 두고, 통합 시장(UN) 현재가를 공식 예제의 헤더로 묻는다")
    let server = fake_kis(|request| {
        if is_token(request) {
            json_response(200, token_body("tok-1"))
        } else {
            json_response(200, price_body())
        }
    })
    .await;
    let kis = client(&server, &clock());
    assert_eq!(
        kis.domestic_quote("000720").await.unwrap(),
        KisQuote {
            price: 152300.0,
            change: 2300.0,
            change_percent: 1.53
        }
    );
    kis.domestic_quote("034020").await.unwrap();

    let calls = server.received_requests().await.unwrap();
    let urls: Vec<String> = calls
        .iter()
        .map(|c| match c.url.query() {
            Some(q) => format!("{}?{q}", c.url.path()),
            None => c.url.path().to_string(),
        })
        .collect();
    assert_eq!(
        urls,
        [
            "/oauth2/tokenP",
            "/uapi/domestic-stock/v1/quotations/inquire-price?FID_COND_MRKT_DIV_CODE=UN&FID_INPUT_ISCD=000720",
            "/uapi/domestic-stock/v1/quotations/inquire-price?FID_COND_MRKT_DIV_CODE=UN&FID_INPUT_ISCD=034020",
        ]
    );
    assert_eq!(calls[0].method.as_str(), "POST");
    let body: Value = serde_json::from_slice(&calls[0].body).unwrap();
    assert_eq!(
        body,
        json!({ "grant_type": "client_credentials", "appkey": "app-key", "appsecret": "app-secret" })
    );
    assert_eq!(calls[1].method.as_str(), "GET");
    assert_eq!(
        header(&calls[1], "content-type"),
        "application/json; charset=utf-8"
    );
    assert_eq!(header(&calls[1], "authorization"), "Bearer tok-1");
    assert_eq!(header(&calls[1], "appkey"), "app-key");
    assert_eq!(header(&calls[1], "appsecret"), "app-secret");
    assert_eq!(header(&calls[1], "tr_id"), "FHKST01010100");
    assert_eq!(header(&calls[1], "custtype"), "P");
}

#[tokio::test(flavor = "current_thread")]
async fn gets_new_token_when_expiry_nears() {
    // it("만료가 다가오면 새 토큰을 받는다")
    let issued = Arc::new(AtomicUsize::new(0));
    let counter = issued.clone();
    let server = fake_kis(move |request| {
        if is_token(request) {
            let n = counter.fetch_add(1, Ordering::SeqCst) + 1;
            json_response(200, token_body(&format!("tok-{n}")))
        } else {
            json_response(200, price_body())
        }
    })
    .await;
    let clock = clock();
    let kis = client(&server, &clock);
    kis.domestic_quote("000720").await.unwrap();
    clock.fetch_add(23 * 60 * 60 * 1000, Ordering::SeqCst);
    kis.domestic_quote("000720").await.unwrap();
    assert_eq!(issued.load(Ordering::SeqCst), 1);
    clock.fetch_add(55 * 60 * 1000, Ordering::SeqCst);
    kis.domestic_quote("000720").await.unwrap();
    assert_eq!(issued.load(Ordering::SeqCst), 2);
    let calls = server.received_requests().await.unwrap();
    assert_eq!(
        header(calls.last().unwrap(), "authorization"),
        "Bearer tok-2"
    );
}

#[tokio::test(flavor = "current_thread")]
async fn token_problem_egw001_retries_once_with_a_new_token() {
    // it("토큰 문제(EGW001xx)면 새 토큰으로 한 번만 다시 묻는다")
    let issued = Arc::new(AtomicUsize::new(0));
    let price_calls = Arc::new(AtomicUsize::new(0));
    let (issued_in, price_in) = (issued.clone(), price_calls.clone());
    let server = fake_kis(move |request| {
        if is_token(request) {
            let n = issued_in.fetch_add(1, Ordering::SeqCst) + 1;
            return json_response(200, token_body(&format!("tok-{n}")));
        }
        if price_in.fetch_add(1, Ordering::SeqCst) == 0 {
            json_response(
                500,
                json!({ "rt_cd": "1", "msg_cd": "EGW00123", "msg1": "기간이 만료된 token 입니다." }),
            )
        } else {
            json_response(200, price_body())
        }
    })
    .await;
    let kis = client(&server, &clock());
    assert_eq!(kis.domestic_quote("000720").await.unwrap().price, 152300.0);
    assert_eq!(issued.load(Ordering::SeqCst), 2);
}

#[tokio::test(flavor = "current_thread")]
async fn token_problem_is_retried_only_once() {
    // Rust 전용: 재귀 대신 반복문으로 옮겼으므로 두 번째 EGW001도 끝없이 되풀이하지 않는다.
    let price_calls = Arc::new(AtomicUsize::new(0));
    let price_in = price_calls.clone();
    let server = fake_kis(move |request| {
        if is_token(request) {
            return json_response(200, token_body("tok"));
        }
        price_in.fetch_add(1, Ordering::SeqCst);
        json_response(
            500,
            json!({ "rt_cd": "1", "msg_cd": "EGW00123", "msg1": "만료" }),
        )
    })
    .await;
    let kis = client(&server, &clock());
    let error = kis.domestic_quote("000720").await.unwrap_err();
    assert_eq!(error.to_string(), "KIS 시세 오류(HTTP 500, EGW00123): 만료");
    assert_eq!(price_calls.load(Ordering::SeqCst), 2);
}

#[tokio::test(flavor = "current_thread")]
async fn quote_error_carries_status_code_and_trimmed_message() {
    // it("시세 오류는 HTTP 상태·코드·메시지를 담아 던진다")
    let server = fake_kis(|request| {
        if is_token(request) {
            json_response(200, token_body("tok-1"))
        } else {
            json_response(
                200,
                json!({ "rt_cd": "1", "msg_cd": "OPSQ0002", "msg1": "없는 종목 " }),
            )
        }
    })
    .await;
    let kis = client(&server, &clock());
    let error = kis.domestic_quote("999999").await.unwrap_err();
    assert_eq!(
        error.to_string(),
        "KIS 시세 오류(HTTP 200, OPSQ0002): 없는 종목"
    );
}

#[tokio::test(flavor = "current_thread")]
async fn quote_error_without_code_or_message_has_only_status() {
    // Rust 전용: msg_cd·msg1이 없으면 "(HTTP n)"만 남는다(TS의 filter(Boolean)·빈 메시지 분기).
    let server = fake_kis(|request| {
        if is_token(request) {
            json_response(200, token_body("tok-1"))
        } else {
            json_response(502, json!({}))
        }
    })
    .await;
    let kis = client(&server, &clock());
    let error = kis.domestic_quote("000720").await.unwrap_err();
    assert_eq!(error.to_string(), "KIS 시세 오류(HTTP 502)");
}

#[tokio::test(flavor = "current_thread")]
async fn failed_token_blocks_requests_for_a_minute() {
    // it("토큰 발급이 실패하면 1분(발급 제한) 동안은 다시 요청하지 않는다")
    let token_calls = Arc::new(AtomicUsize::new(0));
    let counter = token_calls.clone();
    let server = fake_kis(move |request| {
        if is_token(request) {
            if counter.fetch_add(1, Ordering::SeqCst) == 0 {
                json_response(
                    403,
                    json!({ "error_description": "접근토큰 발급 잠시 후 다시 시도하세요." }),
                )
            } else {
                json_response(200, token_body("tok-1"))
            }
        } else {
            json_response(200, price_body())
        }
    })
    .await;
    let clock = clock();
    let kis = client(&server, &clock);
    let error = kis.domestic_quote("000720").await.unwrap_err();
    assert_eq!(
        error.to_string(),
        "KIS 토큰을 받지 못했습니다: 접근토큰 발급 잠시 후 다시 시도하세요."
    );
    clock.fetch_add(TOKEN_RETRY_MS - 1000, Ordering::SeqCst);
    let error = kis.domestic_quote("000720").await.unwrap_err();
    assert!(error.to_string().contains("1분 뒤"));
    assert_eq!(token_calls.load(Ordering::SeqCst), 1);
    clock.fetch_add(1000, Ordering::SeqCst);
    assert_eq!(kis.domestic_quote("000720").await.unwrap().price, 152300.0);
    assert_eq!(token_calls.load(Ordering::SeqCst), 2);
}

#[tokio::test(flavor = "current_thread")]
async fn token_failure_without_description_reports_http_status() {
    // Rust 전용: error_description이 없으면 HTTP 상태를 이유로 쓴다.
    let server = fake_kis(|_| json_response(500, json!({}))).await;
    let kis = client(&server, &clock());
    let error = kis.domestic_quote("000720").await.unwrap_err();
    assert_eq!(error.to_string(), "KIS 토큰을 받지 못했습니다: HTTP 500");
}

#[tokio::test(flavor = "current_thread")]
async fn unreadable_body_reports_http_status() {
    // Rust 전용: JSON이 아닌 본문은 "응답을 읽지 못했습니다".
    let server = fake_kis(|_| ResponseTemplate::new(502).set_body_string("<html>")).await;
    let kis = client(&server, &clock());
    let error = kis.domestic_quote("000720").await.unwrap_err();
    assert_eq!(error.to_string(), "KIS 응답을 읽지 못했습니다(HTTP 502).");
}

#[tokio::test(flavor = "current_thread")]
async fn slow_server_is_cut_off_with_a_clear_message() {
    // Rust 전용: 제한 시간(TS의 AbortSignal.timeout)이 지나면 "응답이 늦어 중단했습니다."
    let server =
        fake_kis(|_| json_response(200, token_body("t")).set_delay(Duration::from_millis(300)))
            .await;
    let kis = ReqwestKisClient::new(KisOptions {
        app_key: "k".into(),
        app_secret: "s".into(),
        env: KisEnv::Demo,
        base_url: Some(server.uri()),
        now: None,
        min_interval_ms: Some(0),
        timeout: Some(Duration::from_millis(50)),
    });
    let error = kis.domestic_quote("000720").await.unwrap_err();
    assert_eq!(error.to_string(), "KIS 응답이 늦어 중단했습니다.");
}

#[tokio::test(flavor = "current_thread")]
async fn connection_failure_is_reported_as_kis_error() {
    let kis = ReqwestKisClient::new(KisOptions {
        app_key: "k".into(),
        app_secret: "s".into(),
        env: KisEnv::Demo,
        base_url: Some("http://127.0.0.1:1".into()),
        now: None,
        min_interval_ms: Some(0),
        timeout: None,
    });
    let error = kis.domestic_quote("000720").await.unwrap_err();
    assert!(error.to_string().starts_with("KIS에 연결하지 못했습니다: "));
}

#[tokio::test(flavor = "current_thread")]
async fn requests_are_spaced_by_the_minimum_interval() {
    // Rust 전용: 요청 시작 간격(호출 한도 대비)을 지킨다. 시계는 고정이라 두 번째부터 간격만큼 기다린다.
    let server = fake_kis(|request| {
        if is_token(request) {
            json_response(200, token_body("tok-1"))
        } else {
            json_response(200, price_body())
        }
    })
    .await;
    let clock = clock();
    let clock_in = clock.clone();
    let kis = ReqwestKisClient::new(KisOptions {
        app_key: "k".into(),
        app_secret: "s".into(),
        env: KisEnv::Demo,
        base_url: Some(server.uri()),
        now: Some(Arc::new(move || clock_in.load(Ordering::SeqCst))),
        min_interval_ms: Some(120),
        timeout: None,
    });
    let started = std::time::Instant::now();
    kis.domestic_quote("000720").await.unwrap(); // 토큰 + 시세 = 요청 2번
    // 시계가 멈춰 있어 요청 둘째는 120ms, (이 시험에서는 시계를 더 나가지 않으므로) 최소 한 번의 간격을 기다린다.
    assert!(started.elapsed() >= Duration::from_millis(100));
}

#[test]
fn real_key_uses_the_real_address_and_demo_the_demo_one() {
    // it("실전 키는 실전 주소를 쓴다")
    assert_eq!(
        kis_base_url(KisEnv::Real),
        "https://openapi.koreainvestment.com:9443"
    );
    assert_eq!(
        kis_base_url(KisEnv::Demo),
        "https://openapivts.koreainvestment.com:29443"
    );
    // 주소 끝의 슬래시는 떼고 쓴다
    let kis = ReqwestKisClient::new(KisOptions {
        app_key: "k".into(),
        app_secret: "s".into(),
        env: KisEnv::Real,
        base_url: None,
        now: None,
        min_interval_ms: None,
        timeout: None,
    });
    assert_eq!(kis.base, "https://openapi.koreainvestment.com:9443");
}

// describe("parseQuote")
#[test]
fn parse_quote_computes_change_and_percent_to_two_decimals() {
    // it("현재가와 기준가로 대비·등락률(소수 둘째 자리)을 계산한다")
    let drop = parse_quote(Some(
        &json!({ "stck_prpr": "98700", "stck_sdpr": "100000" }),
    ))
    .unwrap();
    assert_eq!(
        drop,
        KisQuote {
            price: 98700.0,
            change: -1300.0,
            change_percent: -1.3
        }
    );
    let flat = parse_quote(Some(
        &json!({ "stck_prpr": "100000", "stck_sdpr": "100000" }),
    ))
    .unwrap();
    assert_eq!(
        flat,
        KisQuote {
            price: 100000.0,
            change: 0.0,
            change_percent: 0.0
        }
    );
    assert!(flat.change_percent.is_sign_positive());
}

#[test]
fn parse_quote_rounds_half_toward_positive_infinity_like_math_round() {
    // JS Math.round(-2.5) = -2, Math.round(2.5) = 3. 정확히 .5 경계(1/8 단위)를 일부러 만든다.
    // 기준가 1600, 현재가 1601 → 0.0625% = 6.25(×100) → 6 → 0.06
    let up = parse_quote(Some(&json!({ "stck_prpr": 1601, "stck_sdpr": 1600 }))).unwrap();
    assert_eq!(up.change_percent, 0.06);
    // 기준가 3200, 현재가 3199 → -0.03125% = -3.125 → -3 → -0.03
    let down = parse_quote(Some(&json!({ "stck_prpr": 3199, "stck_sdpr": 3200 }))).unwrap();
    assert_eq!(down.change_percent, -0.03);
    // 현재가 7999, 기준가 8000 → -0.0125% = -1.25 → -1 → -0.01
    let small = parse_quote(Some(&json!({ "stck_prpr": 7999, "stck_sdpr": 8000 }))).unwrap();
    assert_eq!(small.change_percent, -0.01);
    // 딱 -x.5: 기준가 20000, 현재가 19999 → -0.005% = -0.5 → Math.round는 -0(=0)
    let half = parse_quote(Some(&json!({ "stck_prpr": 19999, "stck_sdpr": 20000 }))).unwrap();
    assert_eq!(half.change_percent, 0.0);
}

#[test]
fn parse_quote_reads_numbers_given_as_strings_or_numbers() {
    // Number(row.stck_prpr)는 글자도 숫자도 받는다.
    let mixed = parse_quote(Some(&json!({ "stck_prpr": 110, "stck_sdpr": " 100 " }))).unwrap();
    assert_eq!(mixed.change, 10.0);
    assert_eq!(mixed.change_percent, 10.0);
}

#[test]
fn parse_quote_rejects_missing_zero_negative_and_unreadable_values() {
    // it("값이 없거나 0이면 오류")
    let message = "KIS 시세 응답에 현재가·기준가가 없습니다.";
    for output in [
        Some(json!({ "stck_prpr": "0", "stck_sdpr": "100" })),
        Some(json!({ "stck_prpr": "100", "stck_sdpr": "0" })),
        Some(json!({ "stck_prpr": "-5", "stck_sdpr": "100" })),
        Some(json!({ "stck_prpr": "abc", "stck_sdpr": "100" })),
        Some(json!({ "stck_prpr": null, "stck_sdpr": "100" })),
        Some(json!({ "stck_sdpr": "100" })),
        Some(json!("문자열")),
        Some(json!([])),
        None,
    ] {
        let error = parse_quote(output.as_ref()).unwrap_err();
        assert_eq!(error.to_string(), message, "{output:?}");
    }
}

#[test]
fn token_lifetime_uses_expires_in_or_one_day() {
    let body = |value: Value| match json!({ "expires_in": value }) {
        Value::Object(map) => map,
        _ => unreachable!(),
    };
    assert_eq!(token_lifetime_ms(&body(json!(3600))), 3_600_000);
    assert_eq!(token_lifetime_ms(&body(json!("7200"))), 7_200_000);
    assert_eq!(token_lifetime_ms(&body(json!(0))), DAY_MS);
    assert_eq!(token_lifetime_ms(&body(json!(-1))), DAY_MS);
    assert_eq!(token_lifetime_ms(&body(json!("abc"))), DAY_MS);
    assert_eq!(token_lifetime_ms(&Map::new()), DAY_MS);
}
