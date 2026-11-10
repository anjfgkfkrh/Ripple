//! 설정 화면의 입력 검사와 저장(`settings-form.ts`). TS는 zod로 검사했고, 여기서는 같은 순서·같은 메시지로 직접 검사한다.
//! 항목마다 "처음 어긋난 검사"의 메시지만 남기는 것이 zod 오류를 `settingsErrors`로 줄인 결과와 같다.
use std::sync::LazyLock;

use regex::Regex;
use rusqlite::Connection;
use serde_json::{Map, Value};

use crate::server::db::client::atomically;
use crate::server::keywords::tree::clamp_depth;
use crate::server::schedule::plan::COLLECT_INTERVALS;
use crate::server::settings::{
    BUDGET_MONTHLY_USD, COLLECT_INTERVAL_MINUTES, EVALUATION_MAX_PER_RUN, EVALUATION_MIN_MAGNITUDE,
    EVALUATION_MIN_RELEVANCE, KEYWORDS_DEPTH, KEYWORDS_MIN_MAGNITUDE, KEYWORDS_MIN_RELEVANCE,
    KEYWORDS_REGENERATE_EVERY, LLM_MODEL, TOPICS_INTERVAL_HOURS, TOPICS_TIMES, read_list, read_num,
    read_str, write_list, write_num, write_str,
};
use crate::server::text::{js_len, js_number, js_parse_number, js_trim};
use crate::server::time::EpochMs;
use crate::server::topics::plan::TOPIC_TIME;

/// 토픽 갱신 특정 시각은 이만큼까지
pub const MAX_TOPIC_TIMES: usize = 12;

static MODEL_ID: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[A-Za-z0-9~][A-Za-z0-9._:/~-]*$").expect("정규식이 올바르다"));

/// JS 정규식 `[\s,]+`(JS의 `\s`는 BOM을 포함하고 U+0085를 포함하지 않는다)
static TIME_SEPARATOR: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        "[\u{9}-\u{d}\u{20}\u{a0}\u{1680}\u{2000}-\u{200a}\u{2028}\u{2029}\u{202f}\u{205f}\u{3000}\u{feff},]+",
    )
    .expect("정규식이 올바르다")
});

/// 시를 한 자리로 쓴 시각의 앞(`/^(\d):/`, JS의 `\d`는 ASCII 숫자만)
static SINGLE_DIGIT_HOUR: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^([0-9]):").expect("정규식이 올바르다"));

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsField {
    CollectIntervalMinutes,
    KeywordsDepth,
    KeywordsRegenerateEvery,
    MinRelevance,
    MinMagnitude,
    KeywordMinRelevance,
    KeywordMinMagnitude,
    TopicIntervalHours,
    TopicTimes,
    MaxPerRun,
    MonthlyBudgetUsd,
    LlmModel,
}

impl SettingsField {
    /// 화면·API가 쓰는 이름(TS 필드 이름 그대로)
    pub fn key(self) -> &'static str {
        match self {
            Self::CollectIntervalMinutes => "collectIntervalMinutes",
            Self::KeywordsDepth => "keywordsDepth",
            Self::KeywordsRegenerateEvery => "keywordsRegenerateEvery",
            Self::MinRelevance => "minRelevance",
            Self::MinMagnitude => "minMagnitude",
            Self::KeywordMinRelevance => "keywordMinRelevance",
            Self::KeywordMinMagnitude => "keywordMinMagnitude",
            Self::TopicIntervalHours => "topicIntervalHours",
            Self::TopicTimes => "topicTimes",
            Self::MaxPerRun => "maxPerRun",
            Self::MonthlyBudgetUsd => "monthlyBudgetUsd",
            Self::LlmModel => "llmModel",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RegenerateEvery {
    Daily,
    Weekly,
}

impl RegenerateEvery {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Daily => "daily",
            Self::Weekly => "weekly",
        }
    }
}

/// 설정 화면에서 바꿀 수 있는 항목(SPEC FR-8). Jev 모델·단가는 보이기만 한다(2026-10-21 결정).
#[derive(Debug, Clone, PartialEq)]
pub struct SettingsForm {
    pub collect_interval_minutes: f64,
    pub keywords_depth: f64,
    pub keywords_regenerate_every: RegenerateEvery,
    pub min_relevance: f64,
    pub min_magnitude: f64,
    pub keyword_min_relevance: f64,
    pub keyword_min_magnitude: f64,
    pub topic_interval_hours: f64,
    pub topic_times: Vec<String>,
    pub max_per_run: f64,
    pub monthly_budget_usd: f64,
    pub llm_model: String,
}

