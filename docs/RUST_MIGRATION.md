# Rust 전환 설계 (14단계)

TS(Next.js + Node worker)로 된 Ripple을 기능 변경 없이 Rust로 옮기는 설계다. 결정은 DECISIONS.md 1장의 "14단계: Rust 전환"과 "14단계 설계 세부" 줄에 있다. 이 문서는 **구현자(Sonnet)가 혼자 읽고 헷갈리지 않고 진행할 수 있게** 쓴다. 문서와 코드가 다르면 멈추고 묻는다.

## 0. 읽는 법

| 언제 | 읽을 것 |
|---|---|
| 하위 단계 세션을 시작할 때 | 1장(규칙), 2장(크레이트), 3장(공통 방식), 그 하위 단계의 6장 항목 |
| web 하위 단계(R10~R12) | 위에 더해 5장 |
| worker API나 DB를 건드릴 때 | 4장 |
| 막혔을 때 | 7장(멈추고 물을 것), 8장(함정) |

- 하위 단계 하나에 세션 하나가 기본이다. 한 세션에 다 끝내지 못하면 `/handoff`로 어디까지 했는지 적고 다음 세션에서 이어 간다.
- TS 원본은 **명세 그 자체**다. 옮길 때마다 TS 파일과 그 테스트 파일을 처음부터 끝까지 읽는다. 이 문서는 "어떻게 옮기나"만 정하고, "무엇을 하나"는 TS 코드와 SPEC.md가 정한다.

## 1. 절대 규칙

1. **동작을 바꾸지 않는다.** 화면 글자, 로그 문구(한국어), 오류 메시지, HTTP 상태 코드, JSON 필드 이름, DB에 쓰는 값, 시각 계산, 정렬 순서, 상수(간격·한도·기준값)를 TS와 똑같이 한다. 더 좋아 보이는 방법이 있어도 바꾸지 않고 "전환 뒤 개선 후보"로 보고만 한다.
2. **기능 변경 금지(전환이 끝날 때까지).** 새 기능, 버그 수정, 설정 추가는 하지 않는다. TS에서 버그로 보이는 것을 찾으면 Rust에도 같은 동작으로 옮기고, 보고서의 "발견한 TS 버그" 목록에 적는다(DECISIONS.md 2장 "키워드·채점 점검" B1·B2처럼 이미 알려진 것 포함).
3. **테스트를 먼저 옮긴다.** TS 테스트 케이스 하나당 Rust 테스트 하나를 만들고, 같은 입력·같은 기대값을 쓴다. 테스트 이름은 TS `it("...")` 설명을 한국어 그대로 주석으로 달고 함수 이름은 영어 snake_case로 짓는다. 케이스를 빼거나 기대값을 바꾸지 않는다. Rust에서만 생기는 경우(예: UTF-16 자르기)는 테스트를 더한다.
4. **외부 계약(4장)은 고정이다.** DB 스키마, worker 내부 API, 환경 변수, compose 서비스 이름, 페이지 주소와 쿼리 파라미터, 쿠키 이름은 바꾸지 않는다.
5. **2장에 없는 크레이트는 쓰지 않는다.** 필요해 보이면 멈추고 묻는다(라이브러리는 사용자 결정, CLAUDE.md).
6. **AI 호출은 예산 검사를 거친다**(3.10). 테스트와 개발 중에 실제 OpenRouter·TypeSafe·Finnhub·KIS를 부르지 않는다. 실제 확인은 `/real-check`와 비용 승인 뒤에만 한다.
7. **본 Docker(`ripple`)는 건드리지 않는다.** 실제 DB 확인은 백업 사본으로 한다(`/real-check`).
8. **`.env`는 읽지 않는다.** 비밀 값은 환경 변수로만 쓰고 로그에 찍지 않는다. `Debug` 출력에 키가 들어가지 않게 설정 구조체의 키 필드는 직접 만든 `Debug`로 가린다.
9. `unsafe`는 쓰지 않는다. `unwrap()`·`expect()`는 테스트와 "실패하면 프로그램 버그"인 곳(정적 정규식 컴파일 등)에서만 쓴다.

## 2. 기술 스택과 크레이트

버전은 crates.io에서 2026-10-31에 조회했고, 나온 지 2주가 넘은 것만 골랐다. `rust/Cargo.toml`의 `[workspace.dependencies]`에 **정확한 버전(`=x.y.z`)으로 고정**하고, `Cargo.lock`을 커밋한다. 버전을 올리지 않는다.

| 용도 | 크레이트 | 버전 | features·메모 |
|---|---|---|---|
| Rust 툴체인 | rustc | 1.99.1 | `rust/rust-toolchain.toml`로 고정, edition 2024 |
| 비동기 런타임 | tokio | 1.54.1 | `rt`, `macros`, `time`, `sync`, `signal`, `net`, `fs`. **단일 스레드 런타임**(3.6) |
| HTTP 서버 | axum | 0.8.9 | 기본 features(`json`, `form`, `query` 포함) |
| 쿠키 | axum-extra | 0.12.6 | `cookie` |
| HTTP 미들웨어 | tower-http | 0.7.1 | `trace`, `timeout`(필요한 것만) |
| HTTP 클라이언트 | reqwest | 0.13.5 | 기본(rustls, `charset`, `http2`) + `json`, `stream`. 리다이렉트 따라감(기본) |
| SQLite | rusqlite | 0.40.2 | `bundled`, `backup`. 0.41은 마이그레이션 도구가 아직 지원하지 않아 쓰지 않는다 |
| 마이그레이션 | rusqlite_migration | 2.6.0 | 2026-10-31 결정(3.4) |
| 템플릿 | askama | 0.16.1 | 렌더링 결과 `String`을 `axum::response::Html`로 감싼다(askama_web은 쓰지 않음) |
| 직렬화 | serde / serde_json | 1.0.229 / 1.0.154 | `serde`는 `derive` |
| 시간 | chrono / chrono-tz | 0.4.45 / 0.10.4 | chrono는 `serde` 없이, chrono-tz 기본 |
| 정규식 | regex | 1.13.1 | 앞뒤 보기(lookaround)를 지원하지 않는다(8장) |
| 문자 인코딩 | encoding_rs | 0.8.44 | EUC-KR 피드 디코딩(`collect/http.ts`의 `decode`) |
| 주소 | url | 2.5.8 | |
| 해시·서명 | sha2 / hmac | 0.11.1 / 0.13.0 | 비밀번호 비교·세션 서명·지문 |
| base64 | base64 | (R0에서 조회·고정) | 세션 지문 base64url |
| 오류 | thiserror / anyhow | 2.0.21 / 1.0.104 | 모듈 오류는 thiserror, `main`과 작업 최상단만 anyhow |
| 로그 | (없음) | | `log.ts`처럼 `println!`/`eprintln!`으로 직접 찍는다(3.8). tracing은 쓰지 않는다 |
| 명령줄 | clap | 4.6.7 | `derive`. 하위 명령 `migrate`·`worker`·`web`·`healthcheck` |
| 테스트: 임시 파일 | tempfile | 3.27.0 | dev-dependency |
| 테스트: 가짜 HTTP 서버 | wiremock | 0.6.5 | dev-dependency. 가짜 객체로 안 되는 HTTP 테스트에만(3.13) |
| 화면 상호작용 | htmx | 2.0.11 | `rust/ripple/static/htmx.min.js`로 저장소에 둔다. CDN 쓰지 않음 |
| CSS | Tailwind standalone CLI | 4.3.3 | 지금 `node_modules/tailwindcss`와 같은 버전. Node 없이 실행 파일 하나 |

