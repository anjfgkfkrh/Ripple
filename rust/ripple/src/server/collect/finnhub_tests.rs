use std::sync::Mutex;

use super::*;
use crate::server::collect::http::{HttpError, HttpResponse};
use crate::test_support::ms;

type Call = (String, Vec<(String, String)>);

struct FakeHttp {
    respond: Respond,
    calls: Mutex<Vec<Call>>,
}

type Respond = Box<dyn Fn(&str) -> (u16, String) + Send + Sync>;

impl HttpClient for FakeHttp {
    async fn get(&self, url: &str, headers: &[(&str, &str)]) -> Result<HttpResponse, HttpError> {
        self.calls.lock().unwrap().push((
            url.to_string(),
            headers
                .iter()
                .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
                .collect(),
        ));
        let (status, text) = (self.respond)(url);
        Ok(HttpResponse {
            status,
            text,
            content_type: "application/json".into(),
        })
    }
}

fn fake(status: u16, text: &str) -> FakeHttp {
    let text = text.to_string();
    FakeHttp {
        respond: Box::new(move |_| (status, text.clone())),
        calls: Mutex::new(Vec::new()),
    }
}

fn news_json() -> serde_json::Value {
    serde_json::json!({
        "id": 1, "headline": "Nvidia rises", "summary": "AI demand",
        "url": "https://finnhub.io/api/news?id=abc", "source": "Yahoo",
        "datetime": 1_792_454_400, "related": "NVDA", "category": "company",
    })
}

fn expected_news() -> FinnhubNews {
    FinnhubNews {
        id: Some(1.0),
        headline: "Nvidia rises".into(),
        summary: Some("AI demand".into()),
        url: "https://finnhub.io/api/news?id=abc".into(),
        source: Some("Yahoo".into()),
        datetime: 1_792_454_400.0,
    }
}

// describe("createFinnhubClient")
#[tokio::test(flavor = "current_thread")]
async fn company_news_sends_utc_dates_and_key_in_header() {
    // it("종목 뉴스: 기간을 날짜(UTC)로 보내고 키는 헤더로 보낸다")
    let http = fake(200, &serde_json::json!([news_json()]).to_string());
    let client = FinnhubClient::new(http, "k".into(), Some("https://fh.example/api/v1/".into()));
    let news = client
        .company_news(
            "BRK.B",
            ms("2026-10-19T03:00:00Z"),
            ms("2026-10-20T03:00:00Z"),
        )
        .await
        .unwrap();
    assert_eq!(news, [expected_news()]);
    let calls = client.http.calls.lock().unwrap();
    assert_eq!(
        calls[0].0,
        "https://fh.example/api/v1/company-news?symbol=BRK.B&from=2026-10-19&to=2026-10-20"
    );
    assert_eq!(
        calls[0].1,
        [("X-Finnhub-Token".to_string(), "k".to_string())]
    );
    // 키가 주소에 남지 않는다.
    assert!(!calls[0].0.contains("token"));
}

#[tokio::test(flavor = "current_thread")]
async fn general_news_asks_the_general_category() {
    // it("시장 뉴스는 general 분류를 받는다")
    let client = FinnhubClient::new(fake(200, "[]"), "k".into(), None);
    client.general_news().await.unwrap();
    assert_eq!(
        client.http.calls.lock().unwrap()[0].0,
        "https://finnhub.io/api/v1/news?category=general"
    );
}

#[tokio::test(flavor = "current_thread")]
async fn items_with_wrong_shape_are_dropped() {
    // it("형식이 다른 항목은 버린다")
    let mut without_url = news_json();
    without_url.as_object_mut().unwrap().remove("url");
    let body = serde_json::json!([news_json(), { "headline": 1 }, null, without_url]).to_string();
    let client = FinnhubClient::new(fake(200, &body), "k".into(), None);
    assert_eq!(client.general_news().await.unwrap(), [expected_news()]);
}

#[tokio::test(flavor = "current_thread")]
async fn http_status_becomes_an_error_with_reason() {
    // it.each([[401,"키가 맞지 않거나"],[403,...],[429,"호출 한도"],[500,"HTTP 500"]])("HTTP %i → 이유를 알려 주는 오류")
    for (status, message) in [
        (401, "키가 맞지 않거나"),
        (403, "키가 맞지 않거나"),
        (429, "호출 한도"),
        (500, "HTTP 500"),
    ] {
        let client = FinnhubClient::new(fake(status, "{}"), "k".into(), None);
        let error = client.general_news().await.unwrap_err();
        assert!(error.0.contains(message), "{status}: {error}");
    }
}

#[tokio::test(flavor = "current_thread")]
async fn non_json_or_non_array_is_an_error() {
    // it("JSON이 아니거나 배열이 아니면 오류")
    let bad = FinnhubClient::new(fake(200, "<html>"), "k".into(), None);
    assert!(
        bad.general_news()
            .await
            .unwrap_err()
            .0
            .contains("읽지 못했습니다")
    );
    let obj = FinnhubClient::new(fake(200, r#"{"error":"x"}"#), "k".into(), None);
    assert!(
        obj.general_news()
            .await
            .unwrap_err()
            .0
            .contains("형식이 다릅니다")
    );
}

#[tokio::test(flavor = "current_thread")]
async fn quote_reads_real_2026_10_30_response_and_zero_price_is_none() {
    // it("현재가(13단계): 2026-10-30 실제 응답 형식을 읽고, 현재가가 0이면 null")
    let body = r#"{"c":348.91,"d":5.08,"dp":1.4775,"h":351.43,"l":347.3,"o":348,"pc":343.83,"t":1793304000}"#;
    let client = FinnhubClient::new(fake(200, body), "k".into(), None);
    assert_eq!(
        client.quote("BRK.B").await.unwrap(),
        Some(FinnhubQuote {
            price: 348.91,
            change: 5.08,
            change_percent: 1.4775,
            quoted_at: 1_793_304_000_000
        })
    );
    {
        let calls = client.http.calls.lock().unwrap();
        assert_eq!(calls[0].0, "https://finnhub.io/api/v1/quote?symbol=BRK.B");
        assert_eq!(
            calls[0].1,
            [("X-Finnhub-Token".to_string(), "k".to_string())]
        );
    }

    let empty = FinnhubClient::new(
        fake(
            200,
            r#"{"c":0,"d":null,"dp":null,"h":0,"l":0,"o":0,"pc":0,"t":0}"#,
        ),
        "k".into(),
        None,
    );
    assert_eq!(empty.quote("X").await.unwrap(), None);
    let odd = FinnhubClient::new(fake(200, r#"{"c":"1"}"#), "k".into(), None);
    assert!(
        odd.quote("X")
            .await
            .unwrap_err()
            .0
            .contains("형식이 다릅니다")
    );
}

#[tokio::test(flavor = "current_thread")]
async fn quote_with_null_change_fields_uses_zero_and_symbol_is_encoded() {
    // Rust에서만 생기는 경우: d·dp가 null이면 0, 심볼은 encodeURIComponent
    let client = FinnhubClient::new(
        fake(200, r#"{"c":10.5,"d":null,"dp":null,"t":1793304000}"#),
        "k".into(),
        None,
    );
    let quote = client.quote("A B").await.unwrap().unwrap();
    assert_eq!((quote.change, quote.change_percent), (0.0, 0.0));
    assert_eq!(
        client.http.calls.lock().unwrap()[0].0,
        "https://finnhub.io/api/v1/quote?symbol=A%20B"
    );
}
