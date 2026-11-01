//! 曲の DB（LR2 の `song.db`）を登録したフォルダに合わせる（APP-LIB-*）。
//! 規則は OpenLR2 の `LR2_songmanage.cpp`（`LoadLR2CustomFolder` の起動時の確かめ・`GetFolderDataFromPath`・
//! `SearchSongsFromPath`・`ReloadSongsByQuery`・`SetUndefinedDifficulty`）と、書き起こしの元の `LR2f.cpp` の同じ関数に合わせた。
//! ファイルの見張りによる読み直し（[`apply`]）は reprise の拡張（LR2 は起動時と F8 などでしか確かめない）。
use crate::custom_folder;
use crate::ports::{ChartLoader, FileSystem, PortError, SongDb};
use crate::usecase::load_chart;
use domain::chart::{NoteKind, TimingEvent};
use domain::select::{CustomFolder, Library, SongInfo};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

/// フォルダの行（LR2 の `folder` テーブル）
#[derive(Debug, Clone, PartialEq)]
pub struct FolderEntry {
    pub path: PathBuf,
    /// 一覧に出す名前（フォルダ名）
    pub title: String,
    /// 登録したフォルダ（LR2 の `parent` が `ROOT` のもの）
    pub root: bool,
    /// 更新日時（秒）
    pub date: i64,
    /// カスタムフォルダ（`.lr2folder`）なら中身（LR2 の `type` 2 などの行）
    pub custom: Option<CustomFolder>,
    /// `#BANNER`（カスタムフォルダ）
    pub banner: String,
}

/// 譜面の行（LR2 の `song` テーブル）
#[derive(Debug, Clone, PartialEq)]
pub struct SongEntry {
    pub info: SongInfo,
    /// ファイルの更新日時（秒）
    pub date: i64,
    pub subartist: String,
    pub stage_file: String,
    pub banner: String,
    pub back_bmp: String,
    /// BPM の最小と最大（整数。LR2 の `minbpm`・`maxbpm`）
    pub min_bpm: i32,
    pub max_bpm: i32,
    /// `#RANK`（無ければ 2。LR2 の `judge`）
    pub judge: i32,
    pub long_note: bool,
    pub bga: bool,
    /// 譜面のフォルダに `.txt` がある（LR2 の `txt`）
    pub txt: bool,
}

/// 読み直しに使うもの
pub struct Scan<'a> {
    pub fs: &'a dyn FileSystem,
    pub loaders: &'a [Box<dyn ChartLoader>],
    /// 今の時刻（秒。追加日時に書く）
    pub now: i64,
    /// Shift_JIS の文字列を読む（LR2 は `.lr2folder` を Shift_JIS で読む）
    pub decode: fn(&[u8]) -> String,
}

fn under_roots(path: &Path, roots: &[PathBuf]) -> bool {
    roots.iter().any(|r| path.starts_with(r))
}

fn is_dir(fs: &dyn FileSystem, path: &Path) -> bool {
    fs.modified(path).is_some() && !fs.is_file(path)
}