- `base64`처럼 버전이 비어 있는 것은 R0에서 crates.io로 조회해 2주 넘은 최신 안정판을 고르고 이 표에 적는다.
- 스택 요약: **tokio(실행 엔진) + axum(웹 서버) + reqwest(외부 호출) + rusqlite(DB) + askama(HTML 템플릿) + htmx(작은 화면 동작) + Tailwind(CSS)**.

## 3. 공통 방식

### 3.1 폴더 구조

```
rust/
  Cargo.toml              # workspace. members = ["ripple"], [workspace.dependencies]에 버전 고정
  Cargo.lock              # 커밋한다
  rust-toolchain.toml     # channel = "1.99.1", components = ["rustfmt", "clippy"]
  ripple/
    Cargo.toml
    migrations/           # 마이그레이션 SQL(3.4)
    styles/input.css      # Tailwind 입력. src/app/globals.css를 옮긴 것
    static/               # 실행 파일에 넣는 정적 파일(3.12)
      htmx.min.js         # 커밋
      app.js              # 직접 쓴 작은 JS(5.4), 커밋
      favicon.ico         # src/app/favicon.ico 복사, 커밋
      app.css             # Tailwind 빌드 결과. 커밋하지 않음(.gitignore)
    templates/            # askama 템플릿(5장)
    src/
      main.rs             # clap 하위 명령만. 나머지는 lib
      lib.rs
      server/             # src/server/* 대응(web·worker 공용)
      worker/             # src/worker/* 대응
      web/                # src/app/*, src/proxy.ts, src/components/* 대응
      test_support.rs     # #[cfg(test)] 공용 헬퍼(tests/db.ts, tests/fixtures.ts 대응)
```

- crate는 하나(`ripple`, lib + bin)다. web과 worker는 같은 실행 파일의 다른 하위 명령이다. 서로 다른 프로세스·컨테이너로 돌고(결정 ⑮), web 프로세스는 DB를 읽기 전용으로만 연다.
- `tests/fixtures/` 아래 샘플 파일(`kind-sample.xls` 등)은 옮기지 않고 저장소 루트 경로 그대로 읽는다(`env!("CARGO_MANIFEST_DIR")` 기준 `../../tests/fixtures/...`). R13에서 TS를 지울 때 `rust/ripple/tests/fixtures/`로 옮긴다.

### 3.2 TS 파일 → Rust 모듈 대응

규칙: `src/<경로>/<이름>.ts` → `rust/ripple/src/<경로>/<이름>.rs`. 이름의 `-`는 `_`로 바꾼다.

| TS | Rust |
|---|---|
| `src/server/collect/feed-headlines.ts` | `src/server/collect/feed_headlines.rs` |
| `src/server/collect/feed-headlines.test.ts` | `src/server/collect/feed_headlines_tests.rs` |
| `src/server/ai/decider/typesafe.ts` | `src/server/ai/decider/typesafe.rs` |
| `src/worker/server.ts` | `src/worker/server.rs` |
| `src/app/(app)/...` | `src/web/...` (5.2의 표를 따른다) |

- 테스트: `a/b.test.ts` → `a/b_tests.rs`. 본 파일 끝에 다음처럼 붙인다. TS와 파일이 1:1로 맞아 빠진 테스트를 찾기 쉽다.
  ```rust
  #[cfg(test)]
  #[path = "b_tests.rs"]
  mod tests;
  ```
- 디렉터리는 `mod.rs` 대신 `a.rs` + `a/` 방식(edition 2024 기본)으로 쓴다.
- 함수·타입 이름은 TS 이름을 snake_case·PascalCase로 바꾼 것을 그대로 쓴다(`createTypeSafeDecider` → `create_type_safe_decider`, `BudgetExceededError` → `BudgetExceededError`). 찾기 쉽게 하려는 것이다.
- TS의 "팩토리 함수가 객체를 돌려줌"(`createX(deps) → { method() }`)은 `struct X { deps }` + `impl X { fn method() }`로 옮긴다. 테스트에서 바꿔 끼우는 의존성(`LlmClient`, `Decider`, `HttpClient`, `FinnhubClient`, `KisClient`)은 trait로 둔다(3.13).

### 3.3 DB 접근

- 연결: `rusqlite::Connection` 하나를 `Db = Arc<std::sync::Mutex<Connection>>`로 감싼다. worker는 읽기·쓰기 연결 하나, web은 읽기 전용 연결 하나(`getWebDb`와 같음).
- 연결을 열 때마다 아래를 **반드시** 한다(`src/server/db/client.rs`의 `open_db`).
  - 읽기 전용이면 `OpenFlags::SQLITE_OPEN_READ_ONLY`, 아니면 `READ_WRITE`. 둘 다 `CREATE` 없음(파일이 없으면 실패, TS `fileMustExist: true`). 파일을 만드는 곳은 `migrate` 하나뿐이다.
  - `busy_timeout` 5000ms.
  - `PRAGMA foreign_keys = ON`. **better-sqlite3는 기본으로 켜져 있고(2026-10-31 확인: `foreign_keys` 1, 컴파일 옵션 `DEFAULT_FOREIGN_KEYS`), rusqlite bundled는 기본으로 꺼져 있다.** 빼먹으면 지울 때·넣을 때 동작이 달라진다.
  - worker만 `PRAGMA journal_mode = WAL`을 하고 결과가 `wal`이 아니면 실패한다(`enableWal`).
- 쿼리 함수는 `fn xxx(conn: &Connection, ...) -> rusqlite::Result<T>`(쓰기 묶음은 `&mut Connection` 또는 `&Transaction`)로 만들고, 잠금은 부르는 쪽이 짧게 잡는다: `let rows = { let conn = db.lock().unwrap(); read_rows(&conn, ...)? };`. **`MutexGuard`를 쥔 채 `.await`하지 않는다.**
- drizzle 쿼리는 SQL로 직접 옮긴다. 옮기기 전에 drizzle이 만드는 SQL이 헷갈리면 TS 테스트에서 `.toSQL()`로 찍어 확인한다. 열 이름은 `drizzle/*.sql`과 `src/server/db/schema.ts`의 snake_case 이름을 쓴다.
- 트랜잭션: TS에서 `db.transaction(...)`이면 Rust도 `conn.transaction()`. TS에서 트랜잭션이 아니면 Rust도 아니다(동작을 바꾸지 않는다).
- 값 대응(지금 DB에 들어 있는 형식 그대로 읽고 쓴다).

| schema.ts | DB에 든 값 | Rust 타입 |
|---|---|---|
| `integer(..., { mode: "timestamp_ms" })` | UTC 밀리초 정수 | `i64`(3.5) |
| `integer(..., { mode: "boolean" })` | 0/1 | `bool` |
| `text(..., { enum: [...] })` | 문자열 | Rust enum + `as_str()`/`FromStr`, DB에는 문자열 그대로 |
| `text(..., { mode: "json" })` | JSON 문자열 | `serde` 구조체. **필드 이름은 지금 저장된 JSON 키(camelCase)와 같아야 한다**: `#[serde(rename_all = "camelCase")]`. 옛 줄에 없는 필드는 `Option` + `#[serde(default)]` |
| `real` | 실수 | `f64` |
| nullable 열 | NULL | `Option<T>` |

- JSON 열은 옛 형식(예: 11단계 전 `evaluation`에 영향도 없음)이 섞여 있다. 저장된 JSON 키가 헷갈리면 `/real-check` 백업 사본에서 `SELECT DISTINCT json_each.key ...`로 실제 키를 확인한다.

### 3.4 마이그레이션 (결정 5B)

