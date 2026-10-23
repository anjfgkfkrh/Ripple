# 개발 현황과 이어서 하기

새 세션은 이 문서부터 읽는다. CLAUDE.md는 자동으로 읽히고, 요구사항·결정 기록·미결정 목록은 [SPEC.md](../SPEC.md)에 있다(결정은 12장, 실측 결과는 11장). 사용법은 [README.md](../README.md).

이 문서는 개발하는 동안 계속 고친다. 아래 "지금 상태"가 실제와 다르면 git log와 `git status`를 먼저 본다.

## 1. 지금 상태

마지막 갱신: 2026-10-22 20:50 (한국 시간), 11단계 실제 AI 확인·본 Docker 배포 끝(커밋 전)

- **11단계 진행 중: 시장 전체 기사**(SPEC 10장, 결정은 12장 "11단계 …" 줄, 2026-10-22 사용자 결정 "A, 추천안대로 + 주제 직접 추가").
  - 설계: '한국 증시'(미국 증시는 켜고 끄기)와 사용자가 추가한 주제를 종목 표에 종류('시장'·'주제')로 넣고, 거시 키워드 트리(구조 + 오늘의 시장 이슈로 만든 시사 가지)로 모든 수집 기사에 건다(이름으로는 걸지 않음). Jev는 시장 전체 기준으로 판정. 같은 타임라인에 '시장'·주제 칩과 필터. 비슷한 제목은 한 카드로 묶는다(종목 기사에도).
  - [x] 10단계 커밋(`32f4c0d`), 결정 기록(SPEC 12장)
  - [x] 10단계 보완: 시사 사슬을 따로 받는 출력 형식(`tree.ts`의 `currentChains`). 실측 $0.0077(4번 모두 사슬 2개 온전, SPEC 11장). 단위 360·E2E 30 통과, 빌드 통과. 본 Docker 배포(2026-10-22 14:48, 사용자 결정). 커밋은 11단계 커밋에 함께
  - [x] 11단계 질문 묶음 답(SPEC 12장 "11단계 …"): 주제는 증시 기준, 종목 화면 '시장·주제' 구역, 한국 증시 켠 채로·미국 꺼 둠, 주제 개수 제한 없음, 주제는 켜진 증시(둘 다면 둘 다) 기준 평가
  - [x] 사용자 질문에 답하고 형태를 '토픽 우선'으로 다시 정함(SPEC 12장 "11단계 형태: 토픽 우선" 이하). 10단계 보완·결정 기록 커밋(`6987127`)
  - [x] 남은 질문 답(제목 보관에 주소·요약 3일, 토픽 카드 규칙, 자동 토픽은 필터 칩 없음)과 실측 비용(약 $0.015) 승인 → SPEC 12장
  - [x] 종목–토픽 연결: A(토픽 갱신 때 관련 내 종목을 정해 토픽 기사를 그 종목에도 걸고 Jev 판정, 토픽 카드에 관련 종목)
  - [x] Jev 점수 기반 설계 확정(SPEC 12장 "11단계 Jev 점수 기반 설계" 이하): 기사 표시는 관련도·영향도 둘 다 기준 이상, 트리 안전 상한 40, AI 후보+Jev 점수로 토픽 붙이기, 트리에 점수 표시, 06:00 유지, 실측 약 $0.025 승인
  - [x] 실측 $0.0241(SPEC 11장 "11단계 Jev 점수 기반 실측"). 이미 쓴 코드(테스트는 아직): `ai/decider/questions.ts`(대상별 질문, 한국어 지시문 삭제), `ai/decider/types.ts`·`typesafe.ts`(영향도, `rate`), `keywords/tree.ts`(후보 넉넉히·대상별 프롬프트·`pruneKeywordTree`), `topics/refresh.ts`(토픽 갱신 프롬프트), `budget/report.ts`(용도 이름). `evaluate/run.ts`는 임시로 kind "stock"
  - [x] 기준값·비용 결정(SPEC 12장): 기사 표시 관련도≥6·영향도≥6, 키워드·토픽 관련도≥6·영향도≥5(둘 다 설정 가능), 비슷한 제목은 대표 기사 점수를 그대로 씀, 사용량에 기능별·항목별 비용과 비율
  - 구현 순서(하나씩 끝내고 체크):
    - [x] 비슷한 제목 기준 무료 실측: '…' 앞부분 0.5 이상 또는 전체 0.4 이상(SPEC 11·12장). 평가 21% 줄어듦
    - 참고: 이 맥의 python3(3.9.6)은 한글이 든 아주 긴 줄을 잘못 읽어 SyntaxError가 난다. 긴 한글 문서 수정은 편집 도구나 node로 한다
    - [x] DB(마이그레이션 0007): stocks.kind·topic_id, topics 표, keywords.relevance·magnitude·topic_id, feed_headlines.url·url_key·summary, evaluations.magnitude, usage_logs.stock_id, 설정 키
    - [x] 수집: 제목 보관에 주소·요약, 시장·주제는 이름으로 걸지 않음, 끝난 토픽은 걸지 않음, 보관 제목으로 지난 24시간 다시 걸기
    - [x] 트리: 후보 → Jev 평가 → 기준으로 자르기(상한 40), 토픽이 붙인 가지는 재생성 때 남김, 대기열에 decider
    - [x] 토픽 갱신: 스케줄(간격·시각), 이어 붙이기 보정, 토픽·관점 항목(켜진 증시마다), 관련 종목 Jev 평가 → 붙이기·떼기, 지난 24시간 다시 걸기, 시장 이슈 스냅숏
    - [x] 평가: 대상 종류·영향도 저장, 비슷한 제목 점수 복사, 사용량에 항목
    - (위 다섯 줄은 코드만 씀, 테스트는 아래 테스트 단계에서. 파일: `similar-title.ts`, `collect/backfill.ts`·`feed-headlines.ts`·`match.ts`·`run.ts`, `keywords/generate.ts`·`tree.ts`·`market-issues.ts`·`queue.ts`, `topics/plan.ts`·`refresh.ts`·`run.ts`·`store.ts`, `evaluate/run.ts`, `budget/usage.ts`, `schedule/*`, `worker/index.ts`·`server.ts`(PUT /markets/:m, POST·DELETE /topics, POST /topics/refresh))
    - [x] 화면: 타임라인 토픽 카드(`src/app/(app)/topic-cards.tsx`, 읽기는 `timeline/read.ts`의 `readTopicCards`)·항목 칩(종목 뒤에 켜진 증시·사용자 주제)·표시 규칙(관련도·영향도, 11단계 전 평가는 관련도만)·비슷한 제목 묶기(묶인 기사의 항목을 모음), 종목 화면 '시장·주제'(`stocks/market-topics.tsx` 등), 트리 점수·'토픽' 표시, 설정(영향도·키워드 기준, 토픽 갱신 간격·시각, 기능별·항목별 비용과 비율), 상태 화면 '시장 토픽'(`status/topic-panel.tsx`, "지금 갱신")
    - [x] 토픽 갱신 상태 기록: 설정 값 `topics.startedAt`·`failedAt`·`error`. 재시도(30분)는 스케줄러 메모리 대신 `topicPlan`이 이 기록으로 판단
    - [x] 테스트: 단위 425 통과(51개 파일, 새로 `topics/topics.test.ts`·`collect/backfill.test.ts`·`keywords/read.test.ts` 등), E2E 35 통과(5개는 설계상 건너뜀, 두 번 돌려 같음), lint·타입·포맷·빌드(web·worker) 통과. 화면 캡처 확인(토픽 카드 모바일·데스크톱, 토픽 필터, 시장·주제, 상태, 설정)
    - [x] README·SPEC 본문(1.2, FR-3~9, 6.1~6.3, 7장, 9장, 10장 완료 기준)과 12장 "11단계 … — 구현 중 정함" 줄
    - [x] 무료 실제 확인(SPEC 11장 "11단계 실제 확인(무료)"): 본 DB를 읽기 전용으로 뜬 사본을 시험용 Docker에서 AI 키·터널 없이. 확인 뒤 모두 지움
    - [x] 완료 보고 → 사용자 결정 A(유료 실제 확인 뒤 배포)
    - [x] 실제 AI 확인 $0.0284(SPEC 11장): 트리 출력이 8,000토큰에 잘리는 문제를 찾아 16,000·120초로 늘림. 사용량은 본 DB에 `*_check`로 옮김. 시험용 Docker는 지움
    - [x] 본 Docker 배포(2026-10-22 20:42, 배포 직후 $0.0152): 한국 증시·토픽 12개·종목에 7개 붙임
    - [ ] 다음 단계 시작 때 11단계 커밋(규칙)
  - [x] 미래 발행 시각 고침(2026-10-23 사용자 결정 A): `collect/published.ts`, 마이그레이션 `drizzle/0008_fix_future_published.sql`. 본 Docker 배포(2026-10-23 11:59): 미래 시각 기사 0건 확인
  - [x] 키워드 트리에서 시사 빼기(2026-10-23 사용자 결정 A, SPEC 12장): 트리는 구조 키워드만(`tree.ts`의 `currentChains`·시장 이슈 재료 제거), 하루 1번 시장 이슈 정리(`keywords/market-issues.ts`)와 토픽 갱신의 스냅숏 저장을 없앰(`market_issues` 표는 지난 기록으로 남김). 단위 408·E2E 35 통과. 배포는 요청 때. 배포하면 다음 06:00 재생성(또는 "다시 만들기")부터 트리에서 시사 키워드가 사라진다
  - E2E에서 알아 둘 것(11단계): 수집 시험이 "지금 갱신"으로 실제 토픽 흐름(갱신 → 지난 기사 걸기 → 다시 수집 → 토픽 카드)을 돈다. 가짜 RSS에 무관한 제목 20개를 더해 토픽 갱신이 돈다. 타임라인 시험은 토픽 '중동 긴장'·'옛 이슈'를 DB에 직접 넣고, 가짜 AI는 '중동 긴장'을 이어지는 토픽으로 돌려준다(시험끼리 토픽을 끝내지 않게)
