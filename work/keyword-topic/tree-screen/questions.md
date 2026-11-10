# tree-screen 질문
> 사실만 묻는다. 판단은 묻지 않는다. 하위는 "답"에 사실만 쓴다.

## 질문 (리뷰)
### rust/ripple/src/web.rs
1. 언제 바꿨나? (diff상 커밋 0a204ae, `.route("/stocks/{id}/aliases/remove", post(stocks::remove_alias))` 한 줄)
2. 무엇 때문에 바꿨나? 이 한 줄 말고 바꾼 것이 있나?
3. 바꾸지 않으면 무엇이 실패하나?
4. 인계 "범위 밖: 없음"에 이 파일을 적지 않은 까닭은?

## 답 (하위)
### rust/ripple/src/web.rs
1. 2026-11-10 22:27(+0900) 시도 1의 커밋 0a204ae에서 바꿨다. 한 줄 추가(`.route("/stocks/{id}/aliases/remove", post(stocks::remove_alias))`)가 이 파일의 변경 전부다(`git show 0a204ae -- rust/ripple/src/web.rs`로 확인).
2. 브리프의 별칭 지우기 경로 `POST /stocks/{id}/aliases/remove`를 web 라우터에 등록하려고 바꿨다. 이 파일에서 바꾼 것은 이 한 줄뿐이다.
3. 이 줄이 없으면 라우터에 경로가 없어 그 POST가 처리되지 않는다(`web/stocks_tests.rs`의 지우기 시험과 E2E 별칭 지우기가 실패한다). 줄 없이 시험을 돌려 확인하지는 않았다.
4. 브리프 "수정 가능 범위"는 `rust/ripple/src/web/`(폴더)를 적었고 `rust/ripple/src/web.rs`는 따로 적혀 있지 않다. 시도 1 때 라우터가 이 파일에 있다는 것을 범위 안으로 보고 `- 범위 밖:` 줄을 쓰지 않았고, 인계에도 적지 않았다. 이 파일이 폴더 범위 밖임을 시도 3에서 브리프와 대조해 확인했으며, 인계를 고치지는 않았다(범위 밖 기록 누락). 
