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