- **10단계 완료(2026-10-22 배포): 키워드 트리에 시사 반영**(SPEC 10장, 결정은 12장 "10단계 …" 줄, 2026-10-22 사용자 결정 "1~8 A").
  - 설계: 수집 회차마다 피드 제목을 모두 3일 보관 → 매일 06:00에 LLM 1번으로 "오늘의 시장 이슈" 10~15개 정리(모든 종목 공용) → 종목마다 트리를 만들 때 종목 기사 제목과 함께 넣어, 1단계 5개 중 최대 2개를 시사 가지(테마 → 사건 → 동인 사슬)로 만든다. 키워드에 구조/시사 구분 저장, 트리 화면에 '시사' 표시.
  - [x] 결정 기록(SPEC 12장), 릴리스 묶음 변경 커밋(`0e73b4e`)
  - [x] 실측(승인 약 $0.03, 실제 $0.0281) → SPEC 11장 "시사 반영 키워드 실측". 3단계 누락 원인(LLM이 2단계를 부모마다 5개씩 만들고 3단계를 안 만듦)을 찾았고 새 프롬프트는 4번 모두 5·5·5. 시사 연결 40건은 사람·Jev 모두 기준 미만이라 시사용 Jev 문구는 두지 않음(SPEC 12장)
  - [x] 구현(작은 결정은 SPEC 12장 "10단계 … — 구현 중 정함")
    - DB: `keywords.kind`(구조/시사), `feed_headlines`(제목 3일), `market_issues`(하루 한 번 정리). 마이그레이션 `drizzle/0006_market_issues.sql`(표 추가·열 추가뿐)
    - 수집: `src/server/collect/feed-headlines.ts`, `run.ts`의 `Source.keepHeadlines`(RSS·Finnhub 시장 뉴스)
    - 시장 이슈: `src/server/keywords/market-issues.ts`의 `ensureMarketIssues`(오늘 06:00 뒤 정리가 없으면 한 번, 제목 50개 미만이면 건너뜀, 실패는 빈 정리로 1시간)
    - 트리: `tree.ts`(새 프롬프트, 종류, 시사 사슬, 부모별 고르게 남기기), `generate.ts`(시장 이슈 넣기, 최대 토큰 8,000, 대기 60초, 원문 로그), LLM `timeoutMs`
    - 화면: 트리의 시사 가지 맨 위 키워드에 '시사'. 사용량 표: '시장 이슈 정리', 같은 이름 합치기
  - [x] 테스트·전체 검사: 단위 357 통과(22개 늘어남), E2E 30 통과(4개는 설계상 건너뜀), lint·타입·포맷·빌드 통과. 트리 화면 캡처 확인(데스크톱·모바일)
  - [x] README·SPEC 본문(FR-3·FR-4·6.2·7장) 고침
  - [x] 무료 실제 확인: 시험용 Docker에 오늘 04:00 백업 사본, AI 키·터널 없이 → 마이그레이션 적용, 제목 3,346건 보관(SPEC 11장). 확인 뒤 모두 지움
  - [x] 완료 보고 → 사용자 선택 B(시험용 Docker에서 실제 AI 확인 뒤 배포)
  - [x] 실제 AI 확인($0.0132, SPEC 11장): worker 안에서 제목 보관 → 시장 이슈 13개 → 현대건설 시사 가지 생김. 두산에너빌리티는 LLM이 규칙을 어겨(23개, 시사 사슬을 구조 가지 아래에) 구조 13개만 남음. 사용량은 본 DB에 `*_check`로 옮김
  - [x] 본 Docker 배포(2026-10-22 14:03): 마이그레이션 7개 적용, 기존 키워드는 '구조', 터널 주소 그대로. 피드 제목은 다음 정기 수집부터 쌓이고, 트리는 다음 06:00(또는 사용자가 "키워드 다시 만들기")에 새 방식으로 바뀐다
  - [x] 사용자에게 물은 것: 시사 사슬을 따로 받는 출력 형식(→ 10단계 보완으로 함), 시장 전체 기사 수집 아이디어(→ 11단계)
  - [x] 커밋(`32f4c0d`, 보완 `6987127`)
