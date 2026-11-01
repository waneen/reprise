//! 曲の DB（LR2 の `song.db` と同じテーブル。sqlite）。`SongDb` の実装。
//! テーブルと列は OpenLR2 の `LR2_songmanage.cpp`（`LoadLR2CustomFolder` の `CREATE TABLE`、`SearchSongsFromPath` の `INSERT`）
//! と同じにし、LR2 の `.lr2folder` の SQL（`SELECT * FROM song ... WHERE <条件>`）がそのまま使えるようにする。
//! LR2 に無い SHA-256 は別のテーブル `reprise_song` に置く（LR2 のテーブルの列は変えない）。
//!
//! # 仕様ルール（テストと 1 対 1 対応）
//! | ID | ルール |
//! |---|---|
//! | SNG-01 | 開くときにファイルとフォルダが無ければ作り、LR2 と同じ `folder`・`song`（`exlevel` まで）・`nonstop`・`expert`・`grade` テーブルと索引 `hashidx`・`parentidx`、reprise の `reprise_song` が無ければ作る。開けなければ `Io` エラー |
//! | SNG-02 | フォルダの行: `path` はフォルダのパスの末尾に区切りを付けたもの、`parent` は登録したフォルダなら `ROOT` の、それ以外は親フォルダの CRC（SNG-04）、`type` 1、`title`・`date`・`adddate`。同じパスがあれば足さない。読み戻すと同じ |
//! | SNG-03 | 譜面の行: `hash` は MD5 の 16 進（小文字）、`folder` は譜面のフォルダ・`parent` はその親の CRC、`mode` は鍵盤数（5/7/9/10/14）、`karinotes` は総ノーツ数、`type`・`favorite`・`random`・`exlevel` は 0、`tag` は NULL。SHA-256 は `reprise_song`。同じパスは置き換え、読み戻すと同じ |
//! | SNG-04 | LR2 の `AssignCRC32`: 文字列を Shift_JIS にし、末尾の NUL まで符号付きの char として CRC32（多項式 0xEDB88320）を取り、16 進（小文字・0 埋め無し）にする |
//! | SNG-05 | 消す・日時の書き換え・難易度の書き換え・すべて消す（`folder`・`song`・`reprise_song`）はパスで行に効く |
use application::library::{FolderEntry, SongEntry};
use application::ports::{PortError, SongDb};
use domain::chart::{ChartHash, KeyMode};
use domain::select::SongInfo;
use rusqlite::{params, Connection, OptionalExtension};
use std::path::{Path, PathBuf, MAIN_SEPARATOR};

/// LR2 の `song.db` のテーブル（OpenLR2 `LoadLR2CustomFolder`）と reprise の追加
const CREATE: &str = "\
CREATE TABLE IF NOT EXISTS folder(title TEXT ,subtitle TEXT ,category TEXT,info_a TEXT,info_b TEXT,command TEXT,path TEXT primary key,type INTEGER,banner TEXT,parent TEXT,date INTEGER,max INTEGER,adddate INTEGER);
CREATE TABLE IF NOT EXISTS song(hash TEXT ,title TEXT ,subtitle TEXT ,genre TEXT,artist TEXT,subartist TEXT,tag TEXT ,path TEXT primary key ,type INTEGER,folder TEXT,stagefile TEXT,banner TEXT,backbmp TEXT,parent TEXT,level INTEGER,difficulty INTEGER,maxbpm INTEGER,minbpm INTEGER,mode INTEGER,judge INTEGER,longnote INTEGER,bga INTEGER,random INTEGER,date INTEGER,favorite INTEGER,txt INTEGER,karinotes INTEGER,adddate INTEGER,exlevel INTEGER);
CREATE INDEX IF NOT EXISTS hashidx ON song (hash);
CREATE INDEX IF NOT EXISTS parentidx ON song (parent);
CREATE TABLE IF NOT EXISTS nonstop(id INTEGER primary key,title TEXT,line INTEGER,hash TEXT,ir INTEGER);
CREATE TABLE IF NOT EXISTS expert(id INTEGER primary key,title TEXT,line INTEGER,hash TEXT,ir INTEGER);
CREATE TABLE IF NOT EXISTS grade(id INTEGER primary key,title TEXT,line INTEGER,hash TEXT,ir INTEGER);
CREATE TABLE IF NOT EXISTS reprise_song(path TEXT primary key, sha256 TEXT);";

