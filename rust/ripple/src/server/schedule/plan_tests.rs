use super::*;
use crate::server::settings::{
    KEYWORDS_AUTO_REGENERATED_AT as AUTO_AT, KEYWORDS_REGENERATE_EVERY as EVERY, write_num,
    write_str,
};
use crate::server::stocks::repository::{
    RegisterInput, RegisterResult, delete_stock, register_stock,
};
use crate::test_support::{TestDb, ms, seed_listings, seed_topic, test_db};
use rusqlite::params;

fn testdb() -> TestDb {
    let mut db = test_db();
    seed_listings(&mut db.conn);
    db
}

// 한국 시간 2026-10-21(수) 12:18
fn now() -> EpochMs {
    ms("2026-10-21T03:18:00Z")
}
const MIN: i64 = 60 * 1000;
fn at(minutes: i64) -> EpochMs {
    now() + minutes * MIN
}

fn add_stock(conn: &Connection, code: &str) -> i64 {
    match register_stock(
        conn,
        &RegisterInput {
            market: crate::server::db::schema::Market::Kr,
            code,
            user_input: code,
            name_ko: None,
        },
        now(),
    )
    .unwrap()
    {
        RegisterResult::Created(stock) => stock.id,
        other => panic!("표본에 없음: {other:?}"),
    }
}

fn add_run(conn: &Connection, started_at: EpochMs, status: &str, trigger: &str) {
    conn.execute(
        "INSERT INTO collection_runs (trigger, status, started_at, since) VALUES (?1, ?2, ?3, ?3)",
        params![trigger, status, started_at],
    )
    .unwrap();
}

fn plan(db: &TestDb, now: EpochMs) -> CollectionPlan {
    collection_plan(&db.conn, now).unwrap()
}

// describe("collectionPlan (D-01, D-24, D-31)")
#[test]
fn rests_when_no_stock_is_registered() {
    // it("등록한 종목이 없으면 쉰다(회차를 만들지 않음)")
    let db = testdb();
    let p = plan(&db, now());
    assert!(p.idle && !p.due);
    let id = add_stock(&db.conn, "005930");
    delete_stock(&db.conn, id, now()).unwrap();
    let p = plan(&db, now());
    assert!(p.idle && !p.due);
}

#[test]
fn starts_right_away_when_it_never_ran() {
    // it("한 번도 돈 적이 없으면 바로 시작한다")
    let db = testdb();
    add_stock(&db.conn, "005930");
    assert_eq!(
        plan(&db, now()),
        CollectionPlan {
            interval_minutes: 30,
            idle: false,
            running: false,
            next_at: now(),
            retrying: false,
            due: true,
            trigger: RunTrigger::Scheduled,
        }
    );
}

#[test]
fn next_is_last_successful_run_start_plus_interval() {
    // it("마지막으로 성공한 회차(일부 실패, 직접 실행 포함) 시작 + 30분")
    let db = testdb();
    add_stock(&db.conn, "005930");
    add_run(&db.conn, at(-50), "success", "scheduled");
    add_run(&db.conn, at(-20), "partial", "manual");
    let p = plan(&db, now());
    assert_eq!((p.next_at, p.due), (at(10), false));
    let p = plan(&db, at(10));
    assert!(p.due);
    assert_eq!(p.trigger, RunTrigger::Scheduled);
}

#[test]
fn more_than_five_minutes_late_is_a_catch_up() {
    // it("예정보다 5분 넘게 늦으면(꺼져 있었거나 절전) 누락 보정으로 시작한다")
    let db = testdb();
    add_stock(&db.conn, "005930");
    add_run(&db.conn, at(-120), "success", "scheduled");
    let p = plan(&db, now());
    assert_eq!(
        (p.next_at, p.due, p.trigger),
        (at(-90), true, RunTrigger::CatchUp)
    );
    // 5분까지는 정기
    let p = plan(&db, at(-85));
    assert_eq!((p.due, p.trigger), (true, RunTrigger::Scheduled));
    // 5분 1밀리초부터 누락 보정
    let p = plan(&db, at(-85) + 1);
    assert_eq!(p.trigger, RunTrigger::CatchUp);
}