- **MVP 완료, v0.1.0 릴리스(2026-10-21).** 0~9단계 모두 끝났고, 휴대폰에서 임시 주소 접속·로그인을 사용자가 확인했다.
- 9단계에서 할 일
  - [x] compose에 `tunnel` 서비스(cloudflared `2026.10.0`, Quick Tunnel, `--metrics 0.0.0.0:20241`)
  - [x] 상태 화면에 지금 임시 주소와 마지막 백업 표시(`src/server/tunnel.ts`, `TUNNEL_METRICS_URL`)
  - [x] worker 자동 백업: 매일 04:00(한국 시간), 호스트 `./backups`, 최근 7개, 놓쳤으면 켜질 때 한 번(`src/server/backup.ts`, 스케줄러, `BACKUP_DIR`)
  - [x] 시험용 Docker를 본 Docker와 따로 띄울 수 있게 compose 포트·백업 폴더를 변수로(`WEB_PORT`, `BACKUP_HOST_DIR`)
  - [x] README를 실행 안내 중심으로 다시 쓰기
  - [x] 테스트 추가와 전체 검사: 단위 329, E2E 30 통과(4개는 설계상 건너뜀), lint·타입·포맷·빌드 통과
  - [x] E2E 로그인 간헐 실패 원인 찾고 고침(아래 4장)
  - [x] 실제 확인: 시험용 Docker로 임시 주소 접속·로그인·서버 액션, 백업·복원 확인 후 내림(SPEC 11장)
  - [x] 완료 보고, 휴대폰 접속 확인(사용자)