**도구: rusqlite_migration 2.6.0(2026-10-31 결정).** 이유: rusqlite 전용이고, 이력을 `PRAGMA user_version` 숫자 하나로 남겨 기준선 잡기가 "숫자를 11로 쓰기" 한 줄이다. refinery 0.10은 이력 표에 체크섬을 넣어야 해서 기준선 잡기가 더 복잡하다. R1 시작 때 rusqlite_migration 공식 문서(docs.rs)로 `user_version`을 쓰는지, 트랜잭션으로 적용하는지 확인하고 다르면 멈춘다.

- SQL 파일: `drizzle/0000_init.sql`~`0010_stock_prices.sql` 11개를 `rust/ripple/migrations/`에 **내용 그대로** 복사한다(`--> statement-breakpoint`는 SQL 주석이라 `execute_batch`에 그대로 넣어도 된다). `include_str!`로 실행 파일에 넣는다. 순서 = 파일 이름 순서.
- `ripple migrate` 순서(`src/server/db/migrate.rs`):
  1. 폴더를 만들고 DB를 연다(없으면 만든다. 유일하게 `CREATE` 플래그를 쓰는 곳). `busy_timeout` 5000ms, `foreign_keys = ON`.
  2. **기준선:** `user_version`이 0이고 `__drizzle_migrations` 표가 있으면 drizzle이 적용한 DB다. 그 표의 줄 수가 11이고 `max(created_at)`이 `drizzle/meta/_journal.json`의 0010 `when` 값(1792...)과 같으면 `PRAGMA user_version = 11`을 쓴다. 줄 수나 값이 다르면 **아무것도 바꾸지 않고 실패**한다(오류 메시지에 줄 수와 기대값을 넣는다).
  3. rusqlite_migration으로 아직 적용하지 않은 마이그레이션을 적용한다(새 DB면 0000부터 11개).
  4. `마이그레이션 적용 완료: <경로>` 로그(TS와 같은 문구).
- `__drizzle_migrations` 표는 지우지 않는다. 문제가 생겨 TS 이미지로 되돌려도 drizzle이 같은 DB를 그대로 쓴다(`user_version`은 drizzle이 보지 않는다).
- 전환 중에는 새 마이그레이션을 만들지 않는다(기능 변경 금지).
- 테스트(R1 완료 기준):
  - 새 파일 DB에 Rust로 적용한 결과와 TS(`pnpm db:migrate`)로 적용한 결과의 `SELECT type, name, tbl_name, sql FROM sqlite_master ORDER BY name`이 같다(`__drizzle_migrations` 줄 제외).
  - drizzle로 적용한 DB에 `ripple migrate`를 두 번 돌려도 스키마가 그대로이고 `user_version`이 11이다.
  - `__drizzle_migrations` 줄 수가 10인 DB에서는 실패하고 아무것도 바뀌지 않는다.

### 3.5 시각

- 지금 DB와 코드처럼 **시각은 UTC 밀리초 `i64`로 다룬다.** `type EpochMs = i64;`를 `src/server/time.rs`에 둔다. 덧셈·뺄셈은 TS의 `getTime()` 계산을 그대로 옮긴다(`now - DAY_MS`).
- chrono·chrono-tz는 **시간대 계산과 글자 표시에만** 쓴다. 한국은 고정 UTC+9라 TS `time.ts`처럼 9시간을 더해 계산해도 된다. 뉴욕(`prices/plan.ts`)은 일광절약시간이 있으므로 `chrono_tz::America::New_York`을 쓴다.
- "지금"은 함수 인자로 받는다. TS가 `now?: () => Date`나 `now: Date`를 받는 곳은 Rust도 `now: EpochMs`(또는 `&dyn Fn() -> EpochMs`)를 받는다. 테스트는 고정 시각을 넣는다. `SystemTime::now()`는 최상단(worker·web 시작, 요청 처리 시작)에서만 부른다.
- `formatSeoulDateTime`의 결과("YYYY-MM-DD HH:mm:ss", 24시간제)를 기준 함수로 두고, 나머지 표시 함수는 TS처럼 그 글자를 잘라 쓴다.

### 3.6 비동기·동시성

- **tokio 단일 스레드 런타임**(`#[tokio::main(flavor = "current_thread")]`)을 쓴다. Node처럼 한 번에 한 작업만 돌고 `.await`에서만 다른 작업으로 넘어간다. 그래서 TS의 `let running = false` 같은 "실행 중이면 건너뜀" 판단을 `Rc`가 아닌 `Arc<AtomicBool>`·`Arc<Mutex<..>>`로 옮기기만 하면 되고, 경쟁 조건을 새로 고민하지 않아도 된다(axum이 `Send`를 요구하므로 `Rc`·`RefCell`은 쓰지 않는다).
- DB 호출은 지금(better-sqlite3)처럼 동기로 부른다. `spawn_blocking`은 쓰지 않는다(쿼리가 수 ms라 Node와 같은 방식이다). 예외: 백업(`backup.ts`)은 TS도 비동기라 `spawn_blocking` 안에서 별도 연결로 한다.
- `void promise`(기다리지 않고 시작) → `tokio::spawn`. `.finally(() => flag = false)`는 spawn한 작업 안에서 본문을 `async {}` 블록으로 감싸 `.await`한 뒤(성공·실패 상관없이) 플래그를 내린다.
- `setInterval(tick, 60_000)` → `tokio::time::interval`(`MissedTickBehavior::Delay`). 처음 한 번은 바로 부른다(TS도 `tick()`을 먼저 부름).
- 종료: SIGTERM·SIGINT를 `tokio::signal`로 받아 스케줄러를 멈추고, 서버를 끄고, 로그 `SIGTERM 수신, 종료합니다.`를 찍고 종료 코드 0으로 끝낸다(`worker/index.ts`).

### 3.7 오류

- TS의 오류 클래스는 Rust의 오류 enum으로 옮긴다. `kind` 문자열이 있는 것은 enum 변형으로 둔다.

| TS | Rust |
|---|---|
| `LlmError(message, kind, usage?)` | `struct LlmError { message: String, kind: LlmErrorKind, usage: Option<LlmUsage> }` |
| `DeciderError(message, kind)` | `struct DeciderError { message, kind: DeciderErrorKind }` |
| `BudgetExceededError(spentUsd, limitUsd)` | `struct BudgetExceededError { spent_usd: f64, limit_usd: f64 }` |
| `HttpError(message, kind)` | `struct HttpError { message, kind: HttpErrorKind }` |
| `ListingsUnavailableError`, `ModelListError` 등 | 같은 이름의 struct |

- 한 함수가 여러 종류를 던질 수 있으면(`instanceof`로 갈라 처리하는 곳) 그 함수 전용 enum을 만들어 `#[from]`으로 묶는다. 예: `matchStocks`는 `enum MatchError { Budget(BudgetExceededError), Listings(ListingsUnavailableError), Llm(LlmError), Db(rusqlite::Error) }`. 받는 쪽은 `match`로 TS의 `instanceof` 분기와 같은 순서로 처리한다.
- `Display`(`#[error("...")]`) 문구는 TS `message`와 같은 한국어 문장이다(화면·로그에 그대로 나간다).

### 3.8 로그

- `log(message)` → stdout에 `[YYYY-MM-DD HH:mm:ss] message`(한국 시간). `logError(message, error)` → stderr에 `[시각] message <error>`. TS는 오류 객체를 그대로 찍는데, Rust는 `{error}`(Display)로 찍는다. 이 차이는 허용한다.
- 로그 문구는 TS와 똑같이 한다. 문서와 운영 절차가 로그 줄을 찾는다(예: `토픽 갱신: … 이어짐 N`, `마이그레이션 적용 완료`, `내부 API 대기 중`).
- 키·토큰·비밀번호를 로그에 넣지 않는다.

### 3.9 설정(settings 표)