#[test]
fn whole_failure_retries_after_five_minutes_until_success() {
    // it("회차 전체가 실패하면 5분 뒤 다시, 성공할 때까지 5분마다(D-24)")
    let db = testdb();
    add_stock(&db.conn, "005930");
    add_run(&db.conn, at(-60), "success", "scheduled");
    add_run(&db.conn, at(-2), "failed", "scheduled");
    let p = plan(&db, now());
    assert_eq!((p.next_at, p.retrying, p.due), (at(3), true, false));
    add_run(&db.conn, at(3), "failed", "scheduled");
    let p = plan(&db, at(4));
    assert_eq!((p.next_at, p.retrying), (at(8), true));
    // 처음부터 실패만 했어도 5분 뒤
    let fresh = testdb();
    add_stock(&fresh.conn, "005930");
    add_run(&fresh.conn, at(-1), "failed", "scheduled");
    let p = plan(&fresh, now());
    assert_eq!((p.next_at, p.retrying), (at(4), true));
}

#[test]
fn failure_before_success_or_later_regular_time_follows_the_regular_time() {
    // it("실패가 성공보다 앞이거나 정기 시각이 더 늦으면 정기 시각을 따른다")
    let db = testdb();
    add_stock(&db.conn, "005930");
    add_run(&db.conn, at(-40), "failed", "scheduled");
    add_run(&db.conn, at(-10), "success", "scheduled");
    let p = plan(&db, now());
    assert_eq!((p.next_at, p.retrying), (at(20), false));
    // 성공 직후 "지금 수집"이 실패해도 다음은 정기 시각
    add_run(&db.conn, at(-5), "failed", "manual");
    let p = plan(&db, now());
    assert_eq!((p.next_at, p.retrying), (at(20), false));
}

#[test]
fn does_not_start_while_a_run_is_in_progress() {
    // it("도는 회차가 있으면 시작하지 않는다")
    let db = testdb();
    add_stock(&db.conn, "005930");
    add_run(&db.conn, at(-120), "success", "scheduled");
    add_run(&db.conn, at(-1), "running", "scheduled");
    let p = plan(&db, now());
    assert_eq!((p.running, p.due), (true, false));
}

#[test]
fn follows_the_interval_setting_and_falls_back_to_thirty_minutes() {
    // it("간격 설정(15·30·60·180분)을 따르고, 그 밖의 값이면 30분")
    let db = testdb();
    add_stock(&db.conn, "005930");
    add_run(&db.conn, at(-10), "success", "scheduled");
    write_num(&db.conn, &COLLECT_INTERVAL_MINUTES, 60.0, now()).unwrap();
    let p = plan(&db, now());
    assert_eq!((p.interval_minutes, p.next_at), (60, at(50)));
    write_num(&db.conn, &COLLECT_INTERVAL_MINUTES, 45.0, now()).unwrap();
    let p = plan(&db, now());
    assert_eq!((p.interval_minutes, p.next_at), (30, at(20)));
}

// describe("keywordPlan (FR-3, 매일 06:00)")
// 이번 주기 시작: 한국 10-21 06:00 = 10-20T21:00Z
fn boundary() -> EpochMs {
    ms("2026-10-20T21:00:00Z")
}

fn set_keywords(conn: &Connection, id: i64, status: &str, generated_at: Option<EpochMs>) {
    conn.execute(
        "UPDATE stocks SET keywords_status = ?1, keywords_generated_at = ?2 WHERE id = ?3",
        params![status, generated_at, id],
    )
    .unwrap();
}

