//! 難易度表（beatoraja の難易度表。reprise の拡張で、LR2 には無い）。表の取り方と手元への保存は `TableSource`。
//! いつ取るかは beatoraja の `TableDataAccessor`（`loadNewTableData`・`updateTableData`）と同じにした（APP-TBL-*）。
use crate::ports::TableSource;
use domain::select::DifficultyTable;

/// APP-TBL-01: 手元に保存した表を設定の順に（無いものは飛ばす。URL は設定に書いたもの）
pub fn cached(src: &dyn TableSource, urls: &[String]) -> Vec<DifficultyTable> {
    urls.iter()
        .filter_map(|u| {
            let mut t = src.cached(u)?;
            t.url = u.clone();
            Some(t)
        })
        .collect()
}

/// APP-TBL-02: 手元に無い表だけ取ってくる（beatoraja `loadNewTableData`。起動時）。取れたものがあれば true
pub fn fetch_missing(src: &dyn TableSource, urls: &[String]) -> bool {
    let mut got = false;
    for u in urls {
        if src.cached(u).is_none() {
            got |= src.fetch(u).is_ok();
        }
    }
    got
}

/// APP-TBL-03: すべての表を取り直す（beatoraja `updateTableData`）。取れなかった表は手元のまま。取れたものがあれば true
pub fn fetch_all(src: &dyn TableSource, urls: &[String]) -> bool {
    let mut got = false;
    for u in urls {
        got |= src.fetch(u).is_ok();
    }
    got
}

#[cfg(test)]
mod tests;