- `src/server/settings.rs`에 `SETTING_DEFAULTS`의 키마다 타입이 있는 읽기·쓰기 함수를 둔다. 예: `read_setting_f64(conn, "budget.monthlyUsd")`보다는 `enum SettingKey` + `fn read<T: SettingValue>(conn, key) -> T`처럼, **키 문자열은 한 곳에만** 쓴다.
- 읽기 규칙은 TS와 같다: 줄이 없거나, JSON이 깨졌거나, 타입이 다르면 기본값. 배열은 모든 원소가 문자열일 때만.
- **숫자 타입 주의**: TS `typeof value === "number"`는 정수·실수를 가리지 않는다. Rust에서 숫자 설정은 모두 `f64`로 읽고(`serde_json::Value::as_f64`), 정수로 쓰는 곳(간격·깊이·건수)은 읽은 뒤 바꾼다. 쓸 때는 JS `JSON.stringify`와 같은 글자를 만든다: 정수 값(`5.0`)은 `5`로 쓴다(8장 "숫자를 글자로").
- 쓰기: `INSERT ... ON CONFLICT(key) DO UPDATE SET value = ?, updated_at = ?`. 처음 넣을 때 `updated_at`도 넣는다(drizzle `$defaultFn`).

### 3.10 AI 호출 (CLAUDE.md: 사용량 기록과 비용 한도 검사를 거친 뒤에만)

- `budget/usage.rs`에 `generate_with_budget`, `rate_with_budget`를 TS와 같은 순서로 둔다: ① `assert_within_budget` ② 호출 ③ `record_usage`. 응답을 받은 뒤 형식이 틀려 실패해도(`LlmError.usage`가 있으면) 사용량을 기록한다.
- 기사 평가(`evaluate/run.ts`)는 Jev `decide`를 직접 부르면서 예산 검사·기록을 그 파일 안에서 한다. TS 흐름을 그대로 옮긴다.
- `LlmClient`·`Decider` trait의 메서드는 예산 함수를 거쳐서만 부른다. 다른 모듈에서 `llm.generate_object(...)`를 바로 부르는 코드를 새로 만들지 않는다(리뷰 때 `grep -n "generate_object\|\.decide(\|\.rate(" -r src`로 확인).

### 3.11 외부 HTTP

- **OpenRouter** (`ai/llm/openrouter.ts`): 라이브러리 없이 `POST {base}/chat/completions`. 요청 본문, 오류 분류(402·429·그 밖), 사용량 읽기(`usage.prompt_tokens`·`completion_tokens`·`cost`), 코드 블록 벗기기를 그대로 옮긴다. 제한 시간 기본 30초, 요청별로 바꿀 수 있음.
  - **구조화 출력 스키마**: TS는 zod 스키마를 `z.toJSONSchema`로 바꿔 보낸다. Rust는 요청마다 `serde_json::json!`으로 **같은 JSON 스키마를 직접 쓴다.** 헷갈림을 막는 방법: R2에서 TS 쪽에 스크립트 하나(`rust/tools/dump-schemas.ts`, `pnpm exec tsx`로 실행)를 만들어 지금 쓰는 모든 스키마(`schemaName`별)를 `rust/ripple/src/server/ai/llm/schemas/<schemaName>.json`으로 내보내고 커밋한다. Rust 테스트는 Rust가 만든 스키마가 이 파일과 같은지(`serde_json::Value` 비교) 확인한다. 응답 검증(zod `safeParse`)은 `serde` 역직렬화 + TS 스키마의 길이·범위 조건을 Rust 함수로 옮긴 검사로 한다.
  - 모델 목록(`ai/llm/models.ts`): `GET {base}/models`.
- **TypeSafe Jev** (`ai/decider/typesafe.ts`): SDK 대신 `POST {base}/v1/systemone`을 직접 부른다(`Authorization: Bearer <키>`, JSON, 압축 없음). 요청 본문은 `{ model, state, questions }`. **SDK가 하던 것을 그대로 옮긴다**: 시도마다 제한 시간 10초, 429·5xx·연결 오류면 두 번 더 시도, `retry-after-ms`/`Retry-After` 헤더를 따르고 없으면 지수 백오프. 정확한 기본값(초기 지연·최대 지연·지터)은 `node_modules/@typesafe-ai/sdk/dist/index.mjs`의 `DEFAULT_RETRY_POLICY`(52줄 근처)와 `retryDelayMs`(93줄 근처), `systemOne`(946줄 근처)을 읽고 같은 값으로 한다. 오류 분류(`toDeciderError`)도 같은 상태 코드로 나눈다. 가짜 서버는 `e2e/mock-services.mjs` 282줄(`/typesafe/v1/systemone`)이 이미 있다.
- **수집용 HTTP** (`collect/http.ts`): 호스트마다 요청 시작 간격 1.1초(자리를 먼저 잡는 방식 그대로), 제한 시간 20초, 최대 5,000,000바이트(넘으면 받다가 끊음), `User-Agent`·`Accept` 헤더 문자열 그대로, 리다이렉트 따라감. 디코딩은 `Content-Type`의 charset → XML 선언의 encoding → UTF-8 순서, `cp949`·`ms949`·`x-windows-949`는 `euc-kr`로. `encoding_rs::Encoding::for_label`로 찾고 못 찾으면 UTF-8.
- **Finnhub·KIS·KIND·NASDAQ Trader**: TS 파일의 주소·파라미터·헤더·간격을 그대로.
- 모든 외부 주소는 지금처럼 환경 변수로 바꿀 수 있어야 한다(`*_BASE_URL`, `LISTING_*_URL`, `RSS_FEEDS_JSON`). E2E가 이것으로 가짜 서비스를 쓴다.

### 3.12 정적 파일과 빌드

- `static/` 파일은 `include_bytes!`로 실행 파일에 넣고 web이 `/static/<이름>`으로 내보낸다(올바른 `Content-Type`, `Cache-Control: public, max-age=31536000, immutable`, 주소에 내용 해시를 붙여 캐시를 깬다: `/static/app.<해시8자>.css`). 해시는 빌드 때 `build.rs` 없이 실행 시작 때 계산해 템플릿에 넘긴다.
- `favicon.ico`는 `/favicon.ico`로도 내보낸다(로그인 없이, E2E가 기동 확인에 씀).
- CSS 빌드: `tailwindcss -i styles/input.css -o static/app.css --minify`. Tailwind가 클래스를 찾을 곳은 `templates/**/*.html`과 `static/app.js`(`@source`로 지정). `cargo build` 전에 돌려야 하므로 `rust/build.sh`(CSS → cargo build 순서)를 만들고 Docker와 `/check`가 이것을 쓴다.
- Tailwind 실행 파일은 `rust/tools/tailwindcss`에 내려받는다(.gitignore). 내려받는 스크립트 `rust/tools/fetch-tailwind.sh`가 버전(4.3.3)과 SHA-256을 확인한다(값은 R0에서 GitHub 릴리스의 체크섬으로 채운다).

### 3.13 테스트

