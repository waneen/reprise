//! 選曲の並べ替え・絞り込み（LR2 の `config.select.sort`・`key`・`difficulty`。DOM-SEL-12..16）。
//! OpenLR2 の `LR2_songmanage.cpp`（`LoadFilteredBmsListFromDB`・`SetUndefinedDifficulty`・`CMP_SongDataBy*`・
//! `adjustFilterKey`）・`Scene02_Songselect.cpp`（`InitSelectBySearchResult`・→ と SELECT の難易度の切り替え）を仕様として参照した。
use super::{upper, SongInfo};
use crate::chart::{ChartHash, KeyMode};
use std::cmp::Ordering;
use std::path::Path;

/// 並べ替えの最大（0 ディレクトリ・1 レベル・2 題名・3 クリア・4 スコア）
pub const SORT_MAX: i32 = 4;
/// 鍵盤数の絞り込みの最大（0 すべて・1 SINGLE・2 7 鍵・3 5 鍵・4 DOUBLE・5 14 鍵・6 10 鍵・7 9 鍵）
pub const KEY_MAX: i32 = 7;
/// 難易度の絞り込みの最大（0 すべて・1 BEGINNER … 5 INSANE）
pub const DIFFICULTY_MAX: i32 = 5;

/// LR2 の `LoopInRange`
fn loop_in_range(min: i32, max: i32, add: i32, v: &mut i32) {
    *v += add;
    if max < *v {
        *v = min;
    } else if *v < min {
        *v = max;
    }
}

/// 並べ替えと絞り込み（LR2 の既定は 並べ替え 0・鍵盤数 1・難易度 0）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ListFilter {
    pub sort: i32,
    pub key: i32,
    pub difficulty: i32,
}

impl Default for ListFilter {
    fn default() -> Self {
        Self {
            sort: 0,
            key: 1,
            difficulty: 0,
        }
    }
}

/// どれを切り替えるか（LR2 の BUTTON 10・11・12）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FilterKind {
    /// BUTTON 10
    Difficulty,
    /// BUTTON 11（パネル 1 の鍵盤 1 も）
    Key,
    /// BUTTON 12
    Sort,
}

impl ListFilter {
    /// DOM-SEL-13: 1 つ先へ回す（LR2 `InitSelectBySearchResult` の `LoopInRange(.., 1, ..)`）
    pub fn next(&mut self, kind: FilterKind) {
        match kind {
            FilterKind::Difficulty => loop_in_range(0, DIFFICULTY_MAX, 1, &mut self.difficulty),
            FilterKind::Key => loop_in_range(0, KEY_MAX, 1, &mut self.key),
            FilterKind::Sort => loop_in_range(0, SORT_MAX, 1, &mut self.sort),
        }
    }

    /// 範囲の外の値（設定ファイルの書き損じ）は既定に戻す
    pub fn clamped(self) -> Self {
        let d = Self::default();
        let ok = |v: i32, max: i32, def: i32| if (0..=max).contains(&v) { v } else { def };
        Self {
            sort: ok(self.sort, SORT_MAX, d.sort),
            key: ok(self.key, KEY_MAX, d.key),
            difficulty: ok(self.difficulty, DIFFICULTY_MAX, d.difficulty),
        }
    }
}

/// 並べ替えに使う自分の記録（LR2 の `mybest` の `clear`（0 NO PLAY … 5 FULL COMBO）と EX スコア）
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SortRecord {
    pub clear: i32,
    pub ex_score: u32,
}

/// DOM-SEL-12: 鍵盤数の絞り込みを通るか（LR2 の条件: 1 は 8 鍵以下、4 は 10 鍵以上、0 はすべて）
pub fn key_passes(key: i32, km: KeyMode) -> bool {
    use KeyMode::*;
    match key {
        1 => matches!(km, K5 | K7),
        2 => km == K7,
        3 => km == K5,
        4 => matches!(km, K10 | K14),
        5 => km == K14,
        6 => km == K10,
        7 => km == K9,
        _ => true,
    }
}

/// 難易度の絞り込みの単位（譜面のフォルダと鍵盤数が同じ譜面）の中での譜面の情報
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Group {
    /// 同じ難易度の中で何番目か（LR2 の `difficultyCount`。難易度 1..5 でなければ 0）
    pub count: i32,
    /// 難易度 1..5 の譜面の数（LR2 の `difficultyExist`）
    pub exist: [i32; 5],
}

fn dir_of(s: &SongInfo) -> String {
    upper(s.path.parent().unwrap_or(Path::new("")))
}

