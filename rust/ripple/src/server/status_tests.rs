use serde_json::json;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use super::*;
use crate::server::worker_client::{WorkerHealth, fetch_worker_health};
use crate::test_support::test_db_file;

fn http() -> reqwest::Client {
    reqwest::Client::new()
}

async fn healthy_worker() -> MockServer {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/health"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({"status": "ok", "db": "ok", "listings": {"KR": 0, "US": 0}})),
        )
        .mount(&server)
        .await;
    server
}

// describe("getSystemStatus") — it("web이 DB를 읽기 전용으로 읽고 worker 내부 API에 닿으면 둘 다 정상")
#[tokio::test]
async fn both_ok_when_db_and_worker_reachable() {
    let file = test_db_file();
    // WAL은 쓰는 쪽(worker)이 켠다: TS 시험도 writer에서 enableWal을 부른다.
    {
        let writer = crate::server::db::client::open_db(
            &file.path,
            crate::server::db::client::DbAccess::ReadWrite,
        )
        .unwrap();
        crate::server::db::client::enable_wal(&writer).unwrap();
    }
    let worker = healthy_worker().await;
    let db = WebDb::new(Ok(file.path.clone()));

    let status = get_system_status(&db, Some(&worker.uri()), &http()).await;

    assert_eq!(
        status.database,
        DatabaseStatus::Ok {
            journal_mode: "wal".to_string(),
            settings_count: 0
        }
    );
    assert_eq!(
        status.worker,
        WorkerHealth::Ok {
            listings: Some((0, 0))
        }
    );
    assert!(status.checked_at > 0);
}

// it("DB를 열 수 없거나 worker 주소가 없으면 각각 오류로 표시한다")
#[tokio::test]
async fn shows_errors_without_db_or_worker_url() {
    let db = WebDb::new(Err(
        "환경변수 DATABASE_PATH이(가) 설정되지 않았습니다.".to_string()
    ));
    let status = get_system_status(&db, None, &http()).await;
    assert_eq!(
        status.database,
        DatabaseStatus::Err {
            reason: "환경변수 DATABASE_PATH이(가) 설정되지 않았습니다.".to_string()
        }
    );
    assert_eq!(
        status.worker,
        WorkerHealth::Err {
            reason: "환경변수 WORKER_URL이 설정되지 않았습니다.".to_string()
        }
    );
}

// describe("fetchWorkerHealth") — it("worker가 꺼져 있으면 실패 이유를 돌려준다")
#[tokio::test]
async fn unreachable_worker_gives_reason() {
    let port = {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        listener.local_addr().unwrap().port()
    };
    let health = fetch_worker_health(&http(), &format!("http://127.0.0.1:{port}"), 2000).await;
    let WorkerHealth::Err { reason } = health else {
        panic!("실패여야 한다");
    };
    assert!(!reason.is_empty());
}

// it("200이 아니거나 형식이 다르면 실패로 본다")
#[tokio::test]
async fn non_200_or_wrong_body_is_failure() {
    let s500 = MockServer::start().await;
    Mock::given(path("/health"))
        .respond_with(ResponseTemplate::new(500))
        .mount(&s500)
        .await;
    assert_eq!(
        fetch_worker_health(&http(), &s500.uri(), 2000).await,
        WorkerHealth::Err {
            reason: "HTTP 500".to_string()
        }
    );

    let wrong = MockServer::start().await;
    Mock::given(path("/health"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({})))
        .mount(&wrong)
        .await;
    assert_eq!(
        fetch_worker_health(&http(), &wrong.uri(), 2000).await,
        WorkerHealth::Err {
            reason: "응답 형식이 올바르지 않습니다.".to_string()
        }
    );
}

// it("응답이 늦으면 제한 시간 뒤 실패한다")
#[tokio::test]
async fn slow_worker_times_out() {
    let slow = MockServer::start().await;
    Mock::given(path("/health"))
        .respond_with(ResponseTemplate::new(200).set_delay(std::time::Duration::from_secs(5)))
        .mount(&slow)
        .await;
    let started = std::time::Instant::now();
    let health = fetch_worker_health(&http(), &slow.uri(), 200).await;
    assert!(matches!(health, WorkerHealth::Err { .. }));
    assert!(started.elapsed() < std::time::Duration::from_secs(2));
}

// Rust 전용: listings가 없는 응답은 정상이지만 개수는 모른다
#[tokio::test]
async fn ok_without_listings() {
    let server = MockServer::start().await;
    Mock::given(path("/health"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"status": "ok"})))
        .mount(&server)
        .await;
    assert_eq!(
        fetch_worker_health(&http(), &server.uri(), 2000).await,
        WorkerHealth::Ok { listings: None }
    );
}
