//! web 시작 조립: 설정을 읽고, 라우터를 만들고, 종료 신호를 받을 때까지 듣는다.
use std::sync::Arc;

use crate::server::env::Env;
use crate::server::log::{log, log_error};
use crate::web::config::read_web_config;
use crate::web::state::WebState;
use crate::web::web_router;

/// web을 실행하고 종료 코드를 돌려준다(설정 오류·시작 실패 1, 종료 신호로 끝나면 0).
pub fn run(env: &Env) -> i32 {
    let config = match read_web_config(env) {
        Ok(config) => config,
        Err(error) => {
            eprintln!("[설정 오류] {error} .env를 확인하세요.");
            return 1;
        }
    };
    let runtime = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(error) => {
            log_error("web 시작 실패", &error);
            return 1;
        }
    };
    let state = Arc::new(WebState::new(&config, None));
    let address = format!("{}:{}", config.host, config.port);
    match runtime.block_on(serve(state, &address)) {
        Ok(()) => 0,
        Err(error) => {
            log_error("web 시작 실패", &error);
            1
        }
    }
}

async fn serve(state: Arc<WebState>, address: &str) -> Result<(), Box<dyn std::error::Error>> {
    let listener = tokio::net::TcpListener::bind(address).await?;
    log(&format!("web 대기 중: http://{address}"));
    axum::serve(listener, web_router(state))
        .with_graceful_shutdown(async {
            let signal = wait_for_signal().await;
            log(&format!("{signal} 수신, 종료합니다."));
        })
        .await?;
    Ok(())
}

async fn wait_for_signal() -> &'static str {
    use tokio::signal::unix::{SignalKind, signal};
    let mut term = signal(SignalKind::terminate()).expect("SIGTERM 처리를 등록하지 못했습니다");
    let mut int = signal(SignalKind::interrupt()).expect("SIGINT 처리를 등록하지 못했습니다");
    tokio::select! {
        _ = term.recv() => "SIGTERM",
        _ = int.recv() => "SIGINT",
    }
}