- 본 Docker는 9단계 이미지로 돌고 있으며 휴대폰에서 접속이 확인되었다.
- 릴리스 이미지 배포(2026-10-21 사용자 요청): `scripts/release.sh`로 arm64·amd64 이미지 압축 파일과 설치용 파일을 `release-dist/`에 만든다(README 7장, SPEC 12장 "배포 방식"). 만들기까지 했고 GitHub 릴리스 페이지 업로드는 사용자가 직접 한다. 스크립트·INSTALL.md·README·SPEC 변경은 10단계를 시작하기 전에 커밋했다(2026-10-22).
- 다음: 11단계 배포 뒤 사용자 확인. 11단계 변경은 아직 커밋하지 않았다(다음 단계 시작 때 커밋, 규칙). 그 뒤 MVP 범위 밖(SPEC 2.2)의 기능(요약, 외부 알림, 고정 도메인 등)은 새 단계로 정해서 시작한다.

## 2. 단계별 현황

| 단계 | 내용 | 상태 | 커밋 |
|---|---|---|---|
| 0~4 | 프로젝트 기반, 로그인, 종목 등록, 연관 키워드, 기사 수집 | 완료 | `ea0609c` |
| 5 | Jev 기사 평가 | 완료 | `82e49d7` |
| 6 | 정기 수집, 누락 보정, 키워드 자동 재생성 | 완료 | `895819c` |
| 7 | 타임라인과 필터 | 완료 | `53e0df7` |
| 8 | 설정 화면, 사용량, 비용 한도 | 완료 | `b121a23` |
| 9 | Cloudflare Tunnel, 실행 안내, 백업 | 완료 | `f972e3c` |
| - | v0.1.0 릴리스 준비, 릴리스 묶음 스크립트 | 완료 | `575bef2`, `0e73b4e` |
| 10 | 키워드 트리에 시사 반영 | 완료(2026-10-22 배포) | `32f4c0d` |
| 11 | 시장 전체 기사(토픽 우선: 자동 토픽·한국 증시·사용자 주제), Jev 점수로 트리·기사 고르기, 비슷한 제목 묶기, 항목별 비용 | 완료(2026-10-22 배포) | 커밋 전 |

