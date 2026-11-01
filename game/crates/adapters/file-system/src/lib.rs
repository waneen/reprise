//! `FileSystem` の実装。ネイティブはディスク（`StdFs`）、ブラウザは選ばれたファイルを置いたメモリ（`MemFs`）。
//!
//! # 仕様ルール（テストと 1 対 1 対応）
//! | ID | ルール |
//! |---|---|
//! | FS-STD-01 | `StdFs`: 一覧はフォルダの中身（名前・パス・ファイルか）。フォルダでなければ空。読めないファイルは `Io` エラー |
//! | FS-MEM-01 | `MemFs`: パスは `.` を除き `..` を 1 つ上として正規化して比べる（`\` も区切り）。置いたファイルだけが読めて、それ以外は `Io` エラー |
//! | FS-MEM-02 | `MemFs`: 一覧は直下のファイルと、置いたファイルから分かるフォルダ（名前の重複なし、名前順） |
use application::ports::{DirEntry, FileSystem, PortError};
use std::collections::BTreeMap;
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;

/// ディスク（FS-STD-01）
pub struct StdFs;

impl FileSystem for StdFs {
    fn list(&self, dir: &Path) -> Vec<DirEntry> {
        std::fs::read_dir(dir)
            .into_iter()
            .flatten()
            .flatten()
            .map(|e| DirEntry {
                name: e.file_name().to_string_lossy().into_owned(),
                path: e.path(),
                is_file: e.file_type().map(|t| t.is_file()).unwrap_or(false),
            })
            .collect()
    }
    fn read(&self, path: &Path) -> Result<Vec<u8>, PortError> {
        std::fs::read(path).map_err(|e| PortError::Io(format!("{}: {e}", path.display())))
    }
    fn is_file(&self, path: &Path) -> bool {
        path.is_file()
    }
}

/// メモリ上のファイル（FS-MEM-01, 02）。パスは `/` 始まりで置く
#[derive(Default, Clone)]
pub struct MemFs {
    files: BTreeMap<PathBuf, Arc<[u8]>>,
}

/// `.` を除き、`..` を 1 つ上として正規化する（`\` も区切りとみなす）
fn normalize(p: &Path) -> PathBuf {
    let s = p.to_string_lossy().replace('\\', "/");
    let mut out = PathBuf::from("/");
    for c in Path::new(&s).components() {
        match c {
            Component::Normal(n) => out.push(n),
            Component::ParentDir => {
                out.pop();
            }
            _ => {}
        }
    }
    out
}

impl MemFs {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn insert(&mut self, path: impl AsRef<Path>, bytes: impl Into<Arc<[u8]>>) {
        self.files.insert(normalize(path.as_ref()), bytes.into());
    }
    /// 置いたファイルのパス（名前順）
    pub fn paths(&self) -> impl Iterator<Item = &Path> {
        self.files.keys().map(PathBuf::as_path)
    }
}

impl FileSystem for MemFs {
    fn list(&self, dir: &Path) -> Vec<DirEntry> {
        let dir = normalize(dir);
        let mut out: BTreeMap<String, DirEntry> = BTreeMap::new();
        for p in self.files.keys() {
            let Ok(rest) = p.strip_prefix(&dir) else {
                continue;
            };
            let mut comps = rest.components();
            let Some(Component::Normal(first)) = comps.next() else {
                continue;
            };
            let name = first.to_string_lossy().into_owned();
            let is_file = comps.next().is_none();
            out.entry(name.clone()).or_insert(DirEntry {
                path: dir.join(&name),
                name,
                is_file,
            });
        }
        out.into_values().collect()
    }
    fn read(&self, path: &Path) -> Result<Vec<u8>, PortError> {
        self.files
            .get(&normalize(path))
            .map(|b| b.to_vec())
            .ok_or_else(|| PortError::Io(format!("{}: 見つからない", path.display())))
    }
    fn is_file(&self, path: &Path) -> bool {
        self.files.contains_key(&normalize(path))
    }
}

#[cfg(test)]
mod tests;