/// LR2 の `type`（フォルダ）
const TYPE_FOLDER: i64 = 1;

pub struct SqliteSongDb {
    db: Connection,
}

fn io(e: impl std::fmt::Display) -> PortError {
    PortError::Io(e.to_string())
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

fn unhex<const N: usize>(s: &str) -> Option<[u8; N]> {
    if s.len() != N * 2 || !s.is_ascii() {
        return None;
    }
    let mut out = [0u8; N];
    for (i, b) in out.iter_mut().enumerate() {
        *b = u8::from_str_radix(&s[i * 2..i * 2 + 2], 16).ok()?;
    }
    Some(out)
}

/// SNG-04: LR2 の `AssignCRC32`（`CSTR::CRC32`）
pub fn lr2_crc32(s: &str) -> String {
    let (bytes, _, _) = encoding_rs::SHIFT_JIS.encode(s);
    let mut crc: u32 = 0xffff_ffff;
    for &b in bytes.iter().chain(std::iter::once(&0u8)) {
        // char は符号付きなので 0x80 以上は上位ビットが立つ
        crc ^= b as i8 as i32 as u32;
        for _ in 0..8 {
            crc = if crc & 1 == 0 {
                crc >> 1
            } else {
                (crc >> 1) ^ 0xedb8_8320
            };
        }
    }
    format!("{:x}", !crc)
}

/// フォルダのパスの書き方（末尾に区切り。LR2 は `\` を付ける）
pub fn folder_text(p: &Path) -> String {
    let s = p.to_string_lossy().into_owned();
    if s.ends_with('/') || s.ends_with(MAIN_SEPARATOR) {
        s
    } else {
        format!("{s}{MAIN_SEPARATOR}")
    }
}

fn folder_path(s: &str) -> PathBuf {
    let t = s.trim_end_matches(['/', MAIN_SEPARATOR]);
    PathBuf::from(if t.is_empty() { s } else { t })
}

/// フォルダの CRC（登録したフォルダの子は登録したフォルダのパスの CRC）
pub fn folder_crc(p: &Path) -> String {
    lr2_crc32(&folder_text(p))
}

fn mode_number(k: KeyMode) -> i64 {
    match k {
        KeyMode::K5 => 5,
        KeyMode::K7 => 7,
        KeyMode::K9 => 9,
        KeyMode::K10 => 10,
        KeyMode::K14 => 14,
    }
}

fn key_mode(n: i64) -> KeyMode {
    match n {
        5 => KeyMode::K5,
        9 => KeyMode::K9,
        10 => KeyMode::K10,
        14 => KeyMode::K14,
        _ => KeyMode::K7,
    }
}

impl SqliteSongDb {
    /// SNG-01
    pub fn open(path: &Path) -> Result<Self, PortError> {
        let err = |e: &dyn std::fmt::Display| PortError::Io(format!("{}: {e}", path.display()));
        if let Some(dir) = path.parent().filter(|d| !d.as_os_str().is_empty()) {
            std::fs::create_dir_all(dir).map_err(|e| err(&e))?;
        }
        let db = Connection::open(path).map_err(|e| err(&e))?;
        // ほかの接続（サーバーとゲームなど）が書いている間は待つ
        db.busy_timeout(std::time::Duration::from_secs(5))
            .map_err(|e| err(&e))?;
        db.execute_batch(CREATE).map_err(|e| err(&e))?;
        Ok(Self { db })
    }

    /// 接続（カスタムフォルダの SQL などに使う）
    pub fn connection(&self) -> &Connection {
        &self.db
    }
}

const SONG_COLUMNS: &str = "s.hash, s.title, s.subtitle, s.genre, s.artist, s.subartist, s.path, s.stagefile, s.banner, s.backbmp, \
s.level, s.difficulty, s.maxbpm, s.minbpm, s.mode, s.judge, s.longnote, s.bga, s.date, s.txt, s.karinotes, r.sha256";

fn song_row(r: &rusqlite::Row) -> rusqlite::Result<Option<SongEntry>> {
    let text = |i: usize| -> rusqlite::Result<String> {
        Ok(r.get::<_, Option<String>>(i)?.unwrap_or_default())
    };
    let int = |i: usize| -> rusqlite::Result<i64> { Ok(r.get::<_, Option<i64>>(i)?.unwrap_or(0)) };
    let Some(md5) = unhex::<16>(&text(0)?) else {
        return Ok(None);
    };
    Ok(Some(SongEntry {
        info: SongInfo {
            path: PathBuf::from(text(6)?),
            hash: ChartHash {
                md5,
                sha256: r
                    .get::<_, Option<String>>(21)?
                    .as_deref()
                    .and_then(unhex::<32>),
            },
            title: text(1)?,
            subtitle: text(2)?,
            artist: text(4)?,
            genre: text(3)?,
            level: int(10)? as i32,
            difficulty: int(11)? as i32,
            key_mode: key_mode(int(14)?),
            notes: int(20)?.clamp(0, u32::MAX as i64) as u32,
        },
        date: int(18)?,
        subartist: text(5)?,
        stage_file: text(7)?,
        banner: text(8)?,
        back_bmp: text(9)?,
        max_bpm: int(12)? as i32,
        min_bpm: int(13)? as i32,
        judge: int(15)? as i32,
        long_note: int(16)? != 0,
        bga: int(17)? != 0,
        txt: int(19)? != 0,
    }))
}

impl SongDb for SqliteSongDb {
    fn folders(&self) -> Result<Vec<FolderEntry>, PortError> {
        let root = lr2_crc32("ROOT");
        let mut st = self
            .db
            .prepare("SELECT path, title, parent, date FROM folder ORDER BY path")
            .map_err(io)?;
        let rows = st
            .query_map([], |r| {
                Ok(FolderEntry {
                    path: folder_path(&r.get::<_, String>(0)?),
                    title: r.get::<_, Option<String>>(1)?.unwrap_or_default(),
                    root: r.get::<_, Option<String>>(2)?.as_deref() == Some(root.as_str()),
                    date: r.get::<_, Option<i64>>(3)?.unwrap_or(0),
                })
            })
            .map_err(io)?;
        rows.collect::<Result<_, _>>().map_err(io)
    }

    fn songs(&self) -> Result<Vec<SongEntry>, PortError> {
        let mut st = self
            .db
            .prepare(&format!(
                "SELECT {SONG_COLUMNS} FROM song s LEFT JOIN reprise_song r ON s.path = r.path ORDER BY s.path"
            ))
            .map_err(io)?;
        let rows = st.query_map([], song_row).map_err(io)?;
        let mut out = vec![];
        for r in rows {
            if let Some(s) = r.map_err(io)? {
                out.push(s);
            }
        }
        Ok(out)
    }

    fn song_date(&self, path: &Path) -> Result<Option<i64>, PortError> {
        self.db
            .query_row(
                "SELECT date FROM song WHERE path = ?1",
                params![path.to_string_lossy()],
                |r| r.get::<_, Option<i64>>(0),
            )
            .optional()
            .map(|d| d.map(|d| d.unwrap_or(0)))
            .map_err(io)
    }

    fn insert_folder(&mut self, f: &FolderEntry, now: i64) -> Result<bool, PortError> {
        let parent = if f.root {
            lr2_crc32("ROOT")
        } else {
            folder_crc(f.path.parent().unwrap_or(Path::new("")))
        };
        // LR2 は主キーが重なると INSERT に失敗して何もしない
        let n = self
            .db
            .execute(
                "INSERT OR IGNORE INTO folder (path, title, parent, date, type, adddate) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![folder_text(&f.path), f.title, parent, f.date, TYPE_FOLDER, now],
            )
            .map_err(io)?;
        Ok(n > 0)
    }

    fn touch_folder(&mut self, path: &Path, date: i64, now: i64) -> Result<(), PortError> {
        self.db
            .execute(
                "UPDATE folder SET date = ?1, adddate = ?2 WHERE path = ?3",
                params![date, now, folder_text(path)],
            )
            .map(|_| ())
            .map_err(io)
    }

    fn delete_folder(&mut self, path: &Path) -> Result<(), PortError> {
        self.db
            .execute(
                "DELETE FROM folder WHERE path = ?1",
                params![folder_text(path)],
            )
            .map(|_| ())
            .map_err(io)
    }

    fn put_song(&mut self, s: &SongEntry, now: i64) -> Result<(), PortError> {
        let i = &s.info;
        let path = i.path.to_string_lossy();
        let dir = i.path.parent().unwrap_or(Path::new(""));
        let parent = dir.parent().unwrap_or(Path::new(""));
        self.db
            .execute(
                "INSERT OR REPLACE INTO song (hash,title,subtitle,genre,artist,subartist,level,date,path,folder,stagefile,banner,backbmp,parent,\
maxbpm,minbpm,random,longnote,judge,mode,bga,difficulty,favorite,type,txt,karinotes,adddate,exlevel) \
VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,0,?17,?18,?19,?20,?21,0,0,?22,?23,?24,0)",
                params![
                    hex(&i.hash.md5),
                    i.title,
                    i.subtitle,
                    i.genre,
                    i.artist,
                    s.subartist,
                    i.level,
                    s.date,
                    path,
                    folder_crc(dir),
                    s.stage_file,
                    s.banner,
                    s.back_bmp,
                    folder_crc(parent),
                    s.max_bpm,
                    s.min_bpm,
                    s.long_note as i64,
                    s.judge,
                    mode_number(i.key_mode),
                    s.bga as i64,
                    i.difficulty,
                    s.txt as i64,
                    i.notes,
                    now,
                ],
            )
            .map_err(io)?;
        self.db
            .execute(
                "INSERT OR REPLACE INTO reprise_song (path, sha256) VALUES (?1, ?2)",
                params![path, i.hash.sha256.map(|h| hex(&h))],
            )
            .map_err(io)?;
        Ok(())
    }

    fn delete_song(&mut self, path: &Path) -> Result<(), PortError> {
        let p = path.to_string_lossy();
        self.db
            .execute("DELETE FROM song WHERE path = ?1", params![p])
            .map_err(io)?;
        self.db
            .execute("DELETE FROM reprise_song WHERE path = ?1", params![p])
            .map_err(io)?;
        Ok(())
    }

    fn set_difficulty(&mut self, path: &Path, difficulty: i32) -> Result<(), PortError> {
        self.db
            .execute(
                "UPDATE song SET difficulty = ?1 WHERE path = ?2",
                params![difficulty, path.to_string_lossy()],
            )
            .map(|_| ())
            .map_err(io)
    }

    fn clear(&mut self) -> Result<(), PortError> {
        self.db
            .execute_batch("DELETE FROM folder; DELETE FROM song; DELETE FROM reprise_song;")
            .map_err(io)
    }

    fn begin(&mut self) -> Result<(), PortError> {
        // 前の失敗で残った書きかけは捨てる
        if !self.db.is_autocommit() {
            self.db.execute_batch("ROLLBACK").map_err(io)?;
        }
        self.db.execute_batch("BEGIN").map_err(io)
    }

    fn commit(&mut self) -> Result<(), PortError> {
        self.db.execute_batch("COMMIT").map_err(io)
    }
}

#[cfg(test)]
mod tests;