릴리스: **v0.1.0 (MVP)**, 태그 `v0.1.0`. **v0.2.0**(2026-10-23, 10·11단계: 시사·시장 토픽·Jev 점수 기반·같은 소식 묶기·항목별 비용), 태그 `v0.2.0`, GitHub 릴리스 페이지에 `gh release create`로 올림(이제 `gh`가 있음, 커밋 `4626733`).

## 3. 일하는 규칙 (CLAUDE.md에 더해 사용자가 요청한 것)

- **단계 시작 = 이전 단계 커밋**: 새 단계를 시작할 때 이전 단계 변경을 main에 커밋한다. 커밋 전에 `.env`의 비밀 값과 흔한 키 형식이 스테이징 내용에 없는지 검사하고 `.env`가 스테이징되지 않았는지 본다. 푸시는 요청할 때만.
- **질문 → 기록 → 구현**: 단계 시작 전에 그 단계의 미결정(D-xx)과 화면·비용·구조 관련 질문을 모아 A/B·장단점·추천으로 묻는다. 답은 SPEC 12장(13장 상태)에 먼저 적고 구현한다.
- **작은 결정**: 스스로 정한 것은 SPEC 12장에 "— 구현 중 정함"으로 적고 보고에 표로 알린다. 사용자가 이의 없이 다음 응답을 하면 "— 구현 중 정함, 보고 후 이의 없음"으로 바꾼다.
- **돈이 드는 실행**: 예상 비용을 말하고 승인받는다. 승인과 실제 사용은 SPEC 12장 "N단계 실제 API 호출" 줄과 아래 5장에 적는다. 개발 중 worker 밖에서 직접 부른 AI 호출도 사용량 기록(`usage_logs`)에 `*_check` 용도로 넣어 월 합계를 맞춘다.
- **본 Docker에는 사용자 실제 데이터가 있다(2026-10-21부터)**: compose 프로젝트 `ripple`(볼륨 `ripple_ripple-data`)은 사용자가 쓰고 있다. 실제 확인은 시험용 Docker(프로젝트 `ripple-check`, 아래 4장)로 한다. 본 Docker의 비밀번호·설정·종목·예산을 바꾸지 않고, 재빌드(배포)는 사용자가 요청할 때만 한다. 읽기 전용 조회는 괜찮다.
- **문서화**: 이 문서를 단계 중간에도 계속 고친다(답을 받았을 때, 구현이 끝났을 때, 검사·확인 뒤, 보고 뒤, 커밋 뒤). 세션 전용 임시 폴더(scratchpad)에 둔 것은 새 세션에서 사라지므로, 다시 필요할 방법·절차는 이 문서에 적는다.
- 답변은 한국어로, 결론 먼저, 비교는 표로.

## 4. 실행·검사

| 명령 | 내용 |
|---|---|
| `pnpm lint` / `pnpm typecheck` / `pnpm format:check` | 검사 |
| `pnpm test` | Vitest(실제 마이그레이션으로 만든 임시 DB) |
| `pnpm test:e2e` | Playwright. 가짜 외부 서비스(4200), 시험용 worker(4100)·web(3100)을 띄운다. `.e2e/ripple.db`를 새로 만든다 |
| `pnpm build` | Next 빌드 |
| `docker compose up -d --build` | 본 Docker(사용자 것, 위 규칙 참고) |

