//! 難易度表（beatoraja の難易度表。reprise の拡張で、LR2 には無い）。
//! 表の読み方（HTML の `bmstable` → ヘッダの JSON → データの JSON）は外側（adapters/bms-table）で行い、ここには結果だけが入る。

/// 表の 1 つの譜面
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TableChart {
    /// MD5（小文字の 16 進。無ければ空）
    pub md5: String,
    /// SHA-256（小文字の 16 進。無ければ空）
    pub sha256: String,
    /// 表に書かれた題名と作者（曲の一覧に無い譜面のバーに出す）
    pub title: String,
    pub artist: String,
}

/// 表のレベル（一覧ではフォルダ）
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TableLevel {
    /// フォルダの名前（beatoraja と同じく 記号（`tag`、無ければ `symbol`）+ レベル）
    pub name: String,
    /// 表の順の譜面
    pub charts: Vec<TableChart>,
}

/// 難易度表
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DifficultyTable {
    /// 設定に書いた URL
    pub url: String,
    /// 表の名前（`name`）
    pub name: String,
    /// レベルの順（`level_order`、無ければデータに出てきた順）
    pub levels: Vec<TableLevel>,
}