/// 항목별 첫 오류 메시지(스키마의 항목 순서, `settingsErrors`의 결과). 본문이 객체가 아니면 항목 없이 비어 있다.
pub type SettingsErrors = Vec<(SettingsField, String)>;

/// JS가 값을 글자로 바꾸는 방식(배열은 `join(",")`, 빈 값은 빈 글자, 객체는 `[object Object]`)
fn js_to_string(value: &Value) -> String {
    match value {
        Value::Null => String::new(),
        Value::Bool(flag) => flag.to_string(),
        Value::Number(number) => js_number(number.as_f64().unwrap_or(f64::NAN)),
        Value::String(text) => text.clone(),
        Value::Array(items) => items.iter().map(js_to_string).collect::<Vec<_>>().join(","),
        Value::Object(_) => "[object Object]".to_string(),
    }
}

/// zod `z.coerce.number()`가 쓰는 JS `Number(x)`(없는 값은 `undefined`라 NaN)
fn coerce_number(value: Option<&Value>) -> f64 {
    match value {
        None => f64::NAN,
        Some(Value::Null) => 0.0,
        Some(Value::Bool(flag)) => f64::from(u8::from(*flag)),
        Some(Value::Number(number)) => number.as_f64().unwrap_or(f64::NAN),
        Some(other) => js_parse_number(&js_to_string(other)),
    }
}

/// zod가 값의 종류를 말하는 글자
fn type_name(value: Option<&Value>) -> &'static str {
    match value {
        None => "undefined",
        Some(Value::Null) => "null",
        Some(Value::Bool(_)) => "boolean",
        Some(Value::Number(_)) => "number",
        Some(Value::String(_)) => "string",
        Some(Value::Array(_)) => "array",
        Some(Value::Object(_)) => "object",
    }
}

/// 메시지를 정하지 않은 `z.coerce.number()`의 형식 오류
fn number_type_error(number: f64) -> String {
    let received = if number.is_nan() {
        "NaN"
    } else if number > 0.0 {
        "Infinity"
    } else {
        "-Infinity"
    };
    format!("Invalid input: expected number, received {received}")
}

/// `Number.isSafeInteger`와 같은 기준(zod의 `.int()`)
fn is_safe_integer(number: f64) -> bool {
    number.fract() == 0.0 && number.abs() <= 9_007_199_254_740_991.0
}

/// JS `Math.round`(.5는 위로)
fn js_round(number: f64) -> f64 {
    (number + 0.5).floor()
}