E2E에서 알아 둘 것
- 시험용 worker는 `SCHEDULER_DISABLED=1`로 정기 작업을 끈다(지금 수집과 겹치지 않게).
- 타임라인·설정 테스트는 시험 DB에 데이터를 직접 넣는다(멜콘·나노솔루션 종목, 사용량 4.2달러). 앱 전체 값을 바꾸는 테스트는 데스크톱에서만 차례로 돌고 바로 되돌린다.
- 로그인 테스트 헬퍼(`e2e/helpers.ts`의 `submitPassword`)는 앞 제출이 끝날 때까지 기다린 뒤 비밀번호를 넣는다. React가 제출이 끝난 뒤 폼을 비우는데, 그 전에 넣은 값이 지워져 브라우저가 빈 칸 제출을 막던 것이 7·9단계 간헐 실패(30번 중 1번꼴)의 원인이었다. 고친 뒤 140번 되풀이해 실패 0.
- 되풀이 실행: `pnpm exec playwright test e2e/auth.spec.ts --repeat-each=10`(접속자 라벨에 되풀이 번호가 붙어 실패 제한이 섞이지 않음).

시험용 Docker(본 Docker와 볼륨·포트·백업 폴더를 나눔, 2026-10-21 9단계에서 씀)
- 띄우기: `WEB_PORT=3300 BACKUP_HOST_DIR=./.check-backups OPENROUTER_API_KEY= TYPESAFE_API_KEY= FINNHUB_API_KEY= APP_PASSWORD=… SESSION_SECRET=… docker compose -p ripple-check up -d --build`
  - AI 키를 비워 비용이 생기지 않게 한다. 터널까지 뜨므로 공개 주소가 생긴다. 외부에서 확인할 때는 공개된 시험 비밀번호(`e2e/test-env.ts`) 대신 무작위 비밀번호를 파일로 만들어 쓰고(출력하지 않음) 끝나면 지운다.
  - 임시 주소: `docker compose -p ripple-check logs tunnel | grep trycloudflare` 또는 http://127.0.0.1:3300/status
- 내리기: `WEB_PORT=3300 BACKUP_HOST_DIR=./.check-backups docker compose -p ripple-check down -v`, `rm -rf .check-backups`, 시험 이미지 `docker image rm ripple-check-web ripple-check-worker ripple-check-migrate`
- 복원 시험: `down` → 시험 볼륨(`ripple-check_ripple-data`)에 README의 복원 명령 → `up -d`

## 5. 데이터·비용 메모

- 본 Docker DB(2026-10-21 17:49 조회): 사용자 종목 두산에너빌리티(id 4). 시험 종목 삼성전자·엔비디아·현대건설(id 1~3)은 숨김 상태(deleted_at 1792465169686·…692·…693). 시험에 다시 쓰지 않는다.
- 본 Docker DB(2026-10-22 조회): 사용자 종목 현대건설(id 3, 2026-10-21 저녁 다시 등록)·두산에너빌리티(id 4), 깊이 3. 삼성전자·엔비디아(id 1·2)는 숨김.
- 이번 달 AI 사용: $0.1424(2026-10-22, 한도 $5, 개발 중 시험 호출 포함).
- 단계별 실제 API 사용: 2단계 OpenRouter 약 20회, 3단계 약 $0.016(누적), 5단계 Jev $0.0195(548회), 6단계 $0.0205(OpenRouter 15회, Jev 339회), 7·8단계 0원, 10단계 실측 $0.0281(OpenRouter 10회, Jev 80회)과 실제 AI 확인 $0.0132(OpenRouter 3회, Jev 88회), 10단계 보완 실측 $0.0077(OpenRouter 4회), 11단계 실측 $0.0241(OpenRouter 5회, Jev 137회), 모두 본 DB 사용량 기록에 `*_check`로 넣음. 11단계 무료 실제 확인은 0원, 실제 AI 확인 $0.0284(OpenRouter 5회, Jev 300회, 본 DB에 `*_check`로 옮김). 본 DB 이번 달 합계 약 $0.186(배포 직후).

## 6. 다시 잴 때의 방법

세션 임시 폴더의 스크립트는 남아 있지 않으므로 아래 절차로 다시 만든다. 결과는 SPEC 11장에 적는다.