#[test]
fn regenerates_stocks_whose_keywords_were_made_before_this_cycle() {
    // it("이번 주기에 아직 안 했으면 06:00 전에 만든 키워드를 가진 종목을 다시 만든다")
    let db = testdb();
    let old = add_stock(&db.conn, "005930");
    let fresh = add_stock(&db.conn, "000660");
    let waiting = add_stock(&db.conn, "035720");
    let failed = add_stock(&db.conn, "000720");
    let deleted = add_stock(&db.conn, "179880");
    set_keywords(&db.conn, old, "ready", Some(ms("2026-10-20T10:00:00Z")));
    set_keywords(&db.conn, fresh, "ready", Some(ms("2026-10-21T01:00:00Z")));
    set_keywords(&db.conn, waiting, "pending", None);
    // 실패한 종목도 다음 주기에 다시(6단계 결정)
    set_keywords(&db.conn, failed, "failed", None);
    set_keywords(&db.conn, deleted, "ready", Some(ms("2026-10-19T00:00:00Z")));
    delete_stock(&db.conn, deleted, now()).unwrap();

    assert_eq!(
        keyword_plan(&db.conn, now()).unwrap(),
        KeywordPlan {
            every: RegenerateEvery::Daily,
            idle: false,
            next_at: now(),
            due: true,
            stock_ids: vec![old, failed],
        }
    );
}

#[test]
fn skips_generating_stocks_and_auto_topic_lens_items() {
    // 만드는 중은 빼고, 자동 토픽 항목(검색어는 토픽 갱신이 정함)도 뺀다. 사용자 주제 항목은 그대로 낀다.
    let db = testdb();
    let generating = add_stock(&db.conn, "005930");
    set_keywords(&db.conn, generating, "generating", None);
    let normal = add_stock(&db.conn, "000660");
    set_keywords(&db.conn, normal, "ready", Some(ms("2026-10-20T10:00:00Z")));
    let auto_topic = seed_topic(&db.conn, "auto", "자동", &["a"], now());
    let user_topic = seed_topic(&db.conn, "user", "사용자", &["b"], now());
    let mut lens_ids = Vec::new();
    for (topic_id, code) in [(auto_topic, "TOPIC-1"), (user_topic, "TOPIC-2")] {
        db.conn
            .execute(
                "INSERT INTO stocks (market, code, name, exchange, user_input, created_at, keywords_status, \
                 keywords_generated_at, kind, topic_id) VALUES ('KR', ?1, 'n', 'x', 'n', ?2, 'ready', ?3, 'topic', ?4)",
                params![code, now(), ms("2026-10-20T10:00:00Z"), topic_id],
            )
            .unwrap();
        lens_ids.push(db.conn.last_insert_rowid());
    }
    let plan = keyword_plan(&db.conn, now()).unwrap();
    assert_eq!(plan.stock_ids, vec![normal, lens_ids[1]]);
}

#[test]
fn next_day_six_when_already_done_this_cycle() {
    // it("이번 주기에 이미 했으면 다음 날 06:00")
    let db = testdb();
    add_stock(&db.conn, "005930");
    write_num(&db.conn, &AUTO_AT, (boundary() + MIN) as f64, now()).unwrap();
    let plan = keyword_plan(&db.conn, now()).unwrap();
    assert!(!plan.due);
    assert!(plan.stock_ids.is_empty());
    assert_eq!(plan.next_at, ms("2026-10-21T21:00:00Z"));
    // 다음 날 06:00이 되면 다시
    assert!(
        keyword_plan(&db.conn, ms("2026-10-21T21:00:00Z"))
            .unwrap()
            .due
    );
}

