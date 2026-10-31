use super::*;

// describe("requireEnv")
#[test]
fn require_env_trims_value() {
    // it("값을 앞뒤 공백 없이 돌려준다")
    let env = Env::from_pairs([("A", "  value ")]);
    assert_eq!(require_env("A", &env).unwrap(), "value");
}

#[test]
fn require_env_fails_with_name_but_not_value() {
    // it("없거나 빈 값이면 이름을 담아 실패하고, 값은 메시지에 넣지 않는다")
    let err = require_env("A", &Env::default()).unwrap_err();
    assert!(err.to_string().contains("환경변수 A"));
    let err = require_env("A", &Env::from_pairs([("A", "   ")])).unwrap_err();
    assert!(err.to_string().contains("환경변수 A"));
}

// describe("readPort")
#[test]
fn read_port_uses_fallback_when_missing() {
    // it("값이 없으면 기본값을 쓴다")
    assert_eq!(read_port("P", 4000, &Env::default()).unwrap(), 4000);
}

#[test]
fn read_port_reads_valid_port() {
    // it("올바른 포트를 숫자로 읽는다")
    assert_eq!(
        read_port("P", 4000, &Env::from_pairs([("P", "4100")])).unwrap(),
        4100
    );
}

#[test]
fn read_port_rejects_invalid_values() {
    // it.each(["0", "65536", "abc", "40.5"])("잘못된 값(%s)이면 실패한다")
    for value in ["0", "65536", "abc", "40.5"] {
        let err = read_port("P", 4000, &Env::from_pairs([("P", value)])).unwrap_err();
        assert!(err.to_string().contains("환경변수 P"), "{value}");
    }
}

#[test]
fn read_port_follows_javascript_number_for_odd_forms() {
    // Rust에서만 생기는 경우: JS `Number()`가 받는 16진수·지수 표기도 같은 결과여야 한다.
    assert_eq!(
        read_port("P", 1, &Env::from_pairs([("P", "0x10")])).unwrap(),
        16
    );
    assert_eq!(
        read_port("P", 1, &Env::from_pairs([("P", "1e3")])).unwrap(),
        1000
    );
    assert!(read_port("P", 1, &Env::from_pairs([("P", "inf")])).is_err());
    assert!(read_port("P", 1, &Env::from_pairs([("P", "NaN")])).is_err());
}
