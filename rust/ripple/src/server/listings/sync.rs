// R3에서 목록 받기를 옮긴다. R1(worker 설정)에는 원천 주소만 필요하다.

/// 공식 종목 목록 원천 (D-12). 둘 다 무료이고 키가 필요 없다.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListingSources {
    pub kind: String,
    pub nasdaq_listed: String,
    pub other_listed: String,
}

pub fn listing_sources() -> ListingSources {
    ListingSources {
        kind: "https://kind.krx.co.kr/corpgeneral/corpList.do?method=download&searchType=13"
            .to_string(),
        nasdaq_listed: "https://www.nasdaqtrader.com/dynamic/SymDir/nasdaqlisted.txt".to_string(),
        other_listed: "https://www.nasdaqtrader.com/dynamic/SymDir/otherlisted.txt".to_string(),
    }
}
