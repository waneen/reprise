//! 曲の DB とフォルダの見張り（ネイティブ版）。`LibrarySource` の実装。
//! 起動時に DB の曲一覧をすぐ返し、別のスレッドで LR2 の起動時の確かめ（`application::library::update`）をしてから
//! 登録したフォルダを見張り（`notify`）、変わったパスを読み直して（`apply`）新しい曲一覧を届ける。
//! 見張りは reprise の拡張（LR2 は起動時と F8 などでしか確かめない）。
//!
//! # 仕様ルール（テストと 1 対 1 対応）
//! | ID | ルール |
//! |---|---|
//! | LWT-01 | 始めると DB の曲一覧をすぐ返し、その後の起動時の確かめで何か変われば新しい曲一覧を届ける |
//! | LWT-02 | 見張り: 登録したフォルダの中で譜面が足された・消えたら、読み直した曲一覧を届ける |
//! | LWT-03 | 知らせのまとめ方: 最後の知らせから 300ms 何も来ないか、最初の知らせから 2 秒たったら、それまでのパスを重ねずにまとめて読み直す |
//! | LWT-04 | 作り直し（`rebuild`）: DB を空にして確かめ直し、曲一覧を届ける |
use application::library::{self, Scan};
use application::ports::{ChartLoader, FileSystem, LibrarySource, PortError};
use domain::select::Library;
use notify::{RecursiveMode, Watcher};
use std::collections::BTreeSet;
use std::path::PathBuf;
use std::sync::mpsc::{channel, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// 最後の知らせからこれだけ来なければ読み直す
pub const QUIET: Duration = Duration::from_millis(300);
/// 最初の知らせからこれだけたったら読み直す
pub const MAX_WAIT: Duration = Duration::from_secs(2);

/// LWT-03: 知らせをまとめる
#[derive(Debug, Default)]
pub struct Debounce {
    paths: BTreeSet<PathBuf>,
    first: Option<Instant>,
    last: Option<Instant>,
}

impl Debounce {
    pub fn push(&mut self, now: Instant, paths: impl IntoIterator<Item = PathBuf>) {
        self.paths.extend(paths);
        self.first.get_or_insert(now);
        self.last = Some(now);
    }
    /// 読み直す時が来ていれば、まとめたパスを返して空にする
    pub fn take(&mut self, now: Instant) -> Option<Vec<PathBuf>> {
        let (first, last) = (self.first?, self.last?);
        if now.duration_since(last) < QUIET && now.duration_since(first) < MAX_WAIT {
            return None;
        }
        let paths = std::mem::take(&mut self.paths);
        *self = Self::default();
        Some(paths.into_iter().collect())
    }
    /// 次に [`Debounce::take`] を試すまでの待ち（何も無ければ None）
    pub fn wait(&self, now: Instant) -> Option<Duration> {
        let (first, last) = (self.first?, self.last?);
        let a = (last + QUIET).saturating_duration_since(now);
        let b = (first + MAX_WAIT).saturating_duration_since(now);
        Some(a.min(b))
    }
}

enum Msg {
    Changed(Vec<PathBuf>),
    Rebuild,
}

/// 曲の DB と見張り
pub struct LocalLibrary {
    latest: Arc<Mutex<Option<Library>>>,
    tx: Sender<Msg>,
}

fn now_s() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

impl LocalLibrary {
    /// LWT-01: `db_path` の DB を開いて今の曲一覧を返し、確かめと見張りを始める。`loaders` は読み込みのたびに作る
    pub fn start(
        db_path: PathBuf,
        roots: Vec<PathBuf>,
        fs: Arc<dyn FileSystem>,
        loaders: fn() -> Vec<Box<dyn ChartLoader>>,
    ) -> Result<(Library, Self), PortError> {
        let mut db = song_db::SqliteSongDb::open(&db_path)?;
        let first = library::load(&db)?;
        let latest = Arc::new(Mutex::new(None));
        let (tx, rx) = channel();
        let out = latest.clone();
        let watch_tx = tx.clone();
        // 見張りの知らせは実体のパスで来ることがある（macOS の /var → /private/var など）ので、登録したパスに直す
        let real: Vec<(PathBuf, PathBuf)> = roots
            .iter()
            .filter_map(|r| Some((r.canonicalize().ok()?, r.clone())))
            .collect();
        std::thread::spawn(move || {
            // 見張りを先に始めて、確かめている間の変更も落とさない
            let watcher = notify::recommended_watcher(move |ev: notify::Result<notify::Event>| {
                if let Ok(ev) = ev {
                    let paths = ev.paths.into_iter().map(|p| to_registered(&real, p));
                    let _ = watch_tx.send(Msg::Changed(paths.collect()));
                }
            });
            let mut watcher = match watcher {
                Ok(w) => Some(w),
                Err(e) => {
                    eprintln!("フォルダを見張れない: {e}");
                    None
                }
            };
            if let Some(w) = watcher.as_mut() {
                for r in &roots {
                    if let Err(e) = w.watch(r, RecursiveMode::Recursive) {
                        eprintln!("{} を見張れない: {e}", r.display());
                    }
                }
            }
            run(&mut db, &roots, fs.as_ref(), loaders, rx, &out);
            drop(watcher);
        });
        Ok((first, Self { latest, tx }))
    }

    /// LWT-04: DB を作り直す
    pub fn rebuild(&self) {
        let _ = self.tx.send(Msg::Rebuild);
    }
}

/// 実体のパスを登録したパスに直す（登録したパスの下ならそのまま）
fn to_registered(real: &[(PathBuf, PathBuf)], p: PathBuf) -> PathBuf {
    for (canon, root) in real {
        if !p.starts_with(root) {
            if let Ok(rest) = p.strip_prefix(canon) {
                return root.join(rest);
            }
        }
    }
    p
}

fn publish(db: &song_db::SqliteSongDb, out: &Mutex<Option<Library>>) {
    match library::load(db) {
        Ok(l) => *out.lock().unwrap_or_else(|e| e.into_inner()) = Some(l),
        Err(e) => eprintln!("曲の DB を読めない: {e:?}"),
    }
}

fn run(
    db: &mut song_db::SqliteSongDb,
    roots: &[PathBuf],
    fs: &dyn FileSystem,
    loaders: fn() -> Vec<Box<dyn ChartLoader>>,
    rx: Receiver<Msg>,
    out: &Mutex<Option<Library>>,
) {
    let ls = loaders();
    let scan = |now| Scan {
        fs,
        loaders: &ls,
        now,
    };
    match library::update(&scan(now_s()), db, roots) {
        Ok(true) => publish(db, out),
        Ok(false) => {}
        Err(e) => eprintln!("曲の DB を確かめられない: {e:?}"),
    }
    let mut pending = Debounce::default();
    loop {
        let msg = match pending.wait(Instant::now()) {
            Some(d) => rx.recv_timeout(d),
            None => rx.recv().map_err(|_| RecvTimeoutError::Disconnected),
        };
        match msg {
            Ok(Msg::Changed(paths)) => pending.push(Instant::now(), paths),
            Ok(Msg::Rebuild) => {
                pending = Debounce::default();
                match library::rebuild(&scan(now_s()), db, roots) {
                    Ok(_) => publish(db, out),
                    Err(e) => eprintln!("曲の DB を作り直せない: {e:?}"),
                }
            }
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => return,
        }
        if let Some(paths) = pending.take(Instant::now()) {
            match library::apply(&scan(now_s()), db, roots, &paths) {
                Ok(true) => publish(db, out),
                Ok(false) => {}
                Err(e) => eprintln!("曲の DB を読み直せない: {e:?}"),
            }
        }
    }
}

impl LibrarySource for LocalLibrary {
    fn poll(&self) -> Option<Library> {
        self.latest.lock().unwrap_or_else(|e| e.into_inner()).take()
    }
}

#[cfg(test)]
mod tests;
