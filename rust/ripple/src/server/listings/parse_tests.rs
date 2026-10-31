use super::*;
use crate::test_support::{FIXTURE_FILES, decode_euc_kr, fixture_text};

fn kind_rows() -> Vec<ListingRow> {
    parse_kind_list(&decode_euc_kr(FIXTURE_FILES.kind)).unwrap()
}

// describe("parseKindList (KIND 상장법인목록, 실제 파일 표본)")
#[test]
fn reads_name_code_and_market_from_euc_kr_table() {
    // it("EUC-KR 표에서 회사명·종목코드·시장을 읽는다")
    let rows = kind_rows();
    assert!(rows.contains(&ListingRow {
        market: Market::Kr,
        code: "005930".into(),
        name: "삼성전자".into(),
        exchange: "KOSPI".into(),
        is_etf: false,
    }));
    assert_eq!(
        rows.iter().find(|r| r.name == "SK하이닉스").unwrap().code,
        "000660"
    );
}

#[test]
fn converts_market_to_exchange_and_reads_new_alphanumeric_codes() {
    // it("시장구분을 거래소 이름으로 바꾸고, 영문이 섞인 새 코드도 읽는다")
    let rows = kind_rows();
    let exchanges: std::collections::HashSet<&str> =
        rows.iter().map(|r| r.exchange.as_str()).collect();
    assert_eq!(
        exchanges,
        ["KOSPI", "KOSDAQ", "KONEX"].into_iter().collect()
    );
    assert_eq!(
        rows.iter().find(|r| r.name == "엘리스").unwrap().code,
        "0158S0"
    );
}

#[test]
fn fails_when_column_names_change() {
    // it("열 이름이 바뀌면 조용히 넘어가지 않고 실패한다")
    let error = parse_kind_list("<table><tr><th>이름</th></tr></table>").unwrap_err();
    assert!(error.to_string().contains("형식이 바뀌었습니다"));
}

fn nasdaq() -> Vec<ListingRow> {
    parse_nasdaq_listed(&fixture_text(FIXTURE_FILES.nasdaq_listed)).unwrap()
}

fn other() -> Vec<ListingRow> {
    parse_other_listed(&fixture_text(FIXTURE_FILES.other_listed)).unwrap()
}

// describe("parseNasdaqListed / parseOtherListed (NASDAQ Trader, 실제 파일 표본)")
#[test]
fn reads_nasdaq_stocks_and_etf_flag() {
    // it("NASDAQ 종목과 ETF 여부를 읽는다")
    let nasdaq = nasdaq();
    let nvda = nasdaq.iter().find(|r| r.code == "NVDA").unwrap();
    assert_eq!(nvda.market, Market::Us);
    assert_eq!(nvda.name, "NVIDIA Corporation - Common Stock");
    assert_eq!(nvda.exchange, "NASDAQ");
    assert!(!nvda.is_etf);
    assert!(nasdaq.iter().find(|r| r.code == "QQQ").unwrap().is_etf);
}

#[test]
fn converts_other_exchange_codes_to_names() {
    // it("다른 거래소 코드를 이름으로 바꾼다(N=NYSE, P=NYSE Arca)")
    let other = other();
    assert_eq!(
        other.iter().find(|r| r.code == "TSM").unwrap().exchange,
        "NYSE"
    );
    let spy = other.iter().find(|r| r.code == "SPY").unwrap();
    assert_eq!(spy.exchange, "NYSE Arca");
    assert!(spy.is_etf);
    assert!(other.iter().any(|r| r.code == "BRK.B"));
}

#[test]
fn drops_test_issues_and_file_creation_time_line() {
    // it("시험용 종목과 마지막 파일 생성 시각 줄은 뺀다")
    let (nasdaq, other) = (nasdaq(), other());
    assert_eq!(nasdaq.len(), 5);
    assert_eq!(other.len(), 4);
    assert!(
        ![nasdaq, other]
            .concat()
            .iter()
            .any(|r| r.code.starts_with("FILE"))
    );
}

#[test]
fn fails_when_required_columns_are_missing() {
    // it("필요한 열이 없으면 실패한다")
    let error = parse_nasdaq_listed("Symbol|Name\nAAA|Foo").unwrap_err();
    assert!(error.to_string().contains("형식이 바뀌었습니다"));
}

#[test]
fn html_entities_are_decoded_in_js_order() {
    // Rust에서만 생기는 경우: decodeHtml의 순서(숫자 → nbsp → lt·gt·quot·따옴표 → amp 마지막)와 잘못된 코드 포인트
    assert_eq!(
        decode_html("A&amp;B &lt;x&gt; &quot;q&quot; &#39;s&#39; &apos;t&apos;"),
        "A&B <x> \"q\" 's' 't'"
    );
    assert_eq!(decode_html("&#44032;&#x1F600;&#X41;"), "가😀A");
    assert_eq!(decode_html("a&nbsp;b"), "a b");
    // `&amp;lt;`는 amp를 마지막에 풀기 때문에 `&lt;`로 남는다.
    assert_eq!(decode_html("&amp;lt;"), "&lt;");
    // 잘못된 코드 포인트는 그대로 둔다.
    assert_eq!(
        decode_html("&#55296;x&#99999999999;"),
        "&#55296;x&#99999999999;"
    );
}

#[test]
fn kind_rows_skip_invalid_codes_and_empty_names() {
    let html = "<table><tr><th>회사명</th><th>시장구분</th><th>종목코드</th></tr>\
        <tr><td>가나다</td><td>유가</td><td>12345</td></tr>\
        <tr><td></td><td>유가</td><td>123456</td></tr>\
        <tr><td>라마바</td><td>기타</td><td>a1b2c3</td></tr></table>";
    let rows = parse_kind_list(html).unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(
        (rows[0].code.as_str(), rows[0].exchange.as_str()),
        ("A1B2C3", "기타")
    );
}
