//! web 시작 설정. 비밀번호·서명 키·worker 토큰이 없거나 짧으면 시작하지 않는다(SPEC 8장, `instrumentation-node.ts`).
use crate::server::auth::config::{AuthConfig, read_auth_config};
use crate::server::auth::worker_token::read_worker_api_token;
use crate::server::env::{ConfigError, Env, read_port};

pub struct WebConfig {
    pub auth: AuthConfig,
    pub worker_token: String,
    pub host: String,
    pub port: u16,
    /// `DATABASE_PATH`(없으면 화면에 오류로 보인다, `requireEnv`)
    pub database_path: Option<String>,
    pub worker_url: Option<String>,
    pub tunnel_metrics_url: Option<String>,
    /// `PUBLIC_URL`(15단계, 상태 화면 표시용 고정 주소. 형식 검사는 화면에서 한다)
    pub public_url: Option<String>,
}

impl std::fmt::Debug for WebConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WebConfig")
            .field("host", &self.host)
            .field("port", &self.port)
            .field("database_path", &self.database_path)
            .field("worker_url", &self.worker_url)
            .finish_non_exhaustive()
    }
}

pub fn read_web_config(env: &Env) -> Result<WebConfig, ConfigError> {
    let auth = read_auth_config(env)?;
    let worker_token = read_worker_api_token(env)?;
    Ok(WebConfig {
        auth,
        worker_token,
        host: env.get_trimmed("HOSTNAME").unwrap_or("0.0.0.0").to_string(),
        port: read_port("PORT", 3000, env)?,
        database_path: env.get_trimmed("DATABASE_PATH").map(str::to_string),
        worker_url: env.get_trimmed("WORKER_URL").map(str::to_string),
        tunnel_metrics_url: env.get_trimmed("TUNNEL_METRICS_URL").map(str::to_string),
        public_url: env.get_trimmed("PUBLIC_URL").map(str::to_string),
    })
}

#[cfg(test)]
#[path = "config_tests.rs"]
mod tests;
