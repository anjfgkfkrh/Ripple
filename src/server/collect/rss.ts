/**
 * RSS 2.0·Atom 피드를 읽는다. 라이브러리 없이 필요한 태그만 뽑는다
 * (2026-10-20 수집 대상 피드 56개를 모두 읽는 것을 확인).
 */
export type FeedItem = {
  title: string;
  link: string;
  /** HTML을 걷어 낸 요약(자르기는 저장할 때 한다) */
  summary: string;
  /** 피드에 없거나 읽을 수 없으면 null */
  publishedAt: Date | null;
};

const NAMED_ENTITIES: Record<string, string> = {
  amp: "&",
  lt: "<",
  gt: ">",
  quot: '"',
  apos: "'",
  nbsp: " ",
  middot: "·",
  hellip: "…",
  lsquo: "‘",
  rsquo: "’",
  ldquo: "“",
  rdquo: "”",
  ndash: "–",
  mdash: "—",
  bull: "•",
  copy: "©",
};

export function parseFeed(xml: string, now: Date = new Date()): FeedItem[] {
  const items = blocks(xml, "item");
  const atom = items.length === 0;
  const entries = atom ? blocks(xml, "entry") : items;
  const out: FeedItem[] = [];
  for (const block of entries) {
    const title = toText(tag(block, "title"));
    const link = atom
      ? atomLink(block)
      : toText(tag(block, "link")) || guidLink(block);
    if (!title || !link) continue;
    out.push({
      title,
      link,
      summary: toText(
        tag(block, "description") ??
          tag(block, "summary") ??
          tag(block, "content"),
      ),
      publishedAt: parseFeedDate(
        tag(block, "pubDate") ??
          tag(block, "dc:date") ??
          tag(block, "published") ??
          tag(block, "updated"),
        now,
      ),
    });
  }
  return out;
}

function blocks(xml: string, name: string): string[] {
  const re = new RegExp(`<${name}\\b[^>]*>([\\s\\S]*?)</${name}>`, "gi");
  return [...xml.matchAll(re)].map((m) => m[1]);
}

function tag(block: string, name: string): string | null {
  const m = block.match(
    new RegExp(`<${name}\\b[^>]*>([\\s\\S]*?)</${name}>`, "i"),
  );
  return m ? m[1] : null;
}

function atomLink(block: string): string {
  const links = [...block.matchAll(/<link\b([^>]*)\/?>/gi)].map((m) => m[1]);
  const pick =
    links.find((attrs) => /rel=["']alternate["']/i.test(attrs)) ??
    links.find((attrs) => !/rel=/i.test(attrs)) ??
    links[0];
  return pick
    ? decodeEntities(pick.match(/href=["']([^"']+)["']/i)?.[1] ?? "").trim()
    : "";
}

function guidLink(block: string): string {
  const guid = toText(tag(block, "guid"));
  return /^https?:\/\//i.test(guid) ? guid : "";
}

/** CDATA를 풀고, 엔티티를 풀고, HTML 태그를 걷어 낸 글자 */
export function toText(raw: string | null): string {
  if (!raw) return "";
  let text = raw.replace(/<!\[CDATA\[([\s\S]*?)\]\]>/g, "$1");
  text = decodeEntities(text);
  text = text.replace(/<[^>]*>/g, " ");
  text = decodeEntities(text);
  return text.replace(/\s+/g, " ").trim();
}

export function decodeEntities(text: string): string {
  return text.replace(
    /&(#x[0-9a-f]+|#\d+|[a-z]+);/gi,
    (whole, body: string) => {
      if (body[0] === "#") {
        const code =
          body[1] === "x" || body[1] === "X"
            ? Number.parseInt(body.slice(2), 16)
            : Number.parseInt(body.slice(1), 10);
        return Number.isFinite(code) && code > 0 && code <= 0x10ffff
          ? String.fromCodePoint(code)
          : whole;
      }
      return NAMED_ENTITIES[body.toLowerCase()] ?? whole;
    },
  );
}

const DAY_MS = 24 * 60 * 60 * 1000;

/**
 * 피드 날짜를 읽는다. 표준(RFC 822·ISO 8601) 외에
 * 월이 숫자인 형식("Tue, 20 10 2026 17:36:24 +0900", 노컷뉴스)과
 * 시간대 없는 형식("2026-10-20 17:36", 한국 시간으로 본다)도 받는다.
 * 2000년 이전이나 하루 넘게 미래인 값은 버린다.
 */
export function parseFeedDate(
  raw: string | null,
  now: Date = new Date(),
): Date | null {
  const text = toText(raw);
  if (!text) return null;
  // V8은 "0001"년을 2001년으로 읽는다. 글자에 적힌 연도가 2000년 전이면 바로 버린다.
  const writtenYear = text.match(/(?<!\d)(\d{4})(?!\d)/)?.[1];
  if (writtenYear && Number(writtenYear) < 2000) return null;
  let date: Date | null = null;

  const numericMonth = text.match(
    /^[A-Za-z]{3},\s*(\d{1,2})\s+(\d{1,2})\s+(\d{4})\s+(\d{2}):(\d{2})(?::(\d{2}))?\s*([+-]\d{4})?$/,
  );
  const naive = text.match(
    /^(\d{4})[-./](\d{2})[-./](\d{2})[ T](\d{2}):(\d{2})(?::(\d{2}))?(?:\.\d+)?$/,
  );
  if (numericMonth) {
    const [, d, mo, y, h, mi, s = "00", zone = "+0900"] = numericMonth;
    date = new Date(
      `${y}-${mo.padStart(2, "0")}-${d.padStart(2, "0")}T${h}:${mi}:${s}${zone.slice(0, 3)}:${zone.slice(3)}`,
    );
  } else if (naive) {
    const [, y, mo, d, h, mi, s = "00"] = naive;
    date = new Date(`${y}-${mo}-${d}T${h}:${mi}:${s}+09:00`);
  } else {
    date = new Date(text);
  }
  if (Number.isNaN(date.getTime())) return null;
  if (date.getUTCFullYear() < 2000 || date.getTime() > now.getTime() + DAY_MS) {
    return null;
  }
  return date;
}
