// 커밋·푸시 전에 .env의 비밀 값과 흔한 키 형식이 git에 들어가지 않았는지 본다.
// 값은 어디에도 출력하지 않고, 찾은 곳(커밋·파일)과 .env의 키 이름만 알린다.
// 사용: node scripts/check-secrets.mjs            스테이징한 변경
//       node scripts/check-secrets.mjs --history  모든 브랜치의 커밋 기록(푸시 전)
// 종료 코드: 0 문제 없음, 1 찾음, 2 실행 오류
import { execFileSync, spawn } from "node:child_process";
import { existsSync, readFileSync } from "node:fs";
import path from "node:path";
import { createInterface } from "node:readline";

// 이보다 짧은 값(포트, true 등)은 흔해서 비밀로 보지 않는다.
const MIN_LENGTH = 8;

const KEY_PATTERNS = [
  ["OpenRouter 키", /sk-or-v1-[0-9a-f]{32,}/],
  ["Anthropic 키", /sk-ant-[A-Za-z0-9_-]{20,}/],
  ["OpenAI 키", /\bsk-(?:proj-)?[A-Za-z0-9]{32,}/],
  [
    "GitHub 토큰",
    /\b(?:gh[pousr]_[A-Za-z0-9]{36,}|github_pat_[A-Za-z0-9_]{40,})/,
  ],
  ["AWS 액세스 키", /\bAKIA[0-9A-Z]{16}\b/],
  ["Slack 토큰", /\bxox[abprs]-[A-Za-z0-9-]{10,}/],
  ["개인 키", /-----BEGIN [A-Z ]*PRIVATE KEY-----/],
];

/** .env의 값 중 비밀로 볼 것. .env.example에 그대로 있는 값(기본값)은 뺀다. */
function envSecrets(envText, exampleText = "") {
  const secrets = [];
  for (const line of envText.split(/\r?\n/)) {
    const m = line.match(
      /^\s*(?:export\s+)?([A-Za-z_][A-Za-z0-9_]*)\s*=\s*(.*)$/,
    );
    if (!m) continue;
    let value = m[2].trim();
    const quoted = /^(["']).*\1$/.test(value);
    value = quoted ? value.slice(1, -1) : value.replace(/\s+#.*$/, "");
    if (value.length < MIN_LENGTH || exampleText.includes(value)) continue;
    secrets.push({ key: m[1], value });
  }
  return secrets;
}

const isEnvFile = (file) => path.basename(file) === ".env";

function git(args) {
  return execFileSync("git", args, {
    encoding: "utf8",
    maxBuffer: 64 * 1024 * 1024,
  });
}

/** git 출력(diff 형식)을 줄마다 읽어 찾은 곳을 모은다. */
async function scanDiff(args, secrets, linePrefixes) {
  const found = new Map();
  const child = spawn("git", args, { stdio: ["ignore", "pipe", "inherit"] });
  let commit = "";
  let file = "";
  const note = (what) => {
    const where = commit ? `커밋 ${commit} ${file}` : file;
    found.set(`${where}\u0000${what}`, { where, what });
  };
  for await (const line of createInterface({
    input: child.stdout,
    crlfDelay: Infinity,
  })) {
    if (line.startsWith("commit:")) {
      commit = line.slice("commit:".length);
      continue;
    }
    if (line.startsWith("diff --git ")) {
      file = line.replace(/^diff --git a\/.* b\//, "");
      if (isEnvFile(file)) note("`.env` 파일 자체");
      continue;
    }
    if (line.startsWith("+++ ") || line.startsWith("--- ")) continue;
    if (!linePrefixes.includes(line[0])) continue;
    const text = line.slice(1);
    for (const { key, value } of secrets)
      if (text.includes(value)) note(`.env의 ${key} 값`);
    for (const [name, re] of KEY_PATTERNS)
      if (re.test(text)) note(`키 형식(${name})`);
  }
  const code = await new Promise((resolve) => child.on("close", resolve));
  if (code !== 0) throw new Error(`git ${args[0]} 실패(종료 코드 ${code})`);
  return [...found.values()];
}

async function main() {
  const history = process.argv.includes("--history");
  const root = git(["rev-parse", "--show-toplevel"]).trim();
  const read = (name) => {
    const file = path.join(root, name);
    return existsSync(file) ? readFileSync(file, "utf8") : null;
  };
  const env = read(".env");
  const secrets =
    env === null ? [] : envSecrets(env, read(".env.example") ?? "");

  const found = history
    ? await scanDiff(
        [
          "log",
          "-p",
          "--all",
          "--no-color",
          "--no-ext-diff",
          "--format=commit:%h",
        ],
        secrets,
        ["+", "-"],
      )
    : await scanDiff(
        ["diff", "--cached", "-U0", "--no-color", "--no-ext-diff"],
        secrets,
        ["+"],
      );
  if (!history) {
    for (const file of git(["diff", "--cached", "--name-only", "-z"]).split(
      "\0",
    ))
      if (file && isEnvFile(file) && !found.some((f) => f.where === file))
        found.push({ where: file, what: "`.env` 파일 자체" });
  }

  const scope = history ? "모든 커밋 기록" : "스테이징한 변경";
  const envNote = env === null ? ".env 없음" : `.env 값 ${secrets.length}개`;
  console.log(
    `비밀 값 검사(${scope}): ${envNote}, 키 형식 ${KEY_PATTERNS.length}개`,
  );
  if (found.length === 0) {
    console.log("문제 없음");
    return 0;
  }
  console.log(`찾음 ${found.length}곳(값은 출력하지 않음):`);
  for (const { where, what } of found) console.log(`- ${where}: ${what}`);
  return 1;
}

main().then(
  (code) => process.exit(code),
  (error) => {
    console.error(`비밀 값 검사 실패: ${error.message}`);
    process.exit(2);
  },
);