- **Jev 지시문 언어·모델 버전 비교(5단계, D-23)**: 시험 종목과 키워드를 새 임시 DB에 넣고 `startCollectionRun`(decider 없이)으로 실제 기사를 모은다 → 연결을 연결 방식·단계별로 섞어 50건 뽑는다 → 사람이 관련도(0~5)·호재/악재를 먼저 판정한다 → `createTypeSafeDecider`로 언어 2 × 버전 2를 부른다 → 6점 기준 일치, 평균 오차, 호재·악재 일치, 입력 토큰, 지연을 비교한다. 사용량은 `article_evaluation_check`로 본 DB에 옮긴다.
- **키워드 프롬프트 비교(6단계)**: 같은 DB를 둘로 복사해 한쪽에만 평가를 넣고(최근 기사 제목이 생기게) `generateKeywordsForStock`을 종목마다 두 번씩 부른다 → 트리를 나란히 비교한다. 사용량은 `keyword_generation_check`.
- **피드에 담긴 기간(4·6단계, D-01 근거)**: 피드마다 받은 기사 중 가장 최근과 가장 오래된 발행 시각의 차이를 잰다. 바쁜 피드는 약 2시간치다.
- **시사 반영 키워드(10단계)**: ① 무료: `RSS_FEEDS`·`createHttpClient`·`createRobotsChecker`·`parseFeed`·`createFinnhubClient().generalNews()`로 피드를 한 번 받아 제목·요약·주소·발행 시각을 파일로 둔다(저장소·DB에 쓰지 않음). 본 DB에서 종목 정보와 `recentHeadlines` 규칙의 제목을 읽기 전용으로 꺼낸다(`docker compose exec -T worker node - < 스크립트`, better-sqlite3 `readonly`). ② 유료: 최근 24시간 제목(같은 제목 하나, 최근순 1,500개)으로 `buildMarketIssueMessages` 2번, 종목마다 `git show <이전 커밋>:src/server/keywords/tree.ts`로 꺼낸 지금 프롬프트와 새 `buildKeywordMessages`를 2번씩(`createOpenRouterClient`, 응답 대기 넉넉히). 비용 합계가 상한을 넘으면 멈추게 한다. ③ 무료: 키워드마다 `createMatcher`로 최근 48시간 기사 적중 수를 센다. ④ 시사 키워드로 걸린 기사를 경로마다 고르게 40건 뽑아 **먼저** 사람 판정(0~5)을 파일로 남기고, `createTypeSafeDecider`(지금 문구)와 SDK `systemOne`(link.how만 바꾼 문구)으로 묻는다. ⑤ 사용량은 본 DB `usage_logs`에 `market_issues_check`·`keyword_generation_check`·`article_evaluation_check`로 넣는다(쓰기 연결, busy_timeout 5초, 한 트랜잭션). 스크립트는 세션 임시 폴더에 두고 `.mts`로 쓴다(프로젝트가 ESM). 패키지는 `node_modules/...` 절대 경로로 가져온다.
- **worker 안 실제 AI 확인(10단계)**: ① `WEB_PORT=3300 BACKUP_HOST_DIR=./.check-backups docker compose -p ripple-check build` ② `docker compose -p ripple-check run --rm --no-deps -v "$PWD/backups":/restore:ro worker sh -c '<백업 복사와 node 스크립트>'`로 백업 사본을 `/data/ripple.db`에 넣고, 같은 자리에서 settings `keywords.autoRegeneratedAt`을 지금 시각으로 바꾼다(켜자마자 06:00 재생성이 돌면 제목이 쌓이기 전이라 시장 이슈가 빠짐) ③ `docker compose -p ripple-check up -d migrate worker web`(터널을 빼서 공개 주소 없음, 실제 키는 `.env`) ④ 첫 수집이 끝나면 worker 컨테이너 안에서 `fetch('http://127.0.0.1:4000/stocks/<id>/keywords', {method: 'POST', headers: {authorization: 'Bearer ' + process.env.WORKER_API_TOKEN}})`로 다시 만들기(토큰을 밖으로 꺼내지 않음) ⑤ 지난 기사까지 보려면 시험 DB의 `stocks.backfill_from`을 24시간 전으로 두고 `POST /collect` ⑥ `market_issues`·`keywords`·`article_matches`·`evaluations`를 읽기 전용으로 조회 ⑦ 시험 DB의 이번 `usage_logs`를 용도 끝에 `_check`를 붙여 본 DB에 옮기고 `down -v`, `.check-backups`·시험 이미지를 지운다.

## 7. 남은 일·알려진 문제

