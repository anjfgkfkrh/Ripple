import { describe, expect, it } from "vitest";
import { FIXTURE_FILES, fixtureBytes } from "../../../tests/fixtures";
import { parseKindList, parseNasdaqListed, parseOtherListed } from "./parse";

const kindHtml = new TextDecoder("euc-kr").decode(
  fixtureBytes(FIXTURE_FILES.kind),
);

describe("parseKindList (KIND 상장법인목록, 실제 파일 표본)", () => {
  const rows = parseKindList(kindHtml);

  it("EUC-KR 표에서 회사명·종목코드·시장을 읽는다", () => {
    expect(rows).toContainEqual({
      market: "KR",
      code: "005930",
      name: "삼성전자",
      exchange: "KOSPI",
      isEtf: false,
    });
    expect(rows.find((row) => row.name === "SK하이닉스")?.code).toBe("000660");
  });

  it("시장구분을 거래소 이름으로 바꾸고, 영문이 섞인 새 코드도 읽는다", () => {
    expect(new Set(rows.map((row) => row.exchange))).toEqual(
      new Set(["KOSPI", "KOSDAQ", "KONEX"]),
    );
    expect(rows.find((row) => row.name === "엘리스")?.code).toBe("0158S0");
  });

  it("열 이름이 바뀌면 조용히 넘어가지 않고 실패한다", () => {
    expect(() =>
      parseKindList("<table><tr><th>이름</th></tr></table>"),
    ).toThrow("형식이 바뀌었습니다");
  });
});

describe("parseNasdaqListed / parseOtherListed (NASDAQ Trader, 실제 파일 표본)", () => {
  const nasdaq = parseNasdaqListed(
    fixtureBytes(FIXTURE_FILES.nasdaqListed).toString("utf8"),
  );
  const other = parseOtherListed(
    fixtureBytes(FIXTURE_FILES.otherListed).toString("utf8"),
  );

  it("NASDAQ 종목과 ETF 여부를 읽는다", () => {
    expect(nasdaq.find((row) => row.code === "NVDA")).toMatchObject({
      market: "US",
      name: "NVIDIA Corporation - Common Stock",
      exchange: "NASDAQ",
      isEtf: false,
    });
    expect(nasdaq.find((row) => row.code === "QQQ")?.isEtf).toBe(true);
  });

  it("다른 거래소 코드를 이름으로 바꾼다(N=NYSE, P=NYSE Arca)", () => {
    expect(other.find((row) => row.code === "TSM")?.exchange).toBe("NYSE");
    expect(other.find((row) => row.code === "SPY")).toMatchObject({
      exchange: "NYSE Arca",
      isEtf: true,
    });
    expect(other.find((row) => row.code === "BRK.B")).toBeDefined();
  });

  it("시험용 종목과 마지막 파일 생성 시각 줄은 뺀다", () => {
    expect(nasdaq).toHaveLength(5);
    expect(other).toHaveLength(4);
    expect(
      [...nasdaq, ...other].some((row) => row.code.startsWith("FILE")),
    ).toBe(false);
  });

  it("필요한 열이 없으면 실패한다", () => {
    expect(() => parseNasdaqListed("Symbol|Name\nAAA|Foo")).toThrow(
      "형식이 바뀌었습니다",
    );
  });
});
