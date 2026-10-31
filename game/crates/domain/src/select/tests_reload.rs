//! DOM-SEL-22（曲一覧が替わったとき）のテスト。仕様ルール（lib.rs の表）とテスト関数は 1 対 1。
use super::*;
use rstest::rstest;

/// 絞り込まない
const ALL: ListFilter = ListFilter {
    sort: 0,
    key: 0,
    difficulty: 0,
};

fn no_record(_: &ChartHash) -> SortRecord {
    SortRecord::default()
}

fn song(path: &str, md5: u8) -> SongInfo {
    SongInfo {
        path: path.into(),
        hash: ChartHash {
            md5: [md5; 16],
            sha256: None,
        },
        title: path.into(),
        subtitle: String::new(),
        artist: String::new(),
        genre: String::new(),
        level: 1,
        difficulty: 2,
        key_mode: KeyMode::K7,
        notes: 100,
    }
}

fn lib(roots: &[&str], folders: &[&str], songs: Vec<SongInfo>) -> Library {
    Library::new(
        roots.iter().map(PathBuf::from).collect(),
        folders.iter().map(PathBuf::from).collect(),
        songs,
    )
}

/// 一覧を文字にする（フォルダは名前、譜面はパス）
fn names(l: &Library, bars: &[Bar]) -> Vec<String> {
    bars.iter()
        .map(|b| match b {
            Bar::Folder { title, .. } => format!("[{title}]"),
            Bar::Song(i) => l.songs()[*i].path.to_string_lossy().into_owned(),
        })
        .collect()
}

/// DOM-SEL-22: 曲一覧が替わったとき（開いていたフォルダ, 選んでいたバー, 新しい曲一覧の譜面 → (深さ, 選ぶバー)）
#[rstest]
#[case::same_bar_moves(&["r"], "/r/b/2.bms", &["/r/a/0.bms", "/r/a/1.bms", "/r/b/2.bms"], (1, "/r/b/2.bms"))]
#[case::removed_bar_keeps_position(&["r"], "/r/b/2.bms", &["/r/a/1.bms", "/r/c/3.bms"], (1, "/r/c/3.bms"))]
#[case::position_clamped(&["r"], "/r/b/2.bms", &["/r/a/1.bms"], (1, "/r/a/1.bms"))]
#[case::folder_gone_closes(&["s", "p"], "/s/p/q/4.bms", &["/r/a/1.bms"], (0, "[s]"))]
#[case::nested_kept(&["s", "p"], "/s/p/q/4.bms", &["/s/p/q/4.bms", "/s/p/q/5.bms"], (2, "/s/p/q/4.bms"))]
fn dom_sel_22_reload(
    #[case] open: &[&str],
    #[case] at: &str,
    #[case] then: &[&str],
    #[case] want: (usize, &str),
) {
    let songs = |paths: &[&str]| {
        paths
            .iter()
            .enumerate()
            .map(|(i, p)| song(p, i as u8))
            .collect::<Vec<_>>()
    };
    let dirs_of = |paths: &[&str]| {
        ["/r/a", "/r/b", "/r/c", "/s/p", "/s/p/q"]
            .into_iter()
            .filter(|d| paths.iter().any(|p| p.starts_with(&format!("{d}/"))))
            .collect::<Vec<_>>()
    };
    let first = ["/r/a/1.bms", "/r/b/2.bms", "/s/p/q/4.bms"];
    let before = lib(&["/r", "/s"], &dirs_of(&first), songs(&first));
    let mut s = SelectState::new(&before);
    let mut f = ALL;
    let find = |s: &mut SelectState, l: &Library, want: &str| {
        for _ in 0..s.bars().len() {
            if names(l, std::slice::from_ref(s.current().unwrap()))[0] == want {
                return;
            }
            s.scroll(1);
        }
        panic!("{want} が無い");
    };
    for folder in open {
        find(&mut s, &before, &format!("[{folder}]"));
        assert_eq!(s.decide(&before, &mut f, &no_record), Decided::Opened);
    }
    find(&mut s, &before, at);
    let after = lib(&["/r", "/s"], &dirs_of(then), songs(then));
    s.reload(&before, &after, &mut f, &no_record);
    assert_eq!(
        (
            s.depth(),
            names(&after, std::slice::from_ref(s.current().unwrap()))[0].clone()
        ),
        (want.0, want.1.to_string())
    );
}
