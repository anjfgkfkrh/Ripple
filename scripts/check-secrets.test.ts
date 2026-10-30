import { execFileSync, spawnSync } from "node:child_process";
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import path from "node:path";
import { afterEach, describe, expect, it } from "vitest";

const SCRIPT = path.resolve("scripts/check-secrets.mjs");
// 표본 값은 실행할 때 만든다(이 저장소의 기록 검사에 걸리지 않게).
const SECRET = ["fake", "secret", "value", "1234"].join("-");
const GITHUB_TOKEN = "gh" + "p_" + "a1B2".repeat(9);

const dirs: string[] = [];
afterEach(() => {
  for (const dir of dirs.splice(0))
    rmSync(dir, { recursive: true, force: true });
});

function git(dir: string, ...args: string[]): string {
  return execFileSync(
    "git",
    [
      "-c",
      "user.name=test",
      "-c",
      "user.email=test@example.com",
      "-c",
      "commit.gpgsign=false",
      ...args,
    ],
    { cwd: dir, encoding: "utf8" },
  );
}

/** .env(무시됨)와 .env.example을 둔 새 저장소. */
function repo(env: string, example = ""): string {
  const dir = mkdtempSync(path.join(tmpdir(), "ripple-secrets-"));
  dirs.push(dir);
  git(dir, "init", "-q");
  writeFileSync(path.join(dir, ".env"), env);
  writeFileSync(path.join(dir, ".env.example"), example);
  writeFileSync(path.join(dir, ".gitignore"), ".env\n");
  git(dir, "add", ".gitignore", ".env.example");
  git(dir, "commit", "-q", "-m", "init");
  return dir;
}

function stage(dir: string, file: string, text: string): void {
  mkdirSync(path.dirname(path.join(dir, file)), { recursive: true });
  writeFileSync(path.join(dir, file), text);
  git(dir, "add", "-f", file);
}

function check(
  dir: string,
  ...args: string[]
): { code: number | null; out: string } {
  const result = spawnSync(process.execPath, [SCRIPT, ...args], {
    cwd: dir,
    encoding: "utf8",
  });
  return { code: result.status, out: result.stdout + result.stderr };
}

describe("check-secrets", () => {
  it("스테이징한 변경에 .env 값이 있으면 파일과 키 이름만 알리고 실패한다", () => {
    const dir = repo(`API_KEY=${SECRET}\n`);
    stage(dir, "src/a.ts", `const key = "${SECRET}";\n`);
    const result = check(dir);
    expect(result.code).toBe(1);
    expect(result.out).toContain("src/a.ts: .env의 API_KEY 값");
    expect(result.out).not.toContain(SECRET);
  });

  it("따옴표로 감싼 .env 값도 찾는다", () => {
    const dir = repo(`export SESSION_SECRET="${SECRET}"\n`);
    stage(dir, "notes.md", `${SECRET}\n`);
    const result = check(dir);
    expect(result.code).toBe(1);
    expect(result.out).toContain("SESSION_SECRET");
  });

  it("흔한 키 형식은 .env에 없어도 찾는다", () => {
    const dir = repo("");
    stage(dir, "a.txt", `token ${GITHUB_TOKEN}\n`);
    const result = check(dir);
    expect(result.code).toBe(1);
    expect(result.out).toContain("a.txt: 키 형식(GitHub 토큰)");
    expect(result.out).not.toContain(GITHUB_TOKEN);
  });

  it(".env 파일을 스테이징하면 실패한다", () => {
    const dir = repo(`API_KEY=${SECRET}\n`);
    git(dir, "add", "-f", ".env");
    const result = check(dir);
    expect(result.code).toBe(1);
    expect(result.out).toContain("`.env` 파일 자체");
    expect(result.out).not.toContain(SECRET);
  });

  it("값을 쓰지 않은 변경은 통과한다", () => {
    const dir = repo(`API_KEY=${SECRET}\n`);
    stage(dir, "src/a.ts", "const key = process.env.API_KEY;\n");
    const result = check(dir);
    expect(result.code).toBe(0);
    expect(result.out).toContain("문제 없음");
  });

  it("짧은 값과 .env.example에 그대로 있는 값은 비밀로 보지 않는다", () => {
    const dir = repo("TZ=Asia/Seoul\nWORKER_PORT=4000\n", "TZ=Asia/Seoul\n");
    stage(dir, "a.txt", "Asia/Seoul 4000\n");
    expect(check(dir).code).toBe(0);
  });

  it("--history는 지운 뒤에도 커밋 기록에 남은 값을 찾는다", () => {
    const dir = repo(`API_KEY=${SECRET}\n`);
    stage(dir, "a.txt", `${SECRET}\n`);
    git(dir, "commit", "-q", "-m", "leak");
    stage(dir, "a.txt", "clean\n");
    git(dir, "commit", "-q", "-m", "fix");
    expect(check(dir).code).toBe(0);
    const result = check(dir, "--history");
    expect(result.code).toBe(1);
    expect(result.out).toMatch(/커밋 [0-9a-f]+ a\.txt: \.env의 API_KEY 값/);
    expect(result.out).not.toContain(SECRET);
  });
});