/// APP-LIB-04: 譜面を読んで行にする（乱数の種 0。読めなければ None）
pub fn read_song(scan: &Scan, path: &Path, date: i64) -> Option<SongEntry> {
    let bytes = scan.fs.read(path).ok()?;
    let (c, _) = load_chart(scan.loaders, path, &bytes, 0).ok()?;
    let timing = domain::play::TimingMap::from_chart(&c);
    let notes = domain::play::Lr2Play::new(&c, &timing, domain::play::GaugeType::Groove)
        .map_or(0, |p| p.total_notes());
    // LR2 の `UpdateBpmRange`: 整数にして 0 以下は見ない
    let bpms = c
        .meta
        .initial_bpm
        .into_iter()
        .chain(c.timing.iter().filter_map(|t| match t {
            TimingEvent::Bpm { bpm, .. } => Some(*bpm),
            _ => None,
        }))
        .map(|b| b as i32)
        .filter(|b| *b > 0);
    let (min_bpm, max_bpm) = bpms.fold((0, 0), |(lo, hi), b| {
        if hi == 0 {
            (b, b)
        } else {
            (lo.min(b), hi.max(b))
        }
    });
    let dir = path.parent().unwrap_or(Path::new(""));
    let txt = scan.fs.list(dir).iter().any(|e| {
        e.is_file
            && Path::new(&e.name)
                .extension()
                .is_some_and(|x| x.eq_ignore_ascii_case("txt"))
    });
    Some(SongEntry {
        date,
        subartist: c.meta.subartist.clone(),
        stage_file: c.meta.stage_file.clone(),
        banner: c.meta.banner.clone(),
        back_bmp: c.meta.back_bmp.clone(),
        min_bpm,
        max_bpm,
        judge: c.meta.judge_rank.unwrap_or(2),
        long_note: c
            .notes
            .iter()
            .any(|n| matches!(n.kind, NoteKind::Long { .. })),
        bga: !c.images.is_empty(),
        txt,
        info: SongInfo {
            path: path.to_path_buf(),
            hash: c.hash,
            title: c.meta.title,
            subtitle: c.meta.subtitle,
            artist: c.meta.artist,
            genre: c.meta.genre,
            level: c.meta.level.unwrap_or(0),
            difficulty: c.meta.difficulty.unwrap_or(-1),
            key_mode: c.key_mode,
            notes,
        },
    })
}

/// APP-LIB-08: `.lr2folder` を読んでフォルダの行にする（読めなければ None）
fn read_custom(scan: &Scan, path: &Path, root: bool) -> Option<FolderEntry> {
    let bytes = scan.fs.read(path).ok()?;
    let p = custom_folder::parse(path, &(scan.decode)(&bytes));
    let mut folder = p.folder;
    if root {
        folder.kind = custom_folder::root_kind(&folder.command);
    }
    Some(FolderEntry {
        path: path.to_path_buf(),
        title: folder.title.clone(),
        root,
        date: scan.fs.modified(path).unwrap_or(0),
        custom: Some(folder),
        banner: p.banner,
    })
}

fn is_custom_file(fs: &dyn FileSystem, path: &Path) -> bool {
    fs.is_file(path)
        && path
            .file_name()
            .is_some_and(|n| custom_folder::is_lr2folder(&n.to_string_lossy()))
}

fn name_of(path: &Path) -> String {
    path.file_name()
        .map_or_else(|| path.to_string_lossy(), |n| n.to_string_lossy())
        .into_owned()
}

/// APP-LIB-02: フォルダの中を探す（LR2 `SearchSongsFromPath`）。DB に無い譜面を足し、DB に無いフォルダは足してその中も探す
fn search(scan: &Scan, db: &mut dyn SongDb, dir: &Path) -> Result<bool, PortError> {
    let mut changed = false;
    let mut dirs = vec![dir.to_path_buf()];
    while let Some(d) = dirs.pop() {
        for e in scan.fs.list(&d) {
            if e.is_file && custom_folder::is_lr2folder(&e.name) {
                // LR2 は見つけるたびに INSERT して、あれば失敗する
                if let Some(folder) = read_custom(scan, &e.path, false) {
                    changed |= db.insert_folder(&folder, scan.now)?;
                }
            } else if e.is_file {
                if !scan.loaders.iter().any(|l| l.can_load(&e.path))
                    || db.song_date(&e.path)?.is_some()
                {
                    continue;
                }
                let date = scan.fs.modified(&e.path).unwrap_or(0);
                if let Some(song) = read_song(scan, &e.path, date) {
                    db.put_song(&song, scan.now)?;
                    changed = true;
                }
            } else {
                let folder = FolderEntry {
                    date: scan.fs.modified(&e.path).unwrap_or(0),
                    title: e.name,
                    root: false,
                    path: e.path.clone(),
                    custom: None,
                    banner: String::new(),
                };
                if db.insert_folder(&folder, scan.now)? {
                    changed = true;
                    dirs.push(e.path);
                }
            }
        }
    }
    Ok(changed)
}