/// 숫자 항목 하나의 검사. `type_message`는 숫자로 읽지 못할 때의 메시지(없으면 zod 기본 메시지).
struct NumberRule {
    type_message: Option<&'static str>,
    int_message: Option<&'static str>,
    min: Option<(f64, &'static str)>,
    max: Option<(f64, &'static str)>,
}

fn check_number(value: Option<&Value>, rule: &NumberRule) -> Result<f64, String> {
    let number = coerce_number(value);
    if !number.is_finite() {
        return Err(rule
            .type_message
            .map_or_else(|| number_type_error(number), str::to_string));
    }
    if let Some(message) = rule.int_message
        && !is_safe_integer(number)
    {
        return Err(message.to_string());
    }
    if let Some((min, message)) = rule.min
        && number < min
    {
        return Err(message.to_string());
    }
    if let Some((max, message)) = rule.max
        && number > max
    {
        return Err(message.to_string());
    }
    Ok(number)
}

/// 0~10 점수 기준(소수 첫째 자리까지)
fn check_score(value: Option<&Value>) -> Result<f64, String> {
    let number = check_number(
        value,
        &NumberRule {
            type_message: Some("숫자를 넣으세요."),
            int_message: None,
            min: Some((0.0, "0~10 사이로 넣으세요.")),
            max: Some((10.0, "0~10 사이로 넣으세요.")),
        },
    )?;
    Ok(js_round(number * 10.0) / 10.0)
}

fn check_collect_interval(value: Option<&Value>) -> Result<f64, String> {
    let number = check_number(
        value,
        &NumberRule {
            type_message: None,
            int_message: None,
            min: None,
            max: None,
        },
    )?;
    if COLLECT_INTERVALS
        .iter()
        .any(|interval| *interval as f64 == number)
    {
        Ok(number)
    } else {
        Err("15·30·60·180분 중에서 고르세요.".to_string())
    }
}

fn check_regenerate_every(value: Option<&Value>) -> Result<RegenerateEvery, String> {
    match value.and_then(Value::as_str) {
        Some("daily") => Ok(RegenerateEvery::Daily),
        Some("weekly") => Ok(RegenerateEvery::Weekly),
        _ => Err("하루 1회나 일주일 1회를 고르세요.".to_string()),
    }
}

fn check_topic_times(value: Option<&Value>) -> Result<Vec<String>, String> {
    let Some(Value::String(text)) = value else {
        return Err("09:00, 15:30처럼 넣으세요.".to_string());
    };
    if js_len(text) > 200 {
        return Err("너무 깁니다.".to_string());
    }
    let times: Vec<String> = TIME_SEPARATOR
        .split(text)
        // 9:00처럼 시를 한 자리로 써도 받는다.
        .map(|time| {
            SINGLE_DIGIT_HOUR
                .replace(js_trim(time), "0$1:")
                .into_owned()
        })
        .filter(|time| !time.is_empty())
        .collect();
    if !times.iter().all(|time| TOPIC_TIME.is_match(time)) {
        return Err("09:00, 15:30처럼 24시간제 HH:MM으로 넣으세요.".to_string());
    }
    let mut unique = times;
    unique.sort();
    unique.dedup();
    if unique.len() > MAX_TOPIC_TIMES {
        return Err(format!("{MAX_TOPIC_TIMES}개까지 넣을 수 있습니다."));
    }
    Ok(unique)
}

fn check_model(value: Option<&Value>) -> Result<String, String> {
    let Some(Value::String(text)) = value else {
        return Err(format!(
            "Invalid input: expected string, received {}",
            type_name(value)
        ));
    };
    let model = js_trim(text);
    if model.is_empty() {
        return Err("모델 ID를 넣으세요.".to_string());
    }
    if js_len(model) > 200 {
        return Err("모델 ID가 너무 깁니다.".to_string());
    }
    if !MODEL_ID.is_match(model) {
        return Err("모델 ID 형식이 아닙니다(예: anthropic/claude-haiku-5.5).".to_string());
    }
    Ok(model.to_string())
}

/// 검사 결과가 오류면 항목 이름과 함께 모아 두고 None으로 바꾼다.
fn take<T>(
    errors: &mut SettingsErrors,
    field: SettingsField,
    result: Result<T, String>,
) -> Option<T> {
    match result {
        Ok(value) => Some(value),
        Err(message) => {
            errors.push((field, message));
            None
        }
    }
}

/// 본문(JSON)을 검사한다. 어긋난 항목이 있으면 항목별 첫 메시지를 스키마 순서로 돌려준다.
pub fn parse_settings_form(raw: &Value) -> Result<SettingsForm, SettingsErrors> {
    let Value::Object(body) = raw else {
        return Err(Vec::new());
    };
    let get = |field: SettingsField| body.get(field.key());
    let mut errors: SettingsErrors = Vec::new();
    use SettingsField as F;
    let collect_interval_minutes = take(
        &mut errors,
        F::CollectIntervalMinutes,
        check_collect_interval(get(F::CollectIntervalMinutes)),
    );
    let keywords_depth = take(
        &mut errors,
        F::KeywordsDepth,
        check_number(
            get(F::KeywordsDepth),
            &NumberRule {
                type_message: None,
                int_message: Some("1~2 사이 정수로 넣으세요."),
                min: Some((1.0, "1~2 사이로 넣으세요.")),
                max: Some((2.0, "1~2 사이로 넣으세요.")),
            },
        ),
    );
    let keywords_regenerate_every = take(
        &mut errors,
        F::KeywordsRegenerateEvery,
        check_regenerate_every(get(F::KeywordsRegenerateEvery)),
    );
    let min_relevance = take(
        &mut errors,
        F::MinRelevance,
        check_score(get(F::MinRelevance)),
    );
    let min_magnitude = take(
        &mut errors,
        F::MinMagnitude,
        check_score(get(F::MinMagnitude)),
    );
    let keyword_min_relevance = take(
        &mut errors,
        F::KeywordMinRelevance,
        check_score(get(F::KeywordMinRelevance)),
    );
    let keyword_min_magnitude = take(
        &mut errors,
        F::KeywordMinMagnitude,
        check_score(get(F::KeywordMinMagnitude)),
    );
    let topic_interval_hours = take(
        &mut errors,
        F::TopicIntervalHours,
        check_number(
            get(F::TopicIntervalHours),
            &NumberRule {
                type_message: Some("숫자를 넣으세요."),
                int_message: Some("0~24 사이 정수로 넣으세요."),
                min: Some((0.0, "0~24 사이로 넣으세요.")),
                max: Some((24.0, "0~24 사이로 넣으세요.")),
            },
        ),
    );
    let topic_times = take(
        &mut errors,
        F::TopicTimes,
        check_topic_times(get(F::TopicTimes)),
    );
    let max_per_run = take(
        &mut errors,
        F::MaxPerRun,
        check_number(
            get(F::MaxPerRun),
            &NumberRule {
                type_message: Some("숫자를 넣으세요."),
                int_message: Some("정수로 넣으세요."),
                min: Some((10.0, "10~1000 사이로 넣으세요.")),
                max: Some((1000.0, "10~1000 사이로 넣으세요.")),
            },
        ),
    );
    let monthly_budget_usd = take(
        &mut errors,
        F::MonthlyBudgetUsd,
        check_number(
            get(F::MonthlyBudgetUsd),
            &NumberRule {
                type_message: Some("숫자를 넣으세요."),
                int_message: None,
                min: Some((0.1, "$0.1~$1000 사이로 넣으세요.")),
                max: Some((1000.0, "$0.1~$1000 사이로 넣으세요.")),
            },
        )
        .map(|value| js_round(value * 100.0) / 100.0),
    );
    let llm_model = take(&mut errors, F::LlmModel, check_model(get(F::LlmModel)));
    match (
        collect_interval_minutes,
        keywords_depth,
        keywords_regenerate_every,
        min_relevance,
        min_magnitude,
        keyword_min_relevance,
        keyword_min_magnitude,
        topic_interval_hours,
        topic_times,
        max_per_run,
        monthly_budget_usd,
        llm_model,
    ) {
        (
            Some(collect_interval_minutes),
            Some(keywords_depth),
            Some(keywords_regenerate_every),
            Some(min_relevance),
            Some(min_magnitude),
            Some(keyword_min_relevance),
            Some(keyword_min_magnitude),
            Some(topic_interval_hours),
            Some(topic_times),
            Some(max_per_run),
            Some(monthly_budget_usd),
            Some(llm_model),
        ) => Ok(SettingsForm {
            collect_interval_minutes,
            keywords_depth,
            keywords_regenerate_every,
            min_relevance,
            min_magnitude,
            keyword_min_relevance,
            keyword_min_magnitude,
            topic_interval_hours,
            topic_times,
            max_per_run,
            monthly_budget_usd,
            llm_model,
        }),
        _ => Err(errors),
    }
}

/// 항목별 오류를 응답의 `fields` 객체로 만든다.
pub fn settings_errors_json(errors: &SettingsErrors) -> Value {
    let mut fields = Map::new();
    for (field, message) in errors {
        fields.insert(field.key().to_string(), Value::String(message.clone()));
    }
    Value::Object(fields)
}

/// 지금 저장된 값(없으면 기본값)을 화면 형식으로 읽는다.
pub fn read_settings_form(conn: &Connection) -> rusqlite::Result<SettingsForm> {
    Ok(SettingsForm {
        collect_interval_minutes: read_num(conn, &COLLECT_INTERVAL_MINUTES)?,
        // 저장된 깊이가 3·4여도(17단계 전 값) 화면에는 2로 보이고, 저장하면 2가 된다.
        keywords_depth: clamp_depth(read_num(conn, &KEYWORDS_DEPTH)?) as f64,
        keywords_regenerate_every: if read_str(conn, &KEYWORDS_REGENERATE_EVERY)? == "weekly" {
            RegenerateEvery::Weekly
        } else {
            RegenerateEvery::Daily
        },
        min_relevance: read_num(conn, &EVALUATION_MIN_RELEVANCE)?,
        min_magnitude: read_num(conn, &EVALUATION_MIN_MAGNITUDE)?,
        keyword_min_relevance: read_num(conn, &KEYWORDS_MIN_RELEVANCE)?,
        keyword_min_magnitude: read_num(conn, &KEYWORDS_MIN_MAGNITUDE)?,
        topic_interval_hours: read_num(conn, &TOPICS_INTERVAL_HOURS)?,
        topic_times: read_list(conn, &TOPICS_TIMES)?,
        max_per_run: read_num(conn, &EVALUATION_MAX_PER_RUN)?,
        monthly_budget_usd: read_num(conn, &BUDGET_MONTHLY_USD)?,
        llm_model: read_str(conn, &LLM_MODEL)?,
    })
}

/// 검사를 마친 값을 한 번에 저장한다(worker만 부른다, D-36). 바뀐 항목을 스키마 순서로 돌려준다.
pub fn write_settings_form(
    conn: &Connection,
    form: &SettingsForm,
    now: EpochMs,
) -> rusqlite::Result<Vec<SettingsField>> {
    let before = read_settings_form(conn)?;
    atomically(conn, || {
        write_num(
            conn,
            &COLLECT_INTERVAL_MINUTES,
            form.collect_interval_minutes,
            now,
        )?;
        write_num(conn, &KEYWORDS_DEPTH, form.keywords_depth, now)?;
        write_str(
            conn,
            &KEYWORDS_REGENERATE_EVERY,
            form.keywords_regenerate_every.as_str(),
            now,
        )?;
        write_num(conn, &EVALUATION_MIN_RELEVANCE, form.min_relevance, now)?;
        write_num(conn, &EVALUATION_MIN_MAGNITUDE, form.min_magnitude, now)?;
        write_num(
            conn,
            &KEYWORDS_MIN_RELEVANCE,
            form.keyword_min_relevance,
            now,
        )?;
        write_num(
            conn,
            &KEYWORDS_MIN_MAGNITUDE,
            form.keyword_min_magnitude,
            now,
        )?;
        write_num(conn, &TOPICS_INTERVAL_HOURS, form.topic_interval_hours, now)?;
        write_list(conn, &TOPICS_TIMES, &form.topic_times, now)?;
        write_num(conn, &EVALUATION_MAX_PER_RUN, form.max_per_run, now)?;
        write_num(conn, &BUDGET_MONTHLY_USD, form.monthly_budget_usd, now)?;
        write_str(conn, &LLM_MODEL, &form.llm_model, now)?;
        Ok(())
    })?;
    use SettingsField as F;
    let flags = [
        (
            F::CollectIntervalMinutes,
            form.collect_interval_minutes == before.collect_interval_minutes,
        ),
        (
            F::KeywordsDepth,
            form.keywords_depth == before.keywords_depth,
        ),
        (
            F::KeywordsRegenerateEvery,
            form.keywords_regenerate_every == before.keywords_regenerate_every,
        ),
        (F::MinRelevance, form.min_relevance == before.min_relevance),
        (F::MinMagnitude, form.min_magnitude == before.min_magnitude),
        (
            F::KeywordMinRelevance,
            form.keyword_min_relevance == before.keyword_min_relevance,
        ),
        (
            F::KeywordMinMagnitude,
            form.keyword_min_magnitude == before.keyword_min_magnitude,
        ),
        (
            F::TopicIntervalHours,
            form.topic_interval_hours == before.topic_interval_hours,
        ),
        (F::TopicTimes, form.topic_times == before.topic_times),
        (F::MaxPerRun, form.max_per_run == before.max_per_run),
        (
            F::MonthlyBudgetUsd,
            form.monthly_budget_usd == before.monthly_budget_usd,
        ),
        (F::LlmModel, form.llm_model == before.llm_model),
    ];
    Ok(flags
        .into_iter()
        .filter(|(_, same)| !same)
        .map(|(field, _)| field)
        .collect())
}

#[cfg(test)]
#[path = "settings_form_tests.rs"]
mod tests;
