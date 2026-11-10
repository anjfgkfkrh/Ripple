use std::collections::{HashMap, HashSet};

use rusqlite::{Connection, params};

use crate::server::db::schema::ItemKind;
use crate::server::keywords::performance::keyword_performance;
use crate::server::keywords::tree::{
    AddedKeyword, AddedParent, ExistingKeyword, KeywordStock, LEVEL1_MAX, LEVEL2_MAX,
    RawKeywordUpdate, UPDATE_ADD_MAX, UPDATE_ANCHORS_ADD_MAX, accept_added_keywords, normalize_key,
    refine_anchors,
};
use crate::server::stocks::aliases::validate_aliases;
use crate::server::stocks::repository::{find_stock, set_stock_terms};
use crate::server::time::{DAY_MS, EpochMs};

/// 뺄 키워드를 지키는 최근 통과 기간(17단계 결정: 4주)
pub const PROTECT_DAYS: i64 = 28;

/// 정기 점검이 바꾼 것(로그용)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct UpdateSummary {
    /// 뺀 키워드(답이 뺀 것, 그 아래, 깊이 3 이상)
    pub removed: usize,
    /// 뺄 것이었지만 최근 4주 통과가 있어 남긴 키워드
    pub protected: usize,
    /// 더한 키워드
    pub added: usize,
    /// 상한(1단계 6·2단계 12)을 넘어 밀어낸 키워드(그 아래 포함)
    pub evicted: usize,
    pub anchors_removed: usize,
    pub anchors_added: usize,
    pub aliases_added: usize,
    /// 점검 뒤 구조 키워드 수와 가장 깊은 단계
    pub count: usize,
    pub deepest: i64,
    pub anchors: usize,
    pub aliases: usize,
    /// 답의 `add` 가운데 규칙에 맞지 않아 버린 수
    pub dropped: usize,
}

/// 저장된 구조 키워드(토픽 가지 제외) 한 줄
#[derive(Debug, Clone)]
pub struct TreeRow {
    pub id: i64,
    pub level: i64,
    pub parent_id: Option<i64>,
    pub text_ko: String,
    pub path: String,
    pub created_at: EpochMs,
}

/// 종목의 구조 키워드(토픽 가지 빼고), 만든 순서대로
pub fn load_structure(conn: &Connection, stock_id: i64) -> rusqlite::Result<Vec<TreeRow>> {
    let mut stmt = conn.prepare(
        "SELECT id, level, parent_id, text_ko, path, created_at FROM keywords \
         WHERE stock_id = ?1 AND topic_id IS NULL ORDER BY id ASC",
    )?;
    stmt.query_map([stock_id], |row| {
        Ok(TreeRow {
            id: row.get(0)?,
            level: row.get(1)?,
            parent_id: row.get(2)?,
            text_ko: row.get(3)?,
            path: row.get(4)?,
            created_at: row.get(5)?,
        })
    })?
    .collect()
}

/// 프롬프트에 넣는 지금 트리: 1단계와 그 아래 2단계 글자(3단계 이상은 넣지 않는다)
pub fn tree_for_prompt(rows: &[TreeRow]) -> Vec<(String, Vec<String>)> {
    rows.iter()
        .filter(|r| r.level == 1)
        .map(|l1| {
            let kids = rows
                .iter()
                .filter(|r| r.level == 2 && r.parent_id == Some(l1.id))
                .map(|r| r.text_ko.clone())
                .collect();
            (l1.text_ko.clone(), kids)
        })
        .collect()
}

/// 상한을 따질 때 쓰는 칸 하나(이미 있는 키워드 또는 이번에 더하는 키워드)
struct Slot {
    level: i64,
    parent: Option<usize>,
    /// 마지막 통과 시각, 없으면 만든 시각(작을수록 먼저 밀려난다)
    recency: EpochMs,
    gone: bool,
}

/// 아래 칸까지 함께 지운다. 새로 지운 칸 수를 돌려준다.
fn cascade(slots: &mut [Slot]) -> usize {
    let mut newly = 0;
    loop {
        let mut changed = false;
        for i in 0..slots.len() {
            if !slots[i].gone && slots[i].parent.is_some_and(|p| slots[p].gone) {
                slots[i].gone = true;
                newly += 1;
                changed = true;
            }
        }
        if !changed {
            return newly;
        }
    }
}