/// DOM-SEL-17: `#DIFFICULTY` の無い譜面（0..=5 でないもの）に難易度を付ける（LR2 `SetUndefinedDifficulty`）。
/// 譜面のフォルダ → 鍵盤数 → ノーツ数（同じならパス）の順に見て、前の譜面と同じフォルダ・鍵盤数なら 前の難易度 + 1
/// （5 になれば 4、負なら 2、5 を超えれば 5）、違えば 2
pub fn assign_undefined_difficulty(songs: &mut [SongInfo]) {
    let mut order: Vec<usize> = (0..songs.len()).collect();
    order.sort_by_cached_key(|&i| {
        let s = &songs[i];
        (dir_of(s), s.key_mode, s.notes, upper(&s.path))
    });
    let (mut folder, mut mode, mut diff) = (None::<String>, None::<KeyMode>, 0);
    for i in order {
        let s = &mut songs[i];
        let f = dir_of(s);
        if !(0..=5).contains(&s.difficulty) {
            diff = if folder.as_deref() == Some(f.as_str()) && mode == Some(s.key_mode) {
                match diff + 1 {
                    5 => 4,
                    d if d < 0 => 2,
                    d if d > 5 => 5,
                    d => d,
                }
            } else {
                2
            };
            s.difficulty = diff;
        } else {
            diff = s.difficulty;
        }
        folder = Some(f);
        mode = Some(s.key_mode);
    }
}

/// 絞り込んで並べた譜面（`Library::songs` の添字と単位の中での情報）
pub(super) type Listed = Vec<(usize, Group)>;

/// DOM-SEL-12, 14, 15: 開いたフォルダの譜面を絞り込んで並べる（LR2 `LoadFilteredBmsListFromDB`）。
/// 絞り込みで 1 つも残らなければ鍵盤数を、選んだ難易度の譜面が 1 つも無ければ難易度を先へ回して `filter` を書き換える
pub(super) fn list_songs(
    songs: &[SongInfo],
    ids: &[usize],
    filter: &mut ListFilter,
    record: &dyn Fn(&ChartHash) -> SortRecord,
) -> Listed {
    // 問い合わせの順（フォルダ → 鍵盤数 → 難易度 → レベル。同じならパス）
    let mut rows = ids.to_vec();
    rows.sort_by_cached_key(|&i| {
        let s = &songs[i];
        (dir_of(s), s.key_mode, s.difficulty, s.level, upper(&s.path))
    });
    loop {
        let passing: Vec<usize> = rows
            .iter()
            .copied()
            .filter(|&i| key_passes(filter.key, songs[i].key_mode))
            .collect();
        if passing.is_empty() {
            // DOM-SEL-12: 鍵盤数を先へ（0 = すべては必ず通る）
            let start = filter.key;
            let mut m = start;
            loop {
                m = (m + 1) % (KEY_MAX + 1);
                if m == start {
                    return vec![];
                }
                if rows.iter().any(|&i| key_passes(m, songs[i].key_mode)) {
                    break;
                }
            }
            filter.key = m;
            continue;
        }
        // DOM-SEL-14: 選んだ難易度が無ければ先の難易度へ（無ければすべて）
        let mut count = [0; 6];
        for &i in &passing {
            let d = songs[i].difficulty;
            count[if (1..=5).contains(&d) { d as usize } else { 0 }] += 1;
        }
        let d = filter.difficulty;
        if d > 0 && count[d as usize] == 0 {
            filter.difficulty = (d + 1..=DIFFICULTY_MAX)
                .find(|&n| count[n as usize] > 0)
                .unwrap_or(0);
            continue;
        }
        let mut out = by_difficulty(songs, &passing, d);
        sort(songs, &mut out, filter.sort, record);
        return out;
    }
}

