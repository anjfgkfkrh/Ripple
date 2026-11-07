use serde_json::Value;

use super::*;
use crate::test_support::ms;

fn now() -> EpochMs {
    ms("2026-10-20T10:00:00Z")
}

fn rss(items: &str) -> String {
    format!(
        r#"<?xml version="1.0" encoding="utf-8"?><rss version="2.0"><channel><title>t</title><link>https://example.com</link>{items}</channel></rss>"#
    )
}

// describe("parseFeed: RSS 2.0")
#[test]
fn reads_title_link_summary_date_and_cleans_cdata_html_entities() {
    // it("제목·링크·요약·발행 시각을 읽고 CDATA·HTML·엔티티를 정리한다")
    let items = parse_feed(
        &rss(r#"
        <item>
          <title><![CDATA[삼성전자, HBM <b>공급</b> 확대 &amp; 증설]]></title>
          <link>https://news.example.com/a/1?utm_source=rss</link>
          <description>&lt;p&gt;메모리 &amp;middot; 파운드리&lt;/p&gt;&lt;img src="x.jpg"&gt;</description>
          <pubDate>Tue, 20 Oct 2026 17:36:24 +0900</pubDate>
        </item>"#),
        now(),
    );
    assert_eq!(
        items,
        [FeedItem {
            title: "삼성전자, HBM 공급 확대 & 증설".into(),
            link: "https://news.example.com/a/1?utm_source=rss".into(),
            summary: "메모리 · 파운드리".into(),
            published_at: Some(ms("2026-10-20T08:36:24Z")),
        }]
    );
}

#[test]
fn uses_permalink_guid_when_link_is_missing_and_drops_items_without_title_or_url() {
    // it("link가 없으면 permalink guid를 쓰고, 제목이나 주소가 없으면 버린다")
    let items = parse_feed(
        &rss(r#"
        <item><title>guid만 있음</title><guid isPermaLink="true">https://e.com/g</guid></item>
        <item><title>주소 없음</title><guid>12345</guid></item>
        <item><title></title><link>https://e.com/no-title</link></item>"#),
        now(),
    );
    let pairs: Vec<(&str, &str)> = items
        .iter()
        .map(|i| (i.title.as_str(), i.link.as_str()))
        .collect();
    assert_eq!(pairs, [("guid만 있음", "https://e.com/g")]);
}

#[test]
fn missing_date_is_none() {
    // it("발행 시각이 없으면 null(한겨레처럼)")
    let items = parse_feed(
        &rss("<item><title>a</title><link>https://e.com/1</link></item>"),
        now(),
    );
    assert_eq!(items[0].published_at, None);
}

#[test]
fn content_encoded_is_not_used_as_summary() {
    // it("content:encoded는 요약으로 쓰지 않는다(description만)")
    let items = parse_feed(
        &rss(
            "<item><title>a</title><link>https://e.com/1</link><content:encoded><![CDATA[<p>본문 전체</p>]]></content:encoded></item>",
        ),
        now(),
    );
    assert_eq!(items[0].summary, "");
}

// describe("parseFeed: Atom")
#[test]
fn reads_atom_alternate_link_and_published() {
    // it("entry의 alternate 링크와 published를 읽는다")
    let items = parse_feed(
        r#"<feed xmlns="http://www.w3.org/2005/Atom">
        <entry>
          <title type="html">Nvidia &amp; TSMC</title>
          <link rel="self" href="https://e.com/self"/>
          <link rel="alternate" href="https://e.com/a?x=1&amp;y=2"/>
          <summary>AI chips</summary>
          <published>2026-10-20T01:02:03Z</published>
        </entry>
      </feed>"#,
        now(),
    );
    assert_eq!(
        items,
        [FeedItem {
            title: "Nvidia & TSMC".into(),
            link: "https://e.com/a?x=1&y=2".into(),
            summary: "AI chips".into(),
            published_at: Some(ms("2026-10-20T01:02:03Z")),
        }]
    );
}

// describe("parseFeedDate")
fn date(raw: &str) -> Option<String> {
    parse_feed_date(Some(raw), now()).map(|ms| {
        DateTime::from_timestamp_millis(ms)
            .unwrap()
            .format("%Y-%m-%dT%H:%M:%S%.3fZ")
            .to_string()
    })
}

#[test]
fn reads_standard_and_nocut_and_naive_formats() {
    // it.each([...])
    for (raw, iso) in [
        (
            "Tue, 20 Oct 2026 17:36:24 +0900",
            "2026-10-20T08:36:24.000Z",
        ),
        ("Mon, 19 Oct 2026 23:50:00 GMT", "2026-10-19T23:50:00.000Z"),
        ("2026-10-20T09:00:00+09:00", "2026-10-20T00:00:00.000Z"),
        ("2026-10-20T09:00:00Z", "2026-10-20T09:00:00.000Z"),
        // 노컷뉴스: 월이 숫자
        ("Tue, 20 10 2026 17:36:24 +0900", "2026-10-20T08:36:24.000Z"),
        // 시간대 없음 → 한국 시간
        ("2026-10-20 17:36:24", "2026-10-20T08:36:24.000Z"),
        ("2026-10-20 17:36", "2026-10-20T08:36:00.000Z"),
    ] {
        assert_eq!(date(raw).as_deref(), Some(iso), "{raw}");
    }
}

#[test]
fn unreadable_or_absurd_times_are_none() {
    // it("읽을 수 없거나 말이 안 되는 시각은 null")
    assert_eq!(date(""), None);
    assert_eq!(date("어제"), None);
    assert_eq!(date("Mon, 01 Jan 0001 00:00:00 GMT"), None);
    assert_eq!(date("2026-10-23T00:00:00Z"), None);
    assert_eq!(parse_feed_date(None, now()), None);
}

// describe("toText·decodeEntities")
#[test]
fn decodes_numeric_and_named_entities_and_keeps_unknown() {
    // it("숫자·이름 엔티티를 풀고 모르는 엔티티는 그대로 둔다")
    assert_eq!(
        decode_entities("&#44032;&#xAC01; &hellip; &unknown;"),
        "가각 … &unknown;"
    );
    assert_eq!(to_text(Some("  a\n\n  b  ")), "a b");
    assert_eq!(to_text(None), "");
}

#[test]
fn entity_edge_cases() {
    // Rust에서만 생기는 경우: 0·범위 밖·서로게이트 코드와 대소문자, 두 번 풀기
    assert_eq!(
        decode_entities("&#0;&#1114112;&#xD800;"),
        "&#0;&#1114112;&#xD800;"
    );
    assert_eq!(decode_entities("&AMP;&Hellip;"), "&…");
    // 태그를 걷는 것은 첫 번째 풀기 뒤 한 번뿐이라, 두 번째로 풀려 나온 `<b>`는 남는다.
    assert_eq!(to_text(Some("&amp;lt;b&amp;gt;")), "<b>");
    assert_eq!(to_text(Some("<![CDATA[a &amp; b]]>")), "a & b");
}

// 설계 R5 완료 기준 + 결정 "14단계 R5": TS `parseFeedDate`와 같은 결과(실제 피드 56개의 날짜 글자 + 경계 사례)
const DATE_FIXTURE: &str = include_str!("date_fixture.json");

#[test]
fn feed_dates_equal_ts_results_for_live_samples_and_edge_cases() {
    let fixture: Value = serde_json::from_str(DATE_FIXTURE).unwrap();
    let at = ms(fixture["now"].as_str().unwrap());
    let cases = fixture["cases"].as_array().unwrap();
    assert!(cases.len() > 300);
    let mut mismatches = Vec::new();
    for case in cases {
        let raw = case["raw"].as_str().unwrap();
        let expected = case["expected"].as_str().map(str::to_string);
        let actual = parse_feed_date(Some(raw), at).map(|ms| {
            DateTime::from_timestamp_millis(ms)
                .unwrap()
                .format("%Y-%m-%dT%H:%M:%S%.3fZ")
                .to_string()
        });
        if actual != expected {
            mismatches.push(format!("{raw:?}: TS {expected:?}, Rust {actual:?}"));
        }
    }
    assert!(
        mismatches.is_empty(),
        "{} 건 다름:\n{}",
        mismatches.len(),
        mismatches.join("\n")
    );
}

#[test]
fn written_year_finds_first_four_digit_run() {
    assert_eq!(written_year("Tue, 20 Oct 2026 17:36:24"), Some(2026));
    assert_eq!(written_year("12345 2026"), Some(2026));
    assert_eq!(written_year("no year 99"), None);
    assert_eq!(written_year("0001-01-01"), Some(1));
}