#[test]
fn weekly_still_checks_every_day_at_six_but_targets_stocks_made_before_monday() {
    // 일주일 1회여도 확인은 매일 06:00(다음 확인은 다음 날 06:00). 대상은 이번 주 월요일 06:00 전에 만든 것
    let db = testdb();
    let old = add_stock(&db.conn, "005930");
    let this_week = add_stock(&db.conn, "000660");
    let failed = add_stock(&db.conn, "035720");
    // 이번 주 월요일(10-19) 06:00 = 10-18T21:00Z
    set_keywords(&db.conn, old, "ready", Some(ms("2026-10-18T20:59:00Z")));
    set_keywords(
        &db.conn,
        this_week,
        "ready",
        Some(ms("2026-10-18T21:00:00Z")),
    );
    // 지난 점검에서 실패한 종목은 만든 시각이 옛날 그대로라 다시 고른다.
    set_keywords(&db.conn, failed, "failed", Some(ms("2026-10-10T00:00:00Z")));
    write_str(&db.conn, &EVERY, "weekly", now()).unwrap();
    // 오늘(10-21) 06:00 확인은 아직 안 했다.
    write_num(
        &db.conn,
        &AUTO_AT,
        ms("2026-10-20T21:30:00Z") as f64 - 1.0 * 24.0 * 3_600_000.0,
        now(),
    )
    .unwrap();

    let plan = keyword_plan(&db.conn, now()).unwrap();
    assert_eq!(plan.every, RegenerateEvery::Weekly);
    assert!(plan.due);
    assert_eq!(plan.stock_ids, vec![old, failed]);

    // 오늘 확인을 마치면 다음은 다음 날 06:00(월요일이 아니다).
    write_num(&db.conn, &AUTO_AT, (boundary() + MIN) as f64, now()).unwrap();
    let plan = keyword_plan(&db.conn, now()).unwrap();
    assert!(!plan.due);
    assert!(plan.stock_ids.is_empty());
    assert_eq!(plan.next_at, ms("2026-10-21T21:00:00Z"));
}

#[test]
fn failed_stock_is_picked_again_at_next_days_six_for_daily_and_weekly() {
    // 실패하면 만든 시각이 그대로라 다음 날 06:00 확인 때 다시 고른다. 성공한 종목은 이번 주기 안이라 고르지 않는다.
    for every in ["daily", "weekly"] {
        let db = testdb();
        let failed = add_stock(&db.conn, "005930");
        let done = add_stock(&db.conn, "000660");
        set_keywords(&db.conn, failed, "failed", Some(ms("2026-10-10T00:00:00Z")));
        // 오늘(10-21 수요일) 06:00 이후 점검에 성공했다.
        set_keywords(&db.conn, done, "ready", Some(ms("2026-10-20T22:00:00Z")));
        write_str(&db.conn, &EVERY, every, now()).unwrap();
        write_num(&db.conn, &AUTO_AT, (boundary() + MIN) as f64, now()).unwrap();
        assert!(keyword_plan(&db.conn, now()).unwrap().stock_ids.is_empty());

        // 다음 날 06:00(10-22T21:00Z 전날 = 10-21T21:00Z)이 되면 다시 확인한다.
        let tomorrow = ms("2026-10-21T21:00:00Z") + MIN;
        let plan = keyword_plan(&db.conn, tomorrow).unwrap();
        assert!(plan.due, "{every}");
        // 하루 1회: 성공한 종목도 이제 오늘 06:00 전에 만든 것이라 같이 고른다. 주 1회: 이번 주 안이라 실패한 것만.
        let expected = if every == "daily" {
            vec![failed, done]
        } else {
            vec![failed]
        };
        assert_eq!(plan.stock_ids, expected, "{every}");
    }
}

#[test]
fn waiting_and_generating_stocks_are_never_picked() {
    let db = testdb();
    let pending = add_stock(&db.conn, "005930");
    let generating = add_stock(&db.conn, "000660");
    set_keywords(
        &db.conn,
        pending,
        "pending",
        Some(ms("2026-10-01T00:00:00Z")),
    );
    set_keywords(&db.conn, generating, "generating", None);
    write_str(&db.conn, &EVERY, "weekly", now()).unwrap();
    assert!(keyword_plan(&db.conn, now()).unwrap().stock_ids.is_empty());
}

#[test]
fn no_stocks_means_idle_but_still_due() {
    // it("종목이 없으면 idle(주기 표시는 그대로)")
    let db = testdb();
    let plan = keyword_plan(&db.conn, now()).unwrap();
    assert!(plan.idle && plan.due);
    assert!(plan.stock_ids.is_empty());
}

#[test]
fn unknown_regenerate_setting_falls_back_to_daily() {
    let db = testdb();
    write_str(&db.conn, &EVERY, "monthly", now()).unwrap();
    assert_eq!(
        read_regenerate_every(&db.conn).unwrap(),
        RegenerateEvery::Daily
    );
}
