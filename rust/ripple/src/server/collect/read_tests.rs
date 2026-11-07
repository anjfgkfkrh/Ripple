use super::*;

fn source(label: &str, error: Option<&str>) -> RunSource {
    RunSource {
        label: label.to_string(),
        group: Some(label.to_string()),
        fetched: 0,
        saved: 0,
        matched: 0,
        error: error.map(str::to_string),
    }
}

// it("실패한 소스를 같은 이유끼리 묶어 많은 것부터, 성공한 소스는 뺀다")
#[test]
fn groups_failures_by_reason_most_first() {
    let robots = "robots.txt가 막고 있어 건너뛰었습니다.";
    let groups = group_failures(&[
        source("가경제 경제", Some(robots)),
        source("나일보 전체", None),
        source("Finnhub 시장 뉴스", Some("HTTP 500")),
        source("다일보 전체", Some(robots)),
        source("라일보 전체", Some(robots)),
    ]);
    assert_eq!(
        groups,
        vec![
            FailureGroup {
                reason: robots.to_string(),
                labels: vec![
                    "가경제 경제".to_string(),
                    "다일보 전체".to_string(),
                    "라일보 전체".to_string()
                ],
            },
            FailureGroup {
                reason: "HTTP 500".to_string(),
                labels: vec!["Finnhub 시장 뉴스".to_string()],
            },
        ]
    );
    assert!(group_failures(&[source("나일보 전체", None)]).is_empty());
}

// Rust 전용: DB의 가장 최근 회차를 읽고(옛 기록처럼 group이 없는 소스·copied가 없는 평가 포함), 없으면 None
#[test]
fn reads_latest_run_with_old_json() {
    let db = crate::test_support::test_db();
    let conn = &db.conn;
    assert_eq!(latest_run(conn).unwrap(), None);
    assert_eq!(
        collection_totals(conn).unwrap(),
        CollectionTotals {
            articles: 0,
            matches: 0
        }
    );
    conn.execute(
        "INSERT INTO collection_runs (trigger, status, started_at, finished_at, since, sources, articles_new, matches_new, note, evaluation) \
         VALUES ('manual','success',1000,2000,500,?1,3,4,NULL,?2)",
        [
            r#"[{"id":"a","label":"옛 소스","fetched":5,"fresh":4,"saved":2,"matched":1,"ms":3}]"#,
            r#"{"evaluated":2,"relevant":1,"failed":0,"remaining":0,"costUsd":0.0012,"ms":9}"#,
        ],
    )
    .unwrap();
    let run = latest_run(conn).unwrap().unwrap();
    assert_eq!(run.status, RunStatus::Success);
    assert_eq!(run.sources[0].group, None);
    assert_eq!(run.sources[0].saved, 2);
    let evaluation = run.evaluation.unwrap();
    assert_eq!(evaluation.copied, 0);
    assert_eq!(evaluation.cost_usd, 0.0012);
    assert_eq!((run.articles_new, run.matches_new), (3, 4));
}
