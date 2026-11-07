import os, re
v = os.environ["VERSION"]
src = open("docker-compose.yml", encoding="utf-8").read()
# build 블록(context + target)을 미리 만든 이미지 이름으로 바꾼다.
out = re.sub(
    r"    build:\n      context: \.\n      target: ripple\n    image: ripple-app\n",
    f"    image: ripple-app:{v}\n",
    src,
)
if "build:" in out.replace("# ", ""):
    raise SystemExit("build 블록이 남았습니다. docker-compose.yml 형식을 확인하세요.")
# 맨 위 개발용 안내 주석을 설치용 안내로 바꾼다.
out = re.sub(r"\A(#[^\n]*\n)+", "# Ripple 설치용 compose(미리 만든 이미지 사용). 설치 방법은 INSTALL.md\n", out)
open("work/커스텀도메인/tunnel-docs/dist-compose.yml", "w", encoding="utf-8").write(out)