- 명령: `cd rust && cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test`.
- **테스트 DB**: `test_support::test_db()`가 임시 폴더에 새 DB를 만들고 `migrate`를 적용해 돌려준다(`tempfile::TempDir`를 함께 돌려줘 끝나면 지워지게). TS `tests/global-setup.ts`처럼 한 번 만든 템플릿을 복사하는 방식으로 빠르게 해도 된다(`std::sync::OnceLock`).
- **가짜 의존성**: TS 테스트가 가짜 객체(`fetchImpl`, 가짜 `HttpClient`, 가짜 `LlmClient`·`Decider`)를 넣으면 Rust는 같은 경계의 trait에 가짜 구현을 넣는다. 테스트 파일 안에 `struct FakeLlm { responses: Mutex<VecDeque<...>>, calls: Mutex<Vec<...>> }` 식으로 만든다.
- **실제 HTTP가 필요한 테스트**(클라이언트의 요청 모양·헤더·재시도·제한 시간을 확인하는 테스트, 예: `openrouter.test.ts`, `typesafe.test.ts`, `http.test.ts`, `kis.test.ts`)는 wiremock으로 가짜 서버를 띄운다.
- 테스트에서 시간을 기다려야 하면(재시도 지연·간격) TS가 `sleep`을 주입하듯 Rust도 `sleep` 함수를 주입하거나 `tokio::time::pause()`를 쓴다. 실제로 몇 초씩 기다리는 테스트를 만들지 않는다.
- **E2E**: `pnpm test:e2e`(Playwright, TS 그대로). R9부터 `playwright.config.ts`의 worker 실행 명령을, R10부터 web 실행 명령을 Rust 실행 파일로 바꾼다(6장). 시험 파일(`e2e/*.spec.ts`)은 **화면 동작의 계약**이다. 고쳐도 되는 것은 Next.js에만 있던 기다림(예: 서버 액션 POST 응답 기다리기 `readSaved`)을 같은 뜻의 Rust 쪽 요청 기다리기로 바꾸는 것뿐이다. 확인하는 내용(글자·개수·주소·상태)은 바꾸지 않는다. 고친 줄마다 보고서에 적는다.

## 4. 외부 계약 (바꾸지 않는다)

### 4.1 DB
- 스키마는 `drizzle/0000~0010`이 만든 그대로다. 표·열·인덱스를 더하거나 바꾸지 않는다.
- 전환 중(R9 배포 뒤 ~ R12) Rust worker가 쓰고 Next.js web이 읽는다. Rust가 쓰는 값의 형식(시각 ms, JSON 키, enum 글자)이 TS와 다르면 화면이 깨진다.

### 4.2 worker 내부 API (`src/worker/server.ts`)
- `/health` 말고는 모두 `Authorization: Bearer <WORKER_API_TOKEN>`이 필요하고, 틀리면 401 `{"error":"unauthorized"}`. 비교는 시간 차가 없게(`auth/worker-token.ts`).
- 응답은 모두 `content-type: application/json; charset=utf-8`. 본문 16KB를 넘으면 413 `{"error":"body_too_large"}`, JSON이 깨지거나 검증에 실패하면 400 `{"error":"invalid_request"}`(설정은 400 `invalid_settings` + `fields`). 처리 중 예상 못 한 오류는 500 `{"error":"internal"}` + 로그.

| 메서드·주소 | 본문 | 성공 | 그 밖 |
|---|---|---|---|
| GET `/health` | | 200 `{status:"ok", db:"ok", listings:{KR,US}}` | 503 `{status:"error", db:"error"}` |
| POST `/stocks/match` | `{query}`(앞뒤 공백 제거, 1~`MAX_QUERY_LENGTH`자) | 200 `{candidates}` | 402 `budget_exceeded`(+`spentUsd`,`limitUsd`), 503 `listings_unavailable`, `llm_<kind>`(503·402·429·504·502·502) |
| POST `/stocks` | `{market, code, userInput, nameKo?}` | 201 생성 / 200 복원, 본문 = 결과 그대로 | 404 `not_listed`, 409 `already_registered`(+`stock`) |
| POST `/stocks/{id}/keywords` | | 202 `{status:"queued"}` | 404 `not_found` |
| DELETE `/stocks/{id}` | | 200 `{ok:true}` | 404 `not_found` |
| POST `/collect` | | 202 `{runId}` | 409 `already_running`(+`runId`) |
| PUT `/settings` | 설정 폼 | 200 `{ok:true, changed}` | 400 `invalid_settings`(+`fields`), 422 `unknown_model`, 502 `model_check_failed` |
| POST `/keywords/rebuild` | | 202 `{count}` | |
| PUT `/markets/{KR\|US}` | `{enabled}` | 200 `{ok:true}` | |
| POST `/topics` | `{name}`(최대 100자) | 201 `{topicId}` | 409 `exists`, 400 그 밖의 오류 |
| DELETE `/topics/{id}` | | 200 `{ok:true}` | 404 `not_found` |
| PUT `/articles/read` | `{articleIds(1~MAX_READ_BATCH, 양의 정수), read}` | 200 `{changed}` | |
| POST `/topics/refresh` | | 202 `{started}` | 503 `unavailable` |
| 그 밖 | | | 404 `not_found` |

- 상태 코드·필드 이름·로그 문구는 `server.ts`를 다시 읽고 맞춘다. 표와 `server.ts`가 다르면 `server.ts`가 맞다.
- 검증 규칙(zod)의 세부(정수·양수·trim 순서·`nullish`)는 Rust 검증 함수로 그대로 옮긴다. `serde`만으로 끝내지 않는다(예: `z.string().trim().min(1)`은 "앞뒤 공백을 지운 뒤 1자 이상"이고, 지운 값을 쓴다).

### 4.3 환경 변수
SPEC 8장 표 전부와 E2E가 쓰는 `OPENROUTER_BASE_URL`, `TYPESAFE_BASE_URL`, `FINNHUB_BASE_URL`, `KIS_BASE_URL`(있으면), `LISTING_KIND_URL`, `LISTING_NASDAQ_URL`, `LISTING_OTHER_URL`, `RSS_FEEDS_JSON`, `SCHEDULER_DISABLED`. 이름·기본값·검사 규칙(`APP_PASSWORD` 7자, `SESSION_SECRET`·`WORKER_API_TOKEN` 32자 이상이면 시작, 아니면 `[설정 오류] ... .env를 확인하세요.`를 찍고 종료 코드 1)은 `src/worker/config.ts`, `src/server/env.ts`, `src/server/auth/config.ts`, `src/server/auth/worker-token.ts`, `src/instrumentation-node.ts`를 따른다.

### 4.4 compose·Docker
- 서비스 이름 `migrate`·`worker`·`web`·`tunnel`, 볼륨 `ripple-data`, 포트(`WEB_PORT`, 내부 4000·3000), 백업 바인드, healthcheck 주기는 그대로. 이미지는 하나(결정 ⑬)로, 명령만 `["ripple","migrate"]`·`["ripple","worker"]`·`["ripple","web"]`.
- healthcheck: `["CMD","ripple","healthcheck","http://127.0.0.1:4000/health"]`(Node가 없어지므로). `healthcheck`는 주소를 GET해서 2xx면 0, 아니면 1로 끝난다.
- web은 `PORT`(기본 3000)·`HOSTNAME`(기본 0.0.0.0)으로 듣는다(Next standalone과 같은 이름).

### 4.5 화면 주소·쿠키
- 페이지: `/`(타임라인, 쿼리 파라미터는 `src/app/(app)/page.tsx`·`timeline-filters.tsx`가 쓰는 이름 그대로), `/status`, `/stocks`, `/settings`, `/login`(`?next=`).
- 미로그인: 페이지는 `/login`(원래 주소가 `/`가 아니면 `?next=<경로+쿼리>`)으로 307, `/api/`로 시작하면 401 `{"error":"unauthorized"}`. 예외는 `/login`, `/favicon.ico`, `/static/*`.
- 리다이렉트 `Location`은 **상대 주소**로 쓴다(Next가 그렇게 바꿔 줘서 터널 주소가 유지됐다, `src/proxy.ts` 주석).
- 쿠키 이름 `ripple_session`, 30일, `HttpOnly`, `SameSite=Lax`, `Path=/`, `Secure`는 TS(`src/app/login/actions.ts`)와 같은 조건으로. 형식은 바뀌어도 된다(결정 ⑫: 한 번 다시 로그인).

## 5. web 설계 (R10~R12)

