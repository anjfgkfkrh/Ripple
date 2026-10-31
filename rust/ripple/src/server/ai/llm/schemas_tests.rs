use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::Value;
use tempfile::TempDir;

use super::*;

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap()
}

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
        ("topic_refresh", topic_refresh()),
    ] {
        assert!(schema.get("$schema").is_none(), "{name}");
        assert_strict(&schema, name);
    }
}

#[test]
fn schema_files_equal_what_ts_dump_script_makes_now() {
    // 설계 3.11: 파일이 TS의 zod 스키마와 어긋나지 않는지 TS를 다시 돌려 비교한다.
    let dir = TempDir::new().unwrap();
    let output = Command::new("pnpm")
        .args(["exec", "tsx", "rust/tools/dump-schemas.ts"])
        .arg(dir.path())
        .current_dir(repo_root())
        .output()
        .expect("pnpm을 실행할 수 없습니다");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    for (name, schema) in [
        ("stock_candidates", stock_candidates()),
        ("keyword_tree", keyword_tree()),
        ("topic_refresh", topic_refresh()),
    ] {
        let fresh = std::fs::read_to_string(dir.path().join(format!("{name}.json"))).unwrap();
        assert_eq!(
            serde_json::to_string_pretty(&schema).unwrap() + "\n",
            fresh,
            "{name}"
        );
    }
}
