use super::*;

#[test]
fn normalizes_korean_codes() {
    // it.each([["005930","005930"],[" a005930 ","005930"],["5930","005930"],["0158s0","0158S0"],["005-930","005930"]])
    for (input, expected) in [
        ("005930", "005930"),
        (" a005930 ", "005930"),
        ("5930", "005930"),
        ("0158s0", "0158S0"),
        ("005-930", "005930"),
    ] {
        assert_eq!(normalize_code(Market::Kr, input), expected, "{input}");
    }
}

#[test]
fn normalizes_us_tickers() {
    // it.each([["nvda","NVDA"],["BRK-B","BRK.B"],["BRK/B","BRK.B"],[" tsm ","TSM"]])
    for (input, expected) in [
        ("nvda", "NVDA"),
        ("BRK-B", "BRK.B"),
        ("BRK/B", "BRK.B"),
        (" tsm ", "TSM"),
    ] {
        assert_eq!(normalize_code(Market::Us, input), expected, "{input}");
    }
}

#[test]
fn korean_edge_cases_follow_the_ts_regexes() {
    // Rust에서만 생기는 경우: 정규식 두 개(`^A[0-9A-Z]{6}$`, `^\d{1,5}$`)의 경계
    assert_eq!(normalize_code(Market::Kr, "A00593"), "A00593"); // 6자라 A를 떼지 않는다
    assert_eq!(normalize_code(Market::Kr, "A0059300"), "A0059300"); // 8자
    assert_eq!(normalize_code(Market::Kr, "123456"), "123456");
    assert_eq!(normalize_code(Market::Kr, "1234567"), "1234567"); // 숫자 7자는 채우지 않는다
    assert_eq!(normalize_code(Market::Kr, ""), "");
    assert_eq!(normalize_code(Market::Us, "BRK B"), "BRK.B");
}