/// APP-LIB-03: 行を確かめる（LR2 `ReloadSongsByQuery`。フォルダの行 → 譜面の行）。`pick` で確かめる行を選ぶ
fn check(
    scan: &Scan,
    db: &mut dyn SongDb,
    roots: &[PathBuf],
    pick: &dyn Fn(&Path) -> bool,
) -> Result<bool, PortError> {
    let mut changed = false;
    for f in db.folders()? {
        if !pick(&f.path) {
            continue;
        }
        let custom = f.custom.is_some();
        let now = under_roots(&f.path, roots)
            .then(|| scan.fs.modified(&f.path))
            .flatten()
            .filter(|_| scan.fs.is_file(&f.path) == custom);
        match now {
            None => {
                db.delete_folder(&f.path)?;
                changed = true;
            }
            Some(m) if f.date < m && custom => {
                // カスタムフォルダは読み直して置き換える（LR2 は DELETE して INSERT する）
                db.delete_folder(&f.path)?;
                if let Some(folder) = read_custom(scan, &f.path, f.root) {
                    db.insert_folder(&folder, scan.now)?;
                }
                changed = true;
            }
            Some(m) if f.date < m => {
                db.touch_folder(&f.path, m, scan.now)?;
                search(scan, db, &f.path)?;
                changed = true;
            }
            _ => {}
        }
    }
    for s in db.songs()? {
        let path = &s.info.path;
        if !pick(path) {
            continue;
        }
        let now = under_roots(path, roots)
            .then(|| scan.fs.modified(path))
            .flatten()
            .filter(|_| scan.fs.is_file(path));
        match now {
            None => {
                db.delete_song(path)?;
                changed = true;
            }
            Some(m) if s.date < m => {
                match read_song(scan, path, m) {
                    Some(song) => db.put_song(&song, scan.now)?,
                    None => db.delete_song(path)?,
                }
                changed = true;
            }
            _ => {}
        }
    }
    Ok(changed)
}

/// APP-LIB-01: 登録したフォルダ（LR2 `GetFolderDataFromPath`）。DB に無ければ足して中を探す
fn add_roots(scan: &Scan, db: &mut dyn SongDb, roots: &[PathBuf]) -> Result<bool, PortError> {
    let mut changed = false;
    for r in roots {
        if is_custom_file(scan.fs, r) {
            // 登録したフォルダが .lr2folder なら最初の一覧に出す（中は探さない）
            if let Some(folder) = read_custom(scan, r, true) {
                changed |= db.insert_folder(&folder, scan.now)?;
            }
            continue;
        }
        if !is_dir(scan.fs, r) {
            continue;
        }
        let folder = FolderEntry {
            path: r.clone(),
            title: name_of(r),
            root: true,
            date: scan.fs.modified(r).unwrap_or(0),
            custom: None,
            banner: String::new(),
        };
        if db.insert_folder(&folder, scan.now)? {
            search(scan, db, r)?;
            changed = true;
        }
    }
    Ok(changed)
}

/// APP-LIB-05: `#DIFFICULTY` の無い譜面に難易度を付けて書き込む（LR2 `SetUndefinedDifficulty`。DOM-SEL-17）
pub fn assign_difficulty(db: &mut dyn SongDb) -> Result<(), PortError> {
    let before: Vec<SongInfo> = db.songs()?.into_iter().map(|s| s.info).collect();
    let mut after = before.clone();
    domain::select::filter::assign_undefined_difficulty(&mut after);
    for (b, a) in before.iter().zip(&after) {
        if b.difficulty != a.difficulty {
            db.set_difficulty(&a.path, a.difficulty)?;
        }
    }
    Ok(())
}

