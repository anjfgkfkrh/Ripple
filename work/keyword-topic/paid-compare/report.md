# paid-compare 보고
> 사실만 쓴다.

## 멈춘 이유 (해당하는 보고 조건)
- 브리프 "알려진 함정": Docker가 꺼져 있다. 체크포인트 1·2의 실행(시험 compose)을 시작하지 못했다. 이미지는 새로 빌드하지 않는다는 제약이라 멈춤.
- `.check-restore/ripple.db`는 있다(58355712 바이트, 11월 10일 21:13).

## 재현 방법
1. `docker ps`

## 오류 출력
```
failed to connect to the docker API at unix:///Users/blockall/.docker/run/docker.sock; check if the path is correct and if the daemon is running: dial unix /Users/blockall/.docker/run/docker.sock: connect: no such file or directory
```
(`docker images`도 같은 오류. `ripple-app` 이미지 유무는 확인하지 못함.)

## 시도한 것
- brief.md, README, 파일 목록 읽기만 했다. 요청 전송·유료 호출·파일 복사는 하지 않았다. 비용 $0.

## 마지막 커밋과 현재 상태
- 마지막 커밋: 49f9649(내가 만든 커밋 없음). 끝낸 체크포인트: 없음. 작업 트리 변경: 기존 `work/metrics.csv`(추적 안 됨)뿐.