- 키워드를 매일 다시 만들면 10개 중 0~6개만 겹친다(제목 없이도 같음). 바뀌는 게 부담되면 설정에서 주 1회로.
- 최근 기사 제목을 넣으면 LLM이 단계별 5개를 넘겨 답하는 일이 있다(검증에서 버려 트리는 정상, 출력 토큰만 낭비).
- 같은 소식이라도 언론사마다 제목이 많이 다르면(겹침 0.2~0.35) 묶이지 않는다(11단계 기준은 '…' 앞부분 0.5 또는 전체 0.4, SPEC 11장).
- (11단계) Jev는 키워드·토픽 점수를 사람보다 후하게 매긴다(대부분 6~9점, SPEC 11장). 기준(6·5)으로 남는 키워드가 많으면 설정에서 올린다.
- (11단계) AI가 키워드 후보를 요청(단계마다 5~8개)보다 훨씬 많이 낼 때가 있다(한국 증시 112개). 100개에서 자르고 Jev 평가 뒤 40개 상한으로 남긴다(출력·평가 비용만 늘어남).
- (11단계) 한국 증시 기사가 많이 보일 수 있다(실제 확인: 하루 평가 183건 중 40건이 기준 이상, 한은 금리 결정 날). 많으면 설정에서 영향도 기준을 올린다.
- (11단계) 토픽 갱신에서 AI가 이전 토픽 id를 돌려주지 않을 때가 있어 이름·검색어가 같으면 같은 토픽으로 잇는다. 이름과 검색어가 모두 바뀌면 새 토픽이 되고 이전 토픽은 끝난다.
- 새로 등록한 종목의 지난 24시간 기사 연결은 그 회차에 실패한 소스에서는 놓칠 수 있다.
- (10단계) 시장 이슈 정리는 회사 한 곳의 큰 소식(실적·신차)도 넣고, 기사가 흩어진 주제(예: 트럼프 기사 하루 43건)는 이슈로 따로 잡지 않기도 한다. 시사 키워드 중 넓은 말('기준금리 동결' 하루 17건, 'AI 투자 우려' 15.5건)은 연결이 많지만 대부분 기준 미만이다. 큰 사건 때 시사 연결이 6점을 넘는지는 아직 재지 못했다(SPEC 11장).
- (고침, 2026-10-23) 국민일보 피드가 다음 날 지면 기사에 4~24시간 미래 시각을 붙여 와 타임라인에 미래 시각으로 보였다. 받은 시각보다 10분 넘게 미래면 받은 시각으로 저장하고, 저장된 것은 마이그레이션 0008로 고침(SPEC 12장 "미래 발행 시각"). 2026-10-23 11:59 배포, 본 DB 기사 14건·제목 126건 고침
- MVP 범위 밖(SPEC 2.2): 요약, 외부 알림, 미국 증권사 리포트, 다중 사용자, 종목별 수집 간격, 고정 도메인.

## 8. 코드 지도

폴더 구조는 README "폴더 구조". 자주 보는 진입점:
- worker 시작과 의존성 조립: `src/worker/index.ts`, 내부 API: `src/worker/server.ts`
- 정기 작업: `src/server/schedule/`(plan = 다음 시각 계산, scheduler = 1분 루프)
- 수집: `src/server/collect/run.ts`, 평가: `src/server/evaluate/run.ts`, Jev: `src/server/ai/decider/`
- 키워드: `src/server/keywords/`(tree = 프롬프트·검증·점수로 고르기(구조 키워드만), generate = 생성·Jev 평가·저장, queue = 대기열, headlines = 종목의 최근 기사 제목), 피드 제목 보관: `src/server/collect/feed-headlines.ts`, 지난 기사 걸기: `src/server/collect/backfill.ts`
- 시장·주제·토픽(11단계): `src/server/topics/`(store = 항목·토픽 저장과 가지 붙이기, plan = 갱신 시각, refresh = 프롬프트, run = 갱신 실행, read = 화면 읽기), 같은 소식 기준: `src/server/similar-title.ts`, 토픽 카드: `src/server/timeline/read.ts`의 `readTopicCards`
- 화면: `src/app/(app)/`(page = 타임라인, status/, stocks/, settings/)
- 설정 값 기본: `src/server/settings.ts`, 설정 화면 검사·저장: `src/server/settings-form.ts`
