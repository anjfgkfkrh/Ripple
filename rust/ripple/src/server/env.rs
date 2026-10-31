use std::collections::HashMap;

use crate::server::text::{js_parse_number, js_trim};

/// 환경 변수 읽기·검사에서 나는 오류. 메시지는 화면·로그에 그대로 나간다(비밀 값은 넣지 않는다).
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{0}")]
pub struct ConfigError(pub String);

/// 환경 변수 묶음. 실제 실행은 `Env::from_process()`, 테스트는 `Env::from_pairs`로 넣는다.
#[derive(Debug, Clone, Default)]
pub struct Env(HashMap<String, String>);

impl Env {
    pub fn from_process() -> Self {
        Env(std::env::vars_os()
            .map(|(k, v)| {
                (
                    k.to_string_lossy().into_owned(),
                    v.to_string_lossy().into_owned(),
                )
            })
            .collect())
    }

    pub fn from_pairs<const N: usize>(pairs: [(&str, &str); N]) -> Self {
        Env(pairs
            .iter()
            .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
            .collect())
    }

    /// 값을 하나 더한 새 묶음(테스트에서 기본 값에 덧붙일 때)
    pub fn with(mut self, name: &str, value: &str) -> Self {
        self.0.insert(name.to_string(), value.to_string());
        self
    }

    pub fn get(&self, name: &str) -> Option<&str> {
        self.0.get(name).map(String::as_str)
    }

    /// 값을 앞뒤 공백 없이 돌려준다. 없거나 비면 None(TS의 `env[name]?.trim() || undefined`).
    pub fn get_trimmed(&self, name: &str) -> Option<&str> {
        self.get(name).map(js_trim).filter(|v| !v.is_empty())
    }
}

/// 값이 없으면 바로 실패한다. 비밀 값이 들어올 수 있으므로 값은 메시지에 넣지 않는다.
pub fn require_env(name: &str, env: &Env) -> Result<String, ConfigError> {
    env.get_trimmed(name)
        .map(str::to_string)
        .ok_or_else(|| ConfigError(format!("환경변수 {name}이(가) 설정되지 않았습니다.")))
}

pub fn read_port(name: &str, fallback: u16, env: &Env) -> Result<u16, ConfigError> {
    let Some(raw) = env.get_trimmed(name) else {
        return Ok(fallback);
    };
    let port = js_parse_number(raw);
    if port.fract() != 0.0 || !(1.0..=65535.0).contains(&port) {
        return Err(ConfigError(format!(
            "환경변수 {name}의 값이 올바른 포트 번호가 아닙니다."
        )));
    }
    // 위에서 1~65535 정수임을 확인했다.
    Ok(port as u16)
}

#[cfg(test)]
#[path = "env_tests.rs"]
mod tests;