/// DOM-SEL-14: 難易度の絞り込み（LR2 の 1 行ずつの処理）。単位の中で、選んだ難易度以下で最も高い難易度の譜面（無ければ最も低い難易度の譜面）を残す。
/// 0 はすべて残す。同じ MD5 は残っている譜面と同じなら飛ばす
fn by_difficulty(songs: &[SongInfo], rows: &[usize], filter: i32) -> Listed {
    let mut out: Listed = vec![];
    // 単位の始まり（out の添字）と単位の情報
    let mut group_start = 0;
    let mut exist = [0; 5];
    let mut last: Option<(String, KeyMode)> = None;
    let (mut last_diff, mut same) = (0, 0usize);
    let close = |out: &mut Listed, from: usize, exist: [i32; 5]| {
        for e in &mut out[from..] {
            e.1.exist = exist;
        }
    };
    for &i in rows {
        let s = &songs[i];
        if out.iter().any(|(j, _)| songs[*j].hash == s.hash) {
            continue;
        }
        let key = (dir_of(s), s.key_mode);
        let new_group = last.as_ref() != Some(&key);
        if new_group {
            close(&mut out, group_start, exist);
            group_start = out.len();
            exist = [0; 5];
        }
        let mut g = Group::default();
        if (1..=5).contains(&s.difficulty) {
            g.count = exist[s.difficulty as usize - 1];
            exist[s.difficulty as usize - 1] += 1;
        }
        let d = s.difficulty;
        if new_group {
            last = Some(key);
            (last_diff, same) = (d, 1);
            out.push((i, g));
        } else if filter != 0 && last_diff < d && d <= filter {
            out.truncate(out.len() - same);
            (last_diff, same) = (d, 1);
            out.push((i, g));
        } else if last_diff == d || filter == 0 {
            same += 1;
            out.push((i, g));
        }
    }
    close(&mut out, group_start, exist);
    out
}

/// DOM-SEL-15: 並べ替え（LR2 の `qsort` は安定でないので、同順位はパス順にする）
fn sort(
    songs: &[SongInfo],
    out: &mut Listed,
    kind: i32,
    record: &dyn Fn(&ChartHash) -> SortRecord,
) {
    // 記録は 1 曲につき 1 回だけ引く
    let recs: std::collections::HashMap<usize, SortRecord> = if matches!(kind, 3 | 4) {
        out.iter()
            .map(|(i, _)| (*i, record(&songs[*i].hash)))
            .collect()
    } else {
        Default::default()
    };
    let rec = |i: usize| recs.get(&i).copied().unwrap_or_default();
    let title = |s: &SongInfo| s.title.to_ascii_uppercase();
    // CMP_SongDataByTitle: 題名（大文字）→ #DIFFICULTY 5 は後ろ → レベル
    let by_title = |a: &SongInfo, b: &SongInfo| {
        title(a)
            .cmp(&title(b))
            .then((a.difficulty == 5).cmp(&(b.difficulty == 5)))
            .then(a.level.cmp(&b.level))
    };
    // CMP_SongDataByScore: EX スコア × 10000 / 総ノーツ × 2（整数）
    let rate = |i: usize| {
        let n = songs[i].notes as i64;
        if n > 0 {
            rec(i).ex_score as i64 * 10000 / n * 2
        } else {
            0
        }
    };
    let directory = |s: &SongInfo| {
        let insane = if s.difficulty == 5 { 100 } else { 0 };
        (dir_of(s), s.key_mode, s.level + insane)
    };
    let cmp = |x: usize, y: usize| -> Ordering {
        let (a, b) = (&songs[x], &songs[y]);
        match kind {
            // CMP_SongDataByDifficulty: 鍵盤数 → #DIFFICULTY 5 は後ろ → レベル → 題名
            1 => a
                .key_mode
                .cmp(&b.key_mode)
                .then((a.difficulty == 5).cmp(&(b.difficulty == 5)))
                .then(a.level.cmp(&b.level))
                .then_with(|| by_title(a, b)),
            2 => by_title(a, b),
            // CMP_SongDataByClear: ランプ → 題名
            3 => rec(x).clear.cmp(&rec(y).clear).then_with(|| by_title(a, b)),
            4 => rate(x).cmp(&rate(y)),
            // CMP_SongDataByDirectory: 譜面のフォルダ → 鍵盤数 → レベル（#DIFFICULTY 5 は +100）
            _ => directory(a).cmp(&directory(b)),
        }
    };
    out.sort_by(|x, y| {
        cmp(x.0, y.0).then_with(|| upper(&songs[x.0].path).cmp(&upper(&songs[y.0].path)))
    });
}

/// DOM-SEL-16: → か SELECT で曲の難易度を替える（LR2 の `difficultyChangeType` 0）。
/// 単位の中に今の難易度より上の難易度があればその最も低いもの、無ければ 今がすべて（0）なら単位の最も低い難易度、それ以外はすべて（0）
pub fn next_difficulty(current_filter: i32, difficulty: i32, group: &Group) -> i32 {
    if (0..5).contains(&difficulty) {
        if let Some(d) = (difficulty + 1..=5).find(|&d| group.exist[d as usize - 1] > 0) {
            return d;
        }
    }
    let min = (1..=5)
        .find(|&d| group.exist[d as usize - 1] > 0)
        .unwrap_or(1);
    if current_filter == 0 {
        min
    } else {
        0
    }
}