/// 起動時の確かめ（LR2 の `autoreload` の既定 2）: 登録していないフォルダの行を消し（APP-LIB-01）、登録したフォルダを足し、
/// すべての行を確かめ（APP-LIB-03）、難易度を付ける（APP-LIB-05）。何か変わったら true
pub fn update(scan: &Scan, db: &mut dyn SongDb, roots: &[PathBuf]) -> Result<bool, PortError> {
    db.begin()?;
    let mut changed = false;
    for f in db.folders()? {
        if f.root && !roots.contains(&f.path) {
            db.delete_folder(&f.path)?;
            changed = true;
        }
    }
    changed |= add_roots(scan, db, roots)?;
    changed |= check(scan, db, roots, &|_| true)?;
    if changed {
        assign_difficulty(db)?;
    }
    db.commit()?;
    Ok(changed)
}

/// DB を空にしてから [`update`] する（作り直し）
pub fn rebuild(scan: &Scan, db: &mut dyn SongDb, roots: &[PathBuf]) -> Result<bool, PortError> {
    db.clear()?;
    update(scan, db, roots)?;
    Ok(true)
}

/// APP-LIB-07: 見張りで知らされたパスを読み直す（reprise の拡張）。パスのフォルダ（ファイルなら親）から上って最初の
/// DB にあるフォルダ（か登録したフォルダ）より下の行を APP-LIB-03 で確かめ、そのフォルダを APP-LIB-02 で探す。
/// 登録したフォルダの外のパスは見ない。何か変わったら true
pub fn apply(
    scan: &Scan,
    db: &mut dyn SongDb,
    roots: &[PathBuf],
    paths: &[PathBuf],
) -> Result<bool, PortError> {
    db.begin()?;
    let mut changed = add_roots(scan, db, roots)?;
    let known: BTreeSet<PathBuf> = db.folders()?.into_iter().map(|f| f.path).collect();
    let mut targets: BTreeSet<PathBuf> = BTreeSet::new();
    for p in paths {
        if !under_roots(p, roots) {
            continue;
        }
        let mut a = if is_dir(scan.fs, p) {
            p.clone()
        } else {
            p.parent().map(Path::to_path_buf).unwrap_or_default()
        };
        while !(roots.contains(&a) || known.contains(&a)) {
            match a.parent() {
                Some(up) if under_roots(up, roots) => a = up.to_path_buf(),
                _ => break,
            }
        }
        if under_roots(&a, roots) {
            targets.insert(a);
        }
    }
    let tops: Vec<PathBuf> = targets
        .iter()
        .filter(|t| !targets.iter().any(|o| o != *t && t.starts_with(o)))
        .cloned()
        .collect();
    for t in &tops {
        changed |= check(scan, db, roots, &|p| p.starts_with(t))?;
    }
    // 探すのは知らされたどのフォルダも（上のフォルダを探しても DB にある子フォルダには入らないので）
    for t in &targets {
        if is_dir(scan.fs, t) {
            changed |= search(scan, db, t)?;
        }
    }
    if changed {
        assign_difficulty(db)?;
    }
    db.commit()?;
    Ok(changed)
}

/// APP-LIB-06: DB から曲一覧を作る（最初の一覧は登録したフォルダの行。カスタムフォルダの行はカスタムフォルダ）
pub fn load(db: &dyn SongDb) -> Result<Library, PortError> {
    let rows = db.folders()?;
    let custom: Vec<CustomFolder> = rows.iter().filter_map(|f| f.custom.clone()).collect();
    let (roots, folders): (Vec<FolderEntry>, Vec<FolderEntry>) =
        rows.into_iter().partition(|f| f.root);
    Ok(Library::new(
        roots.into_iter().map(|f| f.path).collect(),
        folders
            .into_iter()
            .filter(|f| f.custom.is_none())
            .map(|f| f.path)
            .collect(),
        db.songs()?.into_iter().map(|s| s.info).collect(),
    )
    .with_custom(custom))
}

#[cfg(test)]
mod tests;