### 5.1 요청 처리
- axum 라우터 하나. 순서: 정적 파일·`/favicon.ico`·`/login` → 로그인 확인 미들웨어(4.5) → 페이지·동작 핸들러.
- 세션 토큰: `base64url(payload JSON) + "." + base64url(HMAC-SHA256(SESSION_SECRET, payload))`. payload `{ "pwd": <비밀번호 지문>, "iat": 초, "exp": 초 }`. 지문은 TS와 같이 `HMAC-SHA256(SESSION_SECRET, "app-password:" + APP_PASSWORD)`의 base64url. 검증은 서명(시간 차 없는 비교) → `exp` → `pwd` 순서. JWT 라이브러리를 쓰지 않는다(크레이트를 늘리지 않으려고).
- 비밀번호 비교: 두 값의 SHA-256을 시간 차 없이 비교(`isPasswordCorrect`).
- 로그인 실패 제한: `auth/rate-limit.ts`를 그대로 옮겨 web 프로세스 메모리(`Arc<Mutex<..>>`)에 둔다. 키는 `auth/request.ts`가 고르는 IP(헤더 순서 그대로).
- **CSRF**: Next 서버 액션은 Origin 검사를 기본으로 했다. Rust는 모든 POST·PUT·DELETE에서 `Origin` 헤더가 있으면 그 호스트가 `Host` 헤더와 같아야 하고, 다르면 403. 쿠키 `SameSite=Lax`와 함께 쓴다.
- DB: 읽기 전용 연결 하나. 쓰기는 모두 worker 내부 API(`server/worker_client.rs`, `worker-client.ts`의 함수·결과 형태 그대로)로.

### 5.2 화면 파일 대응

| TS | Rust 핸들러 | 템플릿 |
|---|---|---|
| `app/layout.tsx`, `app/(app)/layout.tsx`, `budget-banner.tsx` | `web/layout.rs`(공통 값: 예산 배너, 메뉴) | `templates/base.html`, `templates/app_layout.html` |
| `app/login/*` | `web/login.rs` | `templates/login.html` |
| `app/(app)/page.tsx` + `timeline-card`·`timeline-filters`·`topic-cards`·`price-strip`·`read-card`·`impact` | `web/timeline.rs` | `templates/timeline/*.html` |
| `app/(app)/status/*` | `web/status.rs` | `templates/status/*.html` |
| `app/(app)/stocks/*` | `web/stocks.rs` | `templates/stocks/*.html` |
| `app/(app)/settings/*` | `web/settings.rs` | `templates/settings/*.html` |
| `components/ui/*`(shadcn) | 없음 | `templates/ui/*.html`(askama 매크로: button, badge, card, input, label, alert_dialog) |
| `app/(app)/actions.ts` 등 서버 액션 | 각 화면 핸들러의 POST 경로(5.3) | 부분 템플릿 |

- shadcn 컴포넌트는 **지금 렌더링된 HTML의 클래스 문자열을 그대로** askama 매크로로 옮긴다. 옮기기 전에 지금 앱(`pnpm dev` 또는 E2E 서버)을 띄워 브라우저에서 HTML을 복사해 기준으로 삼는다. 색·간격이 달라지면 화면 동작이 같아도 실패다.
- lucide 아이콘은 쓰는 것만 SVG를 `templates/ui/icons.html` 매크로로 옮긴다(lucide는 ISC 라이선스, 파일 맨 위에 출처 주석).
- 화면 확인: 하위 단계마다 데스크톱·휴대폰(Pixel 7) 크기로 TS 화면과 Rust 화면을 같은 데이터로 캡처해 나란히 보고 다른 곳을 고친다(CLAUDE.md: 화면은 캡처해서 직접 본다).

### 5.3 서버 액션 → POST 경로

| 서버 액션 | Rust 경로 | 방식 |
|---|---|---|
| `login` | POST `/login` | 일반 폼. 성공 시 `next`(같은 사이트 경로만) 또는 `/`로 303 |
| `logout` | POST `/logout` | 일반 폼 → `/login` 303 |
| `collectNowAction` | POST `/status/collect` | htmx, 버튼 부분만 바꿈 |
| `refreshTopicsAction` | POST `/status/topics/refresh` | htmx |
| `setArticlesReadAction` | POST `/articles/read` | `static/app.js`의 `fetch`(5.4) |
| `matchStocksAction` | POST `/stocks/match` | htmx, 후보 목록 부분 |
| `registerStockAction` | POST `/stocks` | htmx |
| `deleteStockAction` | POST `/stocks/{id}/delete` | 확인 대화상자 뒤 htmx |
| `regenerateKeywordsAction` | POST `/stocks/{id}/keywords` | htmx |
| `setMarketAction` | POST `/stocks/markets/{KR\|US}` | htmx |
| `addTopicAction` | POST `/stocks/topics` | htmx |
| `deleteTopicAction` | POST `/stocks/topics/{id}/delete` | 확인 대화상자 뒤 htmx |
| `saveSettingsAction` | POST `/settings` | htmx, 폼 전체(필드 오류 표시) |
| `rebuildKeywordsAction` | POST `/settings/keywords/rebuild` | htmx |

- 결과 메시지 글자는 `server/stocks/messages.ts`와 각 `actions.ts`의 글자 그대로.
- Next의 `revalidatePath`(동작 뒤 화면 새로 고침)는 htmx 응답에서 바뀐 부분을 다시 그리거나 `HX-Refresh: true`로 한다. 어느 쪽이든 동작 뒤 보이는 결과가 TS와 같아야 한다.

### 5.4 클라이언트 동작 (`"use client"` 파일 16개)

| TS | Rust |
|---|---|
| `login-form.tsx` | 일반 폼 + 제출 중 버튼 잠금(작은 JS) |
| `stock-search.tsx`, `candidate-item.tsx` | htmx 폼 |
| `delete-stock-button.tsx`, `delete-topic-button.tsx`, `ui/alert-dialog.tsx` | HTML `<dialog>` + `app.js`의 열기·닫기, 확인 버튼이 htmx 요청 |
| `market-toggle.tsx`, `topic-form.tsx`, `regenerate-keywords-button.tsx`, `rebuild-keywords-button.tsx`, `collect-now-button.tsx`, `refresh-topics-button.tsx` | htmx(`hx-post`, `hx-disabled-elt`로 진행 중 잠금) |
| `settings-form.tsx` | htmx 폼, 서버가 필드 오류와 함께 폼 부분을 다시 그림 |
| `read-card.tsx` | `app.js`: 링크 클릭·버튼 클릭 시 카드를 먼저 바꾸고(낙관적) `POST /articles/read`. 실패하면 되돌림. 링크는 새 탭으로 열리는 기본 동작을 막지 않는다 |
| `auto-refresh.tsx` | `app.js`의 같은 간격 타이머(어느 화면에서 몇 초인지 TS 그대로) |
| `ui/label.tsx` | 일반 `<label>` |

- `app.js`는 이벤트 위임으로 한 파일에 짧게 쓴다. 프레임워크·빌드 도구를 더하지 않는다.

## 6. 하위 단계

각 단계 공통 완료 기준: ① 그 단계 TS 테스트가 모두 Rust로 옮겨져 통과 ② `cargo fmt --check`·`cargo clippy -D warnings`·`cargo test` 통과 ③ 기존 `pnpm test`·`pnpm test:e2e`가 계속 통과(R9 전에는 TS가 그대로 돌므로) ④ DECISIONS.md에 스스로 정한 작은 것 기록 ⑤ `/handoff`. 단계가 끝나면 결과를 보고하고 사용자 확인 뒤 다음 단계로 간다.

