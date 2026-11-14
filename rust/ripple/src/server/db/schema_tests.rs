use super::*;

// TS에는 schema 테스트가 없다. DB에 든 글자와 enum 글자가 어긋나지 않게 고정한다.
#[test]
fn enums_round_trip_through_db_strings() {
    assert_eq!(
        Market::ALL.iter().map(|m| m.as_str()).collect::<Vec<_>>(),
        ["KR", "US"]
    );
    assert_eq!(RunTrigger::CatchUp.as_str(), "catch_up");
    assert_eq!(
        "generating".parse::<KeywordStatus>(),
        Ok(KeywordStatus::Generating)
    );
    assert_eq!("topic".parse::<ItemKind>(), Ok(ItemKind::Topic));
    assert!("kr".parse::<Market>().is_err());
    for impact in Impact::ALL {
        assert_eq!(impact.as_str().parse::<Impact>(), Ok(*impact));
    }
}

#[test]
fn classify_nature_picks_tags_and_hidden() {
    let p = |f, a, fo, o, m, r| NatureProbs {
        fact: f,
        analysis: a,
        forecast: fo,
        opinion: o,
        media: m,
        roundup: r,
    };
    use NatureTag::*;
    assert_eq!(
        classify_nature(&p(0.9, 0.5, 0.1, 0.0, 0.0, 0.0)),
        (vec![Fact, Analysis], false)
    );
    assert_eq!(
        classify_nature(&p(0.1, 0.1, 0.8, 0.5, 0.0, 0.0)),
        (vec![Forecast, Other], false)
    );
    assert_eq!(
        classify_nature(&p(0.4, 0.4, 0.4, 0.4, 0.0, 0.0)),
        (vec![Other], false)
    );
    assert_eq!(
        classify_nature(&p(0.9, 0.0, 0.0, 0.0, 0.5, 0.0)),
        (vec![Fact], true)
    );
    assert_eq!(
        classify_nature(&p(0.9, 0.0, 0.0, 0.0, 0.49, 0.5)),
        (vec![Fact], true)
    );
    assert_eq!(
        classify_nature(&p(0.9, 0.0, 0.0, 0.0, 0.49, 0.49)),
        (vec![Fact], false)
    );
    assert_eq!(
        [Fact, Analysis, Forecast, Other].map(NatureTag::label),
        ["사실", "해석", "전망", "기타"]
    );
}
