//! web 요청이 함께 쓰는 값.
use std::sync::Arc;

use crate::server::auth::config::AuthConfig;
use crate::server::auth::rate_limit::LoginRateLimiter;
use crate::server::db::web::WebDb;
use crate::server::http_client::new_client;
use crate::server::time::{EpochMs, now_ms};
use crate::server::worker_client::WorkerApi;
use crate::web::assets::Assets;
use crate::web::config::WebConfig;

pub type NowFn = Arc<dyn Fn() -> EpochMs + Send + Sync>;

pub struct WebState {
    pub auth: AuthConfig,
    /// web 프로세스 하나에 하나(로그인 실패 제한, 메모리에만 둔다)
    pub limiter: LoginRateLimiter,
    pub db: WebDb,
    pub http: reqwest::Client,
    pub worker_url: Option<String>,
    pub worker_token: String,
    pub tunnel_metrics_url: Option<String>,
    pub public_url: Option<String>,
    pub assets: Assets,
    now: NowFn,
}

impl WebState {
    pub fn new(config: &WebConfig, now: Option<NowFn>) -> Self {
        let database_path = config
            .database_path
            .clone()
            .map(std::path::PathBuf::from)
            .ok_or_else(|| "환경변수 DATABASE_PATH이(가) 설정되지 않았습니다.".to_string());
        Self {
            auth: config.auth.clone(),
            limiter: LoginRateLimiter::default(),
            db: WebDb::new(database_path),
            http: new_client(),
            worker_url: config.worker_url.clone(),
            worker_token: config.worker_token.clone(),
            tunnel_metrics_url: config.tunnel_metrics_url.clone(),
            public_url: config.public_url.clone(),
            assets: Assets::load(),
            now: now.unwrap_or_else(|| Arc::new(now_ms)),
        }
    }

    pub fn now(&self) -> EpochMs {
        (self.now)()
    }

    /// worker 내부 API. `WORKER_URL`이 없으면 None
    pub fn worker_api(&self) -> Option<WorkerApi> {
        Some(WorkerApi::new(
            self.http.clone(),
            self.worker_url.clone()?,
            self.worker_token.clone(),
        ))
    }
}