| 단계 | 내용 | 옮길 TS(테스트 포함) | 완료 기준(공통에 더해) |
|---|---|---|---|
| **R0** 준비 | 툴체인·뼈대·검사 명령 | 없음 | ① 사용자가 rustup 설치(아래) ② `rust/` 뼈대, `ripple --help`에 하위 명령 4개 ③ 2장 빈 버전 채움 ④ `/check`에 Rust 검사 추가(7.3) ⑤ Docker에서 `rust:1.99.1` 기반 빌드 시험(rustls의 암호 백엔드가 Docker 빌드 이미지에서 빌드되는지 확인, 안 되면 멈춤) ⑥ Tailwind 내려받기 스크립트 |
| **R1** 기반 | 설정·로그·시간·DB·마이그레이션·설정 표·공용 순수 함수 | `server/env`, `worker/config`, `server/log`, `server/time`, `server/db/{client,migrate,schema}`, `worker/migrate`, `server/settings`, `server/similar-title`, `tests/{db,fixtures,global-setup}` | 3.4의 마이그레이션 테스트 3개. `/real-check` 백업 사본에 `ripple migrate` → `user_version` 11, 스키마 그대로 |
| **R2** AI·예산 | OpenRouter, Jev, 모델 목록, 예산·사용량·보고 | `ai/llm/{types,openrouter,models}`, `ai/decider/{types,questions,typesafe}`, `budget/{usage,report}` | 스키마 파일 비교 테스트(3.11). Jev 재시도 테스트(wiremock). 질문 문구(`questions.ts`)가 글자 하나까지 같음 |
| **R3** 종목·목록 | 공식 목록 받기·찾기, 종목 매칭·등록·삭제 | `listings/{parse,sync,lookup}`, `stocks/{match,repository,messages}` | `tests/fixtures`의 KIND·NASDAQ 샘플로 같은 결과 |
| **R4** 키워드 | 트리 프롬프트·검증·점수, 생성, 대기열, 최근 제목, 읽기 | `keywords/{tree,generate,queue,headlines,read}` | 프롬프트 글자 비교 테스트(TS가 만든 프롬프트를 고정 입력으로 뽑아 파일로 두고 비교) |
| **R5** 수집기 | HTTP, robots, RSS, 피드 목록, Finnhub, 주소 정리, 발행 시각, 기사 걸기 | `collect/{http,robots,rss,feeds,finnhub,url,published,match}` | 8장 정규식·자르기 함정 테스트. 실제 피드는 부르지 않음 |
| **R6** 수집 흐름·평가 | 피드 제목 보관, 지난 기사 걸기, 회차 실행, 러너, Jev 평가 | `collect/{feed-headlines,backfill,run,runner}`, `evaluate/run` | `run.test.ts`(936줄)·`evaluate/run.test.ts` 전부 |
| **R7** 토픽 | 시장·주제 항목, 토픽 저장·계획·프롬프트·갱신 | `topics/{store,plan,refresh,run}` | `topics.test.ts`(1,025줄) 전부 |
| **R8** 주가·백업·스케줄 | KIS·장 시간·주가 받기·저장, 백업, 정기 작업 | `prices/{kis,plan,run,store}`, `backup`, `schedule/{plan,scheduler}` | 뉴욕 일광절약시간 바뀌는 날 테스트 |
| **R9** worker 완성·교체 | 내부 API, 시작 조립, 읽음 저장, 설정 폼 검사, Docker worker | `worker/{server,index}`, `auth/worker-token`, `timeline/reads`, `settings-form` | ① `playwright.config.ts`의 worker 명령을 `rust/target/release/ripple migrate && ... worker`로 바꾸고 **E2E 39개 통과(web은 Next.js 그대로)** ② compose의 migrate·worker를 Rust 이미지로(web은 TS 이미지 유지) ③ `/real-check`로 백업 사본 + 가짜 키로 한 바퀴(수집·평가는 키 없이 미룸 확인) ④ 비용 승인 뒤 실제 AI 짧게 확인 ⑤ **배포는 사용자가 요청할 때**(`/deploy`, 배포 전 백업) |
| **R10** web 기반 | 로그인·세션·미로그인 차단·레이아웃·예산 배너·상태 화면·정적 파일 | `auth/{config,session,rate-limit,request,guard}`, `proxy`, `instrumentation-node`, `worker-client`, `web-worker`, `tunnel`, `status`, `db/web`, `collect/read`, `evaluate/read`, `app/layout`, `app/(app)/layout`, `app/login/*`, `budget-banner`, `app/(app)/status/*`, `actions.ts`의 logout·collectNow·refreshTopics, `components/*`, `globals.css` | `e2e/auth.spec.ts`, `e2e/collect.spec.ts`, `e2e/home.spec.ts`(상태 부분) 통과. Playwright web 명령을 Rust로(타임라인 등 아직 없는 화면 시험은 이 단계에서만 `test.skip` 허용, 보고) |
| **R11** 타임라인 | 피드·필터·토픽 카드·주가 띠·읽음 | `timeline/read`, `topics/read`, `prices/store`(읽기 부분), `app/(app)/{page,timeline-card,timeline-filters,topic-cards,price-strip,read-card,impact}`, `actions.ts`의 setArticlesRead | `e2e/timeline.spec.ts`, `e2e/home.spec.ts` 통과. 데스크톱·휴대폰 캡처 비교 |
| **R12** 종목·설정 화면 | 종목 관리, 시장·주제, 키워드 트리, 설정·사용량 | `app/(app)/stocks/*`, `app/(app)/settings/*` | `e2e/stocks.spec.ts`, `e2e/keywords.spec.ts`, `e2e/settings.spec.ts` 통과, **E2E 전부 통과(skip 0, 모바일 건너뜀 5개는 지금과 같음)** |
| **R13** 정리·교체 | TS 제거, Docker 이미지 하나, 문서 | 남은 TS 전부 | ① `src/`, Next·drizzle 설정, TS 의존성 제거(E2E용 Playwright·better-sqlite3·tsx와 `scripts/*.mjs`는 남김, `package.json` 정리) ② Dockerfile을 Rust 하나로, compose 명령 바꿈 ③ README·SPEC 5·6장·CLAUDE.md 아키텍처 원칙·DEVELOPMENT 4장·`/check`·`/deploy`·`/real-check` 고침 ④ `/real-check` 전체 확인 ⑤ 배포는 요청할 때 |

**R0에서 사용자가 할 일**: `! curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --default-toolchain none` 뒤 새 터미널. 툴체인 버전은 `rust-toolchain.toml`이 정한다. (Claude가 대신 설치하지 않는다: 사용자 환경 변경)

## 7. 세션 운영

### 7.1 한 세션의 순서
1. DEVELOPMENT.md 1장, 이 문서 0~3장과 그 단계의 6장 줄을 읽는다.
2. 그 단계의 TS 파일과 테스트 파일을 모두 읽는다. 파일 목록과 Rust 파일 목록을 대응표로 먼저 만든다(세션 안에서만, 문서에 남길 필요 없음).
3. 테스트를 옮긴다 → 컴파일되게 빈 함수(`todo!()`)를 둔다 → 하나씩 채운다.
4. `cargo fmt`·`clippy`·`test`. 단계 끝에는 `/check`.
5. 결과 보고(표): 옮긴 파일 수·테스트 수(TS 몇 개 → Rust 몇 개), 스스로 정한 것, 발견한 TS 버그, E2E에서 고친 줄, 남은 것.
6. `/handoff`.

### 7.2 멈추고 물을 것
- 2장에 없는 크레이트가 필요할 때
- TS 동작을 그대로 옮길 수 없을 때(예: JS만의 동작이라 Rust로 같은 결과를 못 낼 때). 8장에 답이 있으면 그대로 하고, 없으면 묻는다
- 외부 계약(4장)을 바꿔야 할 것 같을 때
- TS 테스트의 기대값이 틀려 보일 때(고치지 말고 묻는다)
- 이 문서와 TS 코드가 다를 때
- 실제 외부 API를 불러야 확인할 수 있을 때(비용 승인)

