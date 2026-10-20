// 주소가 응답할 때까지 기다린다(최대 30초). E2E에서 worker가 가짜 외부 서비스보다 먼저 뜨지 않게 한다.
const url = process.argv[2];
const deadline = Date.now() + 30_000;
for (;;) {
  try {
    if ((await fetch(url)).ok) process.exit(0);
  } catch {}
  if (Date.now() > deadline) {
    console.error(`${url} 이(가) 30초 안에 응답하지 않았습니다.`);
    process.exit(1);
  }
  await new Promise((resolve) => setTimeout(resolve, 200));
}
