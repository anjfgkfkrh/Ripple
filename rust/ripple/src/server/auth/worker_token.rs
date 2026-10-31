use crate::server::auth::config::MIN_SECRET_LENGTH;
use crate::server::env::{ConfigError, Env};
use crate::server::text::js_len;

/// web → worker 내부 API 공유 토큰(SPEC 8장). web·worker 둘 다 같은 값을 쓴다.
/// 없거나 짧으면 시작하지 않는다. 값은 오류 메시지에 넣지 않는다.
/// (`isAuthorizedBearer`는 R9에서 옮긴다.)
pub fn read_worker_api_token(env: &Env) -> Result<String, ConfigError> {
    let token = env.get("WORKER_API_TOKEN").unwrap_or("");
    if js_len(token) < MIN_SECRET_LENGTH {
        return Err(ConfigError(format!(
            "WORKER_API_TOKEN은 {MIN_SECRET_LENGTH}자 이상이어야 합니다."
        )));
    }
    Ok(token.to_string())
}
