use super::*;

fn same(a: &str, b: &str) -> bool {
    is_same_news(&title_features(a), &title_features(b))
}

// 2026-10-22 실측에 쓴 실제 제목들
// describe("isSameNews (11단계 비슷한 제목 기준)")
#[test]
fn same_news_written_differently_by_outlets() {
    // it("같은 소식을 언론사마다 다르게 쓴 제목은 같은 소식")
    assert!(same(
        "[속보] 한은, 기준금리 3.00% 동결",
        "한은, 기준금리 3.00% 동결…연속 인상 후 숨 고르기"
    ));
    assert!(same(
        "美, 사우디 주재 공무원·가족 철수 명령… 후티 반군 공격 임박했나",
        "美, 사우디 주재 직원·가족 철수 명령…후티 위협에 경계 강화"
    ));
    assert!(same(
        "‘매파적 동결’ 택한 한은…‘11월 인상’ 열어뒀다",
        "‘매파적 동결’ 택한 한은…‘11월 인상’ 문 열어둬"
    ));
    assert!(same(
        "KT클라우드, 50MW 안산 AIDC 착공…수도권 AI 인프라 확대",
        "KT클라우드, 50MW 규모 안산 AI 데이터센터 착공"
    ));
}

#[test]
fn different_news_with_same_template_or_accidental_english_overlap() {
    // it("틀만 같은 다른 회사 소식·우연히 겹친 영어 제목은 다른 소식")
    assert!(!same(
        "LGD, 3분기 영업익 2506억원… 전년比 42%↓",
        "삼성E&A, 3분기 영업익 5006억원···전년比 183.6% 증가"
    ));
    assert!(!same(
        "Oil rises as US orders diplomats to leave Saudi Arabia",
        "Trump to rally for Graham in tight South Carolina race"
    ));
    assert!(!same(
        "국제유가 혼조세 마감…브렌트유 108달러",
        "코스피 3.5% 급락 마감"
    ));
}

#[test]
fn prefixes_and_symbols_are_ignored_and_empty_titles_never_match() {
    // it("머리말과 기호는 보지 않고, 빈 제목은 같은 소식이 아니다")
    assert!(same("[단독](종합) 금리 동결", "금리 동결"));
    assert!(!same("", ""));
    assert!(!same("…", "…"));
}

#[test]
fn bom_before_prefix_is_treated_as_space() {
    // Rust에서만 생기는 경우: JS `\s`는 BOM도 공백이라 머리말 앞 BOM이 머리말 인식을 막지 않는다.
    assert_eq!(
        title_features("\u{FEFF}[속보] 금리 동결"),
        title_features("금리 동결")
    );
}