### 7.3 검사 명령 (`/check`에 더할 것, R0)
| 순서 | 명령 |
|---|---|
| 1 | `cd rust && cargo fmt --check` |
| 2 | `cd rust && cargo clippy --all-targets -- -D warnings` |
| 3 | `cd rust && cargo test` |
| 4 | `cd rust && ./build.sh --release`(CSS + 빌드) |
| 5 | 지금의 TS 검사(R13 전까지) |
| 6 | `pnpm test:e2e` |

## 8. 함정 (JS와 Rust가 다른 곳)

| 무엇 | JS(TS) | Rust에서 할 일 |
|---|---|---|
| 문자열 길이·자르기 | `s.length`, `s.slice(0, n)`은 **UTF-16 단위** | `server/text.rs`에 `js_len(s)`(=`s.encode_utf16().count()`)와 `js_slice(s, 0, n)`을 두고 TS가 `length`·`slice`·`substring`을 쓴 곳은 모두 이것을 쓴다. 한글·영문은 1단위라 같지만 이모지는 2단위다. 서로게이트 쌍 가운데를 자르게 되면 그 앞에서 자른다(TS는 깨진 글자를 남기지만 Rust 문자열에 넣을 수 없음, 테스트로 고정) |
| 바이트 오프셋 | — | Rust `&s[a..b]`는 바이트 단위라 한글에서 패닉. 문자 위치 계산에 쓰지 않는다 |
| 정규식 앞뒤 보기 | `(?<!\d)`, `(?!\d)` 지원(`collect/rss.ts:135`) | regex 크레이트는 지원하지 않는다. 일치 결과 앞뒤 글자를 직접 확인하는 코드로 바꾸고 테스트를 더한다 |
| `\s`, `\p{L}` | `\s`는 유니코드 공백(BOM `﻿` 포함) | regex `\s`는 유니코드 공백이지만 `﻿`는 빠진다. `\s`가 쓰인 곳은 `[\s\u{FEFF}]`로 쓴다. `\p{L}`·`\p{N}`은 같다 |
| 정규식 플래그 | `/i`, `/g`, `/u`, `/m` | `(?i)`, `replace_all`/`find_iter`, (기본 유니코드), `(?m)` |
| 숫자 반올림 | `Math.round(x)`는 .5를 +∞ 쪽으로(−2.5 → −2) | `(x + 0.5).floor()`. Rust `round()`는 0에서 먼 쪽(−2.5 → −3)이라 쓰지 않는다 |
| `toFixed(n)` | 자릿수 반올림 | `format!("{:.n$}")`는 대부분 같지만 경계값(예: 1.005)이 다를 수 있다. `toFixed`가 쓰인 곳(23곳)은 TS 결과와 비교하는 테스트를 둔다 |
| 숫자를 글자로 | `String(5)`→`"5"`, `JSON.stringify(5.0)`→`"5"`, `0.1+0.2`→`"0.30000000000000004"` | `server/text.rs`의 `js_number(x)`: 정수 값이면 소수점 없이, 아니면 Rust `{}`(가장 짧은 표현, JS와 같음). 템플릿·JSON 쓰기에서 `f64`를 그냥 `{}`로 찍지 않는다(`5.0`이 `5.0`이 아니라 `5`로 나오는지 테스트). serde_json도 `f64` 5.0을 `5.0`으로 쓰므로 설정·JSON 열에 쓸 때 주의 |
| `Number(x)`, `parseInt` | 빈 문자열 → 0, `"12abc"` → NaN/12 | TS 코드가 기대는 정확한 결과를 테스트로 고정하고 그대로 만든다 |
| `NaN`·`Infinity` | 비교가 항상 false | 그대로 `f64`로 두되, `-Infinity` 초기값(`scheduler.ts` `lastListingCheck`)은 `Option<i64>`의 `None`으로 |
| 정렬 | `Array.sort`는 안정 정렬, 비교 함수 결과 부호 | `sort_by`(안정 정렬). `a - b`는 `a.partial_cmp(&b)` 또는 `total_cmp` |
| 한국어 정렬 | `localeCompare(b, "ko")`(`collection-panel.tsx:271`, `budget/report.ts:247`) | 유니코드 순서(`str::cmp`)로 바꾼다(결정: 한글끼리는 같은 순서, 영문·숫자가 섞이면 다를 수 있음) |
| 숫자 표시 | `Intl.NumberFormat("ko-KR")`(천 단위 쉼표), `toLocaleString("ko-KR")` | `server/text.rs`의 `group_thousands`. 미국 가격은 `en-US`, 소수 둘째 자리(`price-strip.tsx`) |
| 날짜 | `new Date(ms)`, `Date.UTC`, `toISOString()` | `EpochMs` + chrono. `toISOString().slice(0,10)`은 UTC 날짜(`finnhub.ts:116`) |
| `undefined`와 `null` | JSON에서 `undefined` 필드는 빠지고 `null`은 남는다 | `Option` 필드에 `#[serde(skip_serializing_if = "Option::is_none")]`를 쓸지는 TS가 `undefined`를 넣는지 `null`을 넣는지 보고 정한다. 응답·DB JSON 모두 |
| 객체 키 순서 | `JSON.stringify`는 넣은 순서 | serde 구조체는 필드 선언 순서. DB JSON·응답의 키 순서를 TS와 같게 선언한다(비교 테스트가 글자로 비교하는 곳이 있음) |
| 빈 값 판단 | `if (!x)`는 `0`, `""`, `null`, `NaN`을 모두 거짓 | 조건마다 무엇을 거르는지 풀어서 쓴다(`x.is_empty()`, `x == 0`) |
| `Promise.all` | 동시에 시작, 하나 실패하면 전체 실패 | `futures` 없이 `tokio::join!`/`try_join!`. 단일 스레드라 동시 시작은 같다 |
| 동시에 부르는 HTTP | Node `fetch`는 연결을 재사용 | `reqwest::Client` 하나를 만들어 나눠 쓴다(요청마다 만들지 않음) |
| 정수 크기 | JS 숫자는 64비트 실수(정수 안전 범위 2^53) | DB id·시각은 `i64`, 개수는 `usize`/`i64`. `as` 변환 대신 `try_from` |
| SQLite 설정 | better-sqlite3: `foreign_keys` 켬, `DQS=0`(큰따옴표 문자열 금지) | 3.3대로 `foreign_keys = ON`. SQL 문자열 값은 작은따옴표만 쓴다 |

## 9. 확인하지 못한 것 (해당 단계에서 확인)

| 무엇 | 언제 | 어떻게 |
|---|---|---|
| rusqlite_migration이 `user_version`을 쓰고 트랜잭션으로 적용하는지 | R1 | docs.rs 공식 문서, 시험 DB |
| reqwest 0.13 기본 rustls 암호 백엔드가 Docker 빌드 이미지에서 빌드되는지 | R0 | Docker 빌드 시험 |
| `rust:1.99.1` 공식 Docker 이미지 태그(slim·trixie) | R0 | Docker Hub |
| askama 0.16 문법(매크로·상속·필터) | R10 | docs.rs, 공식 책 |
| htmx 2.0.11의 `hx-disabled-elt` 등 쓰려는 속성 | R10 | htmx.org 문서 |
| Tailwind standalone 4.3.3 릴리스 체크섬 | R0 | GitHub 릴리스 |
| TypeSafe SDK 기본 재시도 값 | R2 | `node_modules/@typesafe-ai/sdk/dist/index.mjs` |
