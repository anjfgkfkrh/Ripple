import type { HttpClient } from "./http";

/** robots.txt에서 우리를 가리키는 이름(User-agent 줄과 대소문자 무시 비교) */
export const ROBOTS_AGENT = "ripple";

type Rule = { allow: boolean; pattern: string };

/**
 * RFC 9309 방식으로 우리에게 적용되는 규칙을 고른다.
 * 'ripple'을 가리키는 그룹이 있으면 그것을, 없으면 '*' 그룹을 쓴다.
 */
export function parseRobots(
  text: string,
  agent: string = ROBOTS_AGENT,
): Rule[] {
  type Group = { agents: string[]; rules: Rule[] };
  const groups: Group[] = [];
  let current: Group | null = null;
  let lastWasAgent = false;
  for (const rawLine of text.split(/\r?\n/)) {
    const line = rawLine.replace(/#.*$/, "").trim();
    const m = line.match(/^([A-Za-z-]+)\s*:\s*(.*)$/);
    if (!m) continue;
    const key = m[1].toLowerCase();
    const value = m[2].trim();
    if (key === "user-agent") {
      if (!current || !lastWasAgent) {
        current = { agents: [], rules: [] };
        groups.push(current);
      }
      current.agents.push(value.toLowerCase());
      lastWasAgent = true;
    } else if ((key === "allow" || key === "disallow") && current) {
      current.rules.push({ allow: key === "allow", pattern: value });
      lastWasAgent = false;
    } else {
      lastWasAgent = false;
    }
  }
  const mine = groups.filter((g) => g.agents.includes(agent.toLowerCase()));
  const chosen =
    mine.length > 0 ? mine : groups.filter((g) => g.agents.includes("*"));
  return chosen.flatMap((g) => g.rules);
}

/** 가장 길게 맞는 규칙을 따른다. 길이가 같으면 허용. 맞는 규칙이 없으면 허용. */
export function isAllowed(rules: Rule[], pathWithQuery: string): boolean {
  let best: { length: number; allow: boolean } | null = null;
  for (const rule of rules) {
    if (rule.pattern === "") continue;
    if (!patternMatches(rule.pattern, pathWithQuery)) continue;
    const length = rule.pattern.length;
    if (
      !best ||
      length > best.length ||
      (length === best.length && rule.allow)
    ) {
      best = { length, allow: rule.allow };
    }
  }
  return best ? best.allow : true;
}

function patternMatches(pattern: string, path: string): boolean {
  const anchored = pattern.endsWith("$");
  const body = anchored ? pattern.slice(0, -1) : pattern;
  const source = body
    .split("*")
    .map((part) => part.replace(/[.+?^${}()|[\]\\]/g, "\\$&"))
    .join(".*");
  return new RegExp(`^${source}${anchored ? "$" : ""}`).test(path);
}

export type RobotsChecker = {
  /** 이 주소를 가져가도 되는지. robots.txt를 확인하지 못하면 이번에는 가져가지 않는다. */
  allowed(url: string): Promise<boolean>;
};

/** 사이트마다 robots.txt를 하루 동안 기억한다. 없으면(4xx) 모두 허용, 서버 오류·연결 실패면 막힌 것으로 본다(RFC 9309). */
export function createRobotsChecker(
  http: HttpClient,
  options: { ttlMs?: number; now?: () => number } = {},
): RobotsChecker {
  const { ttlMs = 24 * 60 * 60 * 1000, now = Date.now } = options;
  const cache = new Map<string, { rules: Rule[] | null; at: number }>();

  async function rulesFor(origin: string): Promise<Rule[] | null> {
    const hit = cache.get(origin);
    if (hit && now() - hit.at < ttlMs) return hit.rules;
    let rules: Rule[] | null;
    try {
      const response = await http.get(`${origin}/robots.txt`);
      if (response.status >= 200 && response.status < 300) {
        rules = parseRobots(response.text);
      } else if (response.status >= 400 && response.status < 500) {
        rules = [];
      } else {
        rules = null;
      }
    } catch {
      rules = null;
    }
    // 확인하지 못한 경우는 기억하지 않고 다음 회차에 다시 본다.
    if (rules) cache.set(origin, { rules, at: now() });
    return rules;
  }

  return {
    async allowed(url) {
      const parsed = new URL(url);
      const rules = await rulesFor(parsed.origin);
      if (!rules) return false;
      return isAllowed(rules, parsed.pathname + parsed.search);
    },
  };
}
