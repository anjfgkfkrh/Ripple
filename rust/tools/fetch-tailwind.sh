#!/usr/bin/env bash
# Tailwind standalone CLI 4.3.3을 내려받아 rust/tools/tailwindcss에 둔다(SHA-256 확인, 커밋하지 않음).
set -euo pipefail

VERSION="4.3.3"
DIR="$(cd "$(dirname "$0")" && pwd)"
OUT="$DIR/tailwindcss"

case "$(uname -s)-$(uname -m)" in
  Darwin-arm64) NAME="tailwindcss-macos-arm64"; SHA="cdf646702987a743464dff4d9c60fd4480d1c1e73dd819a9a67f1078815dce9d" ;;
  Darwin-x86_64) NAME="tailwindcss-macos-x64"; SHA="7922e0953f2110c05976e3bf58f14e643d90427575e766b7d433f5f80cbee7e1" ;;
  Linux-aarch64) NAME="tailwindcss-linux-arm64"; SHA="55fd0b241214eff3de1e8ee4f22796662f2d2e7a49bcfca7477cfd0bac398195" ;;
  Linux-x86_64) NAME="tailwindcss-linux-x64"; SHA="dc61b3ac6b8c9ca874c0cc4c57b2409791a64c5540404ca5f5367360babc313a" ;;
  *) echo "지원하지 않는 환경: $(uname -s)-$(uname -m)" >&2; exit 1 ;;
esac

sha256_of() { if command -v sha256sum >/dev/null; then sha256sum "$1" | cut -d' ' -f1; else shasum -a 256 "$1" | cut -d' ' -f1; fi; }

if [ -x "$OUT" ] && [ "$(sha256_of "$OUT")" = "$SHA" ]; then
  echo "tailwindcss $VERSION 이미 있음"
  exit 0
fi

TMP="$OUT.download"
curl -fsSL "https://github.com/tailwindlabs/tailwindcss/releases/download/v$VERSION/$NAME" -o "$TMP"
if [ "$(sha256_of "$TMP")" != "$SHA" ]; then
  rm -f "$TMP"
  echo "SHA-256이 맞지 않습니다: $NAME" >&2
  exit 1
fi
chmod +x "$TMP"
mv "$TMP" "$OUT"
echo "tailwindcss $VERSION 내려받음"
