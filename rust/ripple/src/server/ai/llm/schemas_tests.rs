use serde_json::Value;

use super::*;

/// 구조화 출력 strict 모드 조건: 모든 object는 additionalProperties false이고 required가 properties 전부다.
fn assert_strict(schema: &Value, at: &str) {
    if schema.get("type").and_then(Value::as_str) == Some("object") {
        assert_eq!(schema["additionalProperties"], Value::Bool(false), "{at}");
        let props: Vec<&str> = schema["properties"]
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        let required: Vec<&str> = schema["required"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap())
            .collect();
        assert_eq!(props, required, "{at}");
        for (name, child) in schema["properties"].as_object().unwrap() {
            assert_strict(child, &format!("{at}.{name}"));
        }
    }
    if let Some(items) = schema.get("items") {
        assert_strict(items, &format!("{at}[]"));
    }
}

#[test]
fn schemas_are_strict_and_have_no_meta_field() {
    for (name, schema) in [
        ("stock_candidates", stock_candidates()),
        ("keyword_tree", keyword_tree()),
        ("keyword_update", keyword_update()),
        ("topic_refresh", topic_refresh()),
    ] {
        assert!(schema.get("$schema").is_none(), "{name}");
        assert_strict(&schema, name);
    }
}

#[test]
fn keyword_tree_schema_asks_for_aliases_anchors_and_keywords() {
    let schema = keyword_tree();
    let keys: Vec<&str> = schema["properties"]
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    assert_eq!(keys, ["aliases", "anchors", "keywords"]);
    assert_eq!(schema["properties"]["aliases"]["items"]["type"], "string");
    assert_eq!(schema["properties"]["anchors"]["items"]["type"], "string");
    let item_keys: Vec<&str> = schema["properties"]["keywords"]["items"]["properties"]
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    assert_eq!(
        item_keys,
        ["level", "keyword", "keywordEn", "parent", "reason"]
    );
}

#[test]
fn keyword_update_schema_asks_for_removes_adds_anchors_and_aliases() {
    let schema = keyword_update();
    let keys: Vec<&str> = schema["properties"]
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    assert_eq!(
        keys,
        ["remove", "add", "anchorsRemove", "anchorsAdd", "aliasesAdd"]
    );
    let remove_keys: Vec<&str> = schema["properties"]["remove"]["items"]["properties"]
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    assert_eq!(remove_keys, ["keyword", "reason"]);
    // 더할 키워드는 전체 생성과 같은 항목 모양
    assert_eq!(
        schema["properties"]["add"]["items"],
        keyword_tree()["properties"]["keywords"]["items"]
    );
}
