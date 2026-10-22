#!/usr/bin/env bash
# 릴리스 묶음 만들기: Docker 이미지를 arm64·amd64 두 종류로 빌드해 압축 파일로 저장한다.
# 사용: scripts/release.sh            (package.json의 version을 버전으로 씀)
#       ARCHS="arm64" scripts/release.sh   (한 종류만 다시 만들 때)
# 결과: release-dist/ (git에서 제외). 이 안의 파일을 GitHub 릴리스 페이지에 직접 올린다.
# 비밀 값은 이미지에 들어가지 않는다(.dockerignore). 이 스크립트가 만든 이미지 안을 한 번 더 검사하고,
# 값은 절대 출력하지 않는다.
set -euo pipefail

cd "$(dirname "$0")/.."
VERSION="$(node -p "require('./package.json').version")"
ARCHS="${ARCHS:-arm64 amd64}"
OUT="release-dist"
mkdir -p "$OUT"

echo "== Ripple v$VERSION 릴리스 묶음 ($ARCHS)"

# 검사할 비밀 값: .env에서 키 이름이 API_KEY·SECRET·PASSWORD·TOKEN인 값(8자 이상). 값은 출력하지 않는다.
SECRETS_FILE="$(mktemp)"
trap 'rm -f "$SECRETS_FILE"' EXIT
if [ -f .env ]; then
  while IFS='=' read -r key value; do
    case "$key" in
      *API_KEY|*SECRET|*PASSWORD|*TOKEN)
        value="${value%\"}"; value="${value#\"}"; value="${value%\'}"; value="${value#\'}"
        [ "${#value}" -ge 8 ] && printf '%s\n' "$value" >>"$SECRETS_FILE"
        ;;
    esac
  done < <(grep -E '^[A-Z_]+=' .env || true)
fi
echo "검사할 비밀 값 $(wc -l <"$SECRETS_FILE" | tr -d ' ')개(값은 출력하지 않음)"

for arch in $ARCHS; do
  echo
  echo "── linux/$arch 빌드"
  for target in worker web; do
    docker buildx build --platform "linux/$arch" --target "$target" --load \
      -t "ripple-$target:$VERSION" .
  done

  echo "── linux/$arch 검사"
  for target in worker web; do
    image="ripple-$target:$VERSION"
    got="$(docker image inspect --format '{{.Architecture}}' "$image")"
    [ "$got" = "$arch" ] || { echo "종류가 다릅니다: $image = $got (기대 $arch)"; exit 1; }
    # 1) 이미지 설정의 환경변수에 키·비밀번호가 없어야 한다(이름만 본다).
    if docker image inspect --format '{{range .Config.Env}}{{println .}}{{end}}' "$image" \
        | cut -d= -f1 | grep -Eq 'API_KEY|SECRET|PASSWORD|TOKEN'; then
      echo "이미지 환경변수에 비밀 값 이름이 있습니다: $image"; exit 1
    fi
    # 2) 이미지 파일 어디에도 .env 파일이 없어야 한다(.env.example은 이미지에 없음).
    found="$(docker run --rm --platform "linux/$arch" --entrypoint sh "$image" -c \
      "find / -xdev \\( -name '.env' -o -name '.env.*' -o -name '*.db' \\) -not -path '/proc/*' 2>/dev/null | head -5")"
    if [ -n "$found" ]; then echo "이미지에 .env나 DB 파일이 있습니다($image):"; echo "$found"; exit 1; fi
    # 3) 이미지 내용 전체에서 내 .env의 비밀 값이 없어야 한다.
    if [ -s "$SECRETS_FILE" ]; then
      if docker save "$image" | LC_ALL=C grep -aqFf "$SECRETS_FILE"; then
        echo "이미지 안에서 .env의 비밀 값이 발견되었습니다: $image"; exit 1
      fi
    fi
    echo "  $image: 종류 $got, 환경변수·.env·DB·비밀 값 없음"
  done

  # 4) 이 종류에서 마이그레이션이 실제로 끝까지 도는지(네이티브 모듈 포함) 빈 볼륨으로 확인한다.
  vol="ripple-release-check-$arch-$$"
  docker volume create "$vol" >/dev/null
  # 실패하면(set -e, pipefail) 여기서 멈춘다. 빈 볼륨은 끝나거나 실패해도 지운다.
  trap 'docker volume rm "$vol" >/dev/null 2>&1 || true; rm -f "$SECRETS_FILE"' EXIT
  docker run --rm --platform "linux/$arch" -v "$vol:/data" \
    -e DATABASE_PATH=/data/ripple.db "ripple-worker:$VERSION" \
    node dist/worker/migrate.mjs 2>&1 | tail -2
  echo "  마이그레이션 실행 확인(linux/$arch)"
  docker volume rm "$vol" >/dev/null

  file="$OUT/ripple-$VERSION-images-$arch.tar.gz"
  echo "── $file 만드는 중"
  docker save "ripple-web:$VERSION" "ripple-worker:$VERSION" | gzip -9 >"$file"
  echo "  $(du -h "$file" | cut -f1)"
done

echo
echo "── 설치용 파일"
VERSION="$VERSION" python3 -I - <<'PY'
import os, re
v = os.environ["VERSION"]
src = open("docker-compose.yml", encoding="utf-8").read()
# build 블록(context + target)을 미리 만든 이미지 이름으로 바꾼다.
out = re.sub(
    r"    build:\n      context: \.\n      target: (\w+)\n",
    lambda m: f"    image: ripple-{m.group(1)}:{v}\n",
    src,
)
if "build:" in out.replace("# ", ""):
    raise SystemExit("build 블록이 남았습니다. docker-compose.yml 형식을 확인하세요.")
# 맨 위 개발용 안내 주석을 설치용 안내로 바꾼다.
out = re.sub(r"\A(#[^\n]*\n)+", "# Ripple 설치용 compose(미리 만든 이미지 사용). 설치 방법은 INSTALL.md\n", out)
open("release-dist/docker-compose.yml", "w", encoding="utf-8").write(out)
text = open("release/INSTALL.md", encoding="utf-8").read().replace("__VERSION__", v)
open("release-dist/INSTALL.md", "w", encoding="utf-8").write(text)
PY
# GitHub 릴리스는 점으로 시작하는 파일 이름의 점을 지우므로 env.example로 둔다.
cp .env.example "$OUT/env.example"

(cd "$OUT" && shasum -a 256 ripple-"$VERSION"-images-*.tar.gz docker-compose.yml env.example INSTALL.md >SHA256SUMS)
echo
echo "== 끝. release-dist/:"
ls -lh "$OUT"
