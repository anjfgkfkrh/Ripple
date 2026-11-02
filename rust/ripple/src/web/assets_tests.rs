use super::*;

// Rust 전용: 주소에 내용 해시 8자가 붙고, 같은 내용이면 같은 주소다
#[test]
fn urls_carry_an_eight_char_content_hash() {
    let assets = Assets::load();
    for url in [&assets.urls.css, &assets.urls.app_js, &assets.urls.htmx] {
        let name = url.strip_prefix("/static/").unwrap();
        let parts: Vec<&str> = name.split('.').collect();
        assert_eq!(parts.len(), 3, "{name}");
        assert_eq!(parts[1].len(), 8, "{name}");
        assert!(parts[1].chars().all(|c| c.is_ascii_hexdigit()));
    }
    assert_eq!(Assets::load().urls.css, assets.urls.css);
}

// Rust 전용: 이름이 맞는 파일만 찾는다(해시가 다르면 없음)
#[test]
fn finds_only_exact_names() {
    let assets = Assets::load();
    let css = assets.urls.css.strip_prefix("/static/").unwrap();
    assert!(assets.find(css).is_some());
    assert!(assets.find("app.00000000.css").is_none());
    assert!(assets.find("../Cargo.toml").is_none());
}
