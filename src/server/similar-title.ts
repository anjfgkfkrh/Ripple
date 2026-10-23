/**
 * 같은 소식을 다른 언론사가 낸 기사인지 제목으로 본다(11단계, 2026-10-22 실측으로 정함).
 * 머리말([속보] 등)·기호를 뺀 제목을 두 글자씩 잘라, '…' 앞부분의 겹침 비율이 0.5 이상이거나
 * 제목 전체가 0.4 이상이면 같은 소식이다. 같은 항목·24시간 안에서만 비교한다.
 * 평가 점수 복사(같은 소식은 한 번만 Jev에 묻는다)와 타임라인 묶기에 같이 쓴다.
 */

/** 같은 소식으로 볼 수 있는 발행 시각 차이 */
export const SAME_NEWS_WINDOW_MS = 24 * 60 * 60 * 1000;
const LEAD_THRESHOLD = 0.5;
const ALL_THRESHOLD = 0.4;

/** 제목 앞의 [속보]·(종합)·【단독】 같은 머리말 */
const PREFIX = /^\s*([[【(][^\]】)]{1,12}[\]】)]\s*)+/;
/** 핵심 내용과 덧붙인 내용을 가르는 표시 */
const CLAUSE_BREAK = /…|⋯|\.\.\.|···| - | \| |—/;

export type TitleFeatures = { all: Set<string>; lead: Set<string> };

export function titleFeatures(title: string): TitleFeatures {
  const body = title.replace(PREFIX, "");
  return {
    all: bigrams(normalize(body)),
    lead: bigrams(normalize(body.split(CLAUSE_BREAK)[0] ?? "")),
  };
}

export function isSameNews(a: TitleFeatures, b: TitleFeatures): boolean {
  return (
    jaccard(a.lead, b.lead) >= LEAD_THRESHOLD ||
    jaccard(a.all, b.all) >= ALL_THRESHOLD
  );
}

function normalize(text: string): string {
  return text.replace(/[^0-9A-Za-z가-힣]/g, "").toLowerCase();
}

function bigrams(text: string): Set<string> {
  const grams = new Set<string>();
  for (let i = 0; i < text.length - 1; i++) grams.add(text.slice(i, i + 2));
  return grams;
}

function jaccard(a: Set<string>, b: Set<string>): number {
  if (a.size === 0 || b.size === 0) return 0;
  let shared = 0;
  for (const gram of a) if (b.has(gram)) shared += 1;
  return shared / (a.size + b.size - shared);
}