/// 점검 답을 한 트랜잭션으로 반영한다(17단계 결정).
/// 1. 뺄 것: 정규화한 글자로 찾고, 최근 4주 안에 통과가 있으면 남긴다. 1단계를 빼면 그 아래도 빠지고, 3단계 이상은 모두 뺀다.
/// 2. 더할 것: 최대 3개, 전체 생성과 같은 검증. 2단계의 부모는 남은 1단계나 이번에 더하는 1단계다.
/// 3. 상한: 1단계 6·2단계 12. 넘으면 마지막 통과 시각(없으면 만든 시각)이 가장 오래된 것부터 뺀다(그 아래 포함).
/// 4. 종목: 고유어 빼기 다음 더하기(최대 3개, 전체 생성과 같은 다듬기). 결과가 비면 빼기를 하지 않는다. 별칭은 더할 것만 검증한다.
/// 5. 바뀌지 않은 키워드 줄은 그대로 두고, 상태 'ready'·`keywords_generated_at`을 지금으로 한다.
pub fn apply_update(
    conn: &mut Connection,
    target: &KeywordStock,
    stock_id: i64,
    update: &RawKeywordUpdate,
    now: EpochMs,
) -> rusqlite::Result<UpdateSummary> {
    let tx = conn.transaction()?;
    let Some(stock) = find_stock(&tx, stock_id)? else {
        return Err(rusqlite::Error::QueryReturnedNoRows);
    };
    let rows = load_structure(&tx, stock_id)?;
    let recent = keyword_performance(&tx, stock_id, now - PROTECT_DAYS * DAY_MS)?;
    let overall = keyword_performance(&tx, stock_id, 0)?;
    let mut summary = UpdateSummary::default();

    // 1. 뺄 것(지금 있는 칸)
    let id_to_slot: HashMap<i64, usize> = rows.iter().enumerate().map(|(i, r)| (r.id, i)).collect();
    let mut slots: Vec<Slot> = rows
        .iter()
        .map(|row| Slot {
            level: row.level,
            parent: row.parent_id.and_then(|p| id_to_slot.get(&p).copied()),
            recency: overall
                .get(&normalize_key(&row.text_ko))
                .and_then(|p| p.last_passed_at)
                .unwrap_or(row.created_at),
            // 깊이는 2단계까지다. 옛 트리의 3단계 이상은 뺀다.
            gone: row.level >= 3,
        })
        .collect();
    summary.removed += slots.iter().filter(|s| s.gone).count();
    for (keyword, _reason) in &update.remove {
        let key = normalize_key(keyword);
        for (i, row) in rows.iter().enumerate() {
            if slots[i].gone || normalize_key(&row.text_ko) != key {
                continue;
            }
            if recent.get(&key).is_some_and(|p| p.passed > 0) {
                summary.protected += 1;
            } else {
                slots[i].gone = true;
                summary.removed += 1;
            }
        }
    }
    summary.removed += cascade(&mut slots);

    // 2. 더할 것(남은 키워드가 부모 후보)
    let alive: Vec<usize> = (0..rows.len()).filter(|i| !slots[*i].gone).collect();
    let existing: Vec<ExistingKeyword> = alive
        .iter()
        .map(|i| ExistingKeyword {
            level: rows[*i].level,
            text_ko: rows[*i].text_ko.clone(),
            path: rows[*i].path.clone(),
        })
        .collect();
    let added = accept_added_keywords(target, &existing, &update.add, UPDATE_ADD_MAX);
    summary.dropped = update.add.len().saturating_sub(added.len());
    let existing_count = rows.len();
    for keyword in &added {
        slots.push(Slot {
            level: keyword.level,
            parent: match keyword.parent {
                AddedParent::Root => None,
                AddedParent::Existing(i) => Some(alive[i]),
                AddedParent::Added(j) => Some(existing_count + j),
            },
            recency: now,
            gone: false,
        });
    }

    // 3. 상한: 오래된 것부터 밀어낸다(1단계를 밀어내면 그 아래도 빠진다)
    for (level, max) in [(1, LEVEL1_MAX), (2, LEVEL2_MAX)] {
        loop {
            let at_level: Vec<usize> = (0..slots.len())
                .filter(|i| !slots[*i].gone && slots[*i].level == level)
                .collect();
            if at_level.len() <= max {
                break;
            }
            let oldest = at_level
                .iter()
                .copied()
                .min_by_key(|i| (slots[*i].recency, *i))
                .expect("비어 있지 않다");
            slots[oldest].gone = true;
            summary.evicted += 1 + cascade(&mut slots);
        }
    }

    // 4. 고유어·별칭(종목만)
    let mut new_terms: Option<(Vec<String>, Vec<String>)> = None;
    if stock.kind == ItemKind::Stock {
        let valid_adds = refine_anchors(&update.anchors_add);
        let removing: HashSet<String> = update
            .anchors_remove
            .iter()
            .map(|a| normalize_key(a))
            .collect();
        // 이미 있는 고유어와 겹치는 것은 더하는 수(최대 3개)에 세지 않는다.
        let build = |skip_removed: bool| -> Vec<String> {
            let mut candidates: Vec<String> = stock
                .anchors
                .iter()
                .filter(|a| !skip_removed || !removing.contains(&normalize_key(a)))
                .cloned()
                .collect();
            let kept: HashSet<String> = candidates.iter().map(|a| normalize_key(a)).collect();
            candidates.extend(
                valid_adds
                    .iter()
                    .filter(|a| !kept.contains(&normalize_key(a)))
                    .take(UPDATE_ANCHORS_ADD_MAX)
                    .cloned(),
            );
            refine_anchors(&candidates)
        };
        let mut anchors = build(true);
        if anchors.is_empty() {
            anchors = build(false);
        }
        let before: HashSet<String> = stock.anchors.iter().map(|a| normalize_key(a)).collect();
        let after: HashSet<String> = anchors.iter().map(|a| normalize_key(a)).collect();
        summary.anchors_removed = before.difference(&after).count();
        summary.anchors_added = after.difference(&before).count();

        let mut aliases = stock.aliases.clone();
        let mut known: HashSet<String> = aliases.iter().map(|a| normalize_key(a)).collect();
        for alias in validate_aliases(&tx, &stock, &update.aliases_add)? {
            if known.insert(normalize_key(&alias)) {
                aliases.push(alias);
                summary.aliases_added += 1;
            }
        }
        summary.anchors = anchors.len();
        summary.aliases = aliases.len();
        if anchors != stock.anchors || aliases != stock.aliases {
            new_terms = Some((aliases, anchors));
        }
    }

    // 5. 저장: 지운 줄을 지우고 더한 줄을 넣는다. 바뀌지 않은 줄은 건드리지 않는다.
    for (i, row) in rows.iter().enumerate() {
        if slots[i].gone {
            tx.execute("DELETE FROM keywords WHERE id = ?1", params![row.id])?;
        }
    }
    let mut inserted: HashMap<usize, i64> = HashMap::new();
    for (j, keyword) in added.iter().enumerate() {
        let slot = existing_count + j;
        if slots[slot].gone {
            continue;
        }
        let parent_id = slots[slot].parent.map(|p| {
            if p < existing_count {
                rows[p].id
            } else {
                inserted[&p]
            }
        });
        insert_added(&tx, stock_id, parent_id, keyword, now)?;
        inserted.insert(slot, tx.last_insert_rowid());
        summary.added += 1;
    }
    if let Some((aliases, anchors)) = &new_terms {
        set_stock_terms(&tx, stock_id, aliases, anchors)?;
    }
    tx.execute(
        "UPDATE stocks SET keywords_status = 'ready', keywords_error = NULL, keywords_generated_at = ?1 WHERE id = ?2",
        params![now, stock_id],
    )?;
    summary.count = slots.iter().filter(|s| !s.gone).count();
    summary.deepest = slots
        .iter()
        .filter(|s| !s.gone)
        .map(|s| s.level)
        .max()
        .unwrap_or(0);
    tx.commit()?;
    Ok(summary)
}

fn insert_added(
    conn: &Connection,
    stock_id: i64,
    parent_id: Option<i64>,
    keyword: &AddedKeyword,
    at: EpochMs,
) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT INTO keywords (stock_id, level, parent_id, text_ko, text_en, reason, path, kind, created_at) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 'structure', ?8)",
        params![
            stock_id,
            keyword.level,
            parent_id,
            keyword.text_ko,
            keyword.text_en,
            keyword.reason,
            keyword.path,
            at
        ],
    )?;
    Ok(())
}
