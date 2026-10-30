//! 仕様ルール（lib.rs の表）とテスト関数は 1 対 1。関数名の接頭辞がルール ID。
use super::*;
use rstest::rstest;

/// 譜面（パス, MD5 の 1 バイト目, 鍵盤数, レベル, 難易度）
fn song(path: &str, md5: u8, key_mode: KeyMode, level: i32, difficulty: i32) -> SongInfo {
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
        level,
        difficulty,
        key_mode,
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

/// DOM-SEL-01: 最初の一覧
#[rstest]
#[case::sorted_upper(&["/b", "/A", "/c"], vec!["[A]", "[b]", "[c]"])]
#[case::upper_before_lower_symbol(&["/a_", "/aB"], vec!["[aB]", "[a_]"])]
#[case::empty(&[], vec![])]
fn dom_sel_01_root(#[case] roots: &[&str], #[case] want: Vec<&str>) {
    let l = lib(roots, &[], vec![]);
    assert_eq!(names(&l, &l.root_bars()), want);
}

/// DOM-SEL-02: フォルダを開く
#[rstest]
#[case::songs_in_child_folders("/bms", Some(vec!["/bms/x/1.bms", "/bms/y/2.bms"]))]
#[case::folders_when_no_songs("/packs", Some(vec!["[p1]", "[P2]"]))]
#[case::song_folder_itself_has_no_children("/bms/x", None)]
#[case::empty_folder("/empty", None)]
fn dom_sel_02_open(#[case] folder: &str, #[case] want: Option<Vec<&str>>) {
    let l = lib(
        &["/bms", "/packs", "/empty"],
        &["/bms/x", "/bms/y", "/packs/p1", "/packs/P2", "/packs/p1/s"],
        vec![
            song("/bms/x/1.bms", 1, KeyMode::K7, 1, 0),
            song("/bms/y/2.bms", 2, KeyMode::K7, 1, 0),
            // 開いたフォルダの直下と孫の譜面は出さない
            song("/bms/0.bms", 3, KeyMode::K7, 1, 0),
            song("/packs/p1/s/3.bms", 4, KeyMode::K7, 1, 0),
        ],
    );
    let got = l.open(Path::new(folder)).map(|b| names(&l, &b));
    assert_eq!(got, want.map(|v| v.into_iter().map(String::from).collect()));
}

/// DOM-SEL-03: 譜面の並び
#[rstest]
#[case::folder_first(vec![song("/r/b/1.bms", 1, KeyMode::K5, 1, 0), song("/r/A/2.bms", 2, KeyMode::K14, 12, 0)], vec!["/r/A/2.bms", "/r/b/1.bms"])]
#[case::key_mode_then_level(vec![song("/r/a/1.bms", 1, KeyMode::K14, 1, 0), song("/r/a/2.bms", 2, KeyMode::K7, 9, 0), song("/r/a/3.bms", 3, KeyMode::K7, 3, 0)], vec!["/r/a/3.bms", "/r/a/2.bms", "/r/a/1.bms"])]
#[case::insane_after(vec![song("/r/a/1.bms", 1, KeyMode::K7, 2, 5), song("/r/a/2.bms", 2, KeyMode::K7, 12, 4)], vec!["/r/a/2.bms", "/r/a/1.bms"])]
#[case::tie_by_path(vec![song("/r/a/b.bms", 1, KeyMode::K7, 5, 0), song("/r/a/A.bms", 2, KeyMode::K7, 5, 0)], vec!["/r/a/A.bms", "/r/a/b.bms"])]
#[case::same_md5_once(vec![song("/r/b/1.bms", 7, KeyMode::K7, 1, 0), song("/r/a/1.bms", 7, KeyMode::K7, 1, 0)], vec!["/r/a/1.bms"])]
fn dom_sel_03_song_order(#[case] songs: Vec<SongInfo>, #[case] want: Vec<&str>) {
    let l = lib(&["/r"], &[], songs);
    assert_eq!(names(&l, &l.open(Path::new("/r")).unwrap()), want);
}

/// DOM-SEL-04: カーソル
#[rstest]
#[case::down(&[1], 1, "[b]")]
#[case::wrap_down(&[1, 1, 1], 0, "[a]")]
#[case::wrap_up(&[-1], 2, "[c]")]
#[case::many(&[5], 2, "[c]")]
fn dom_sel_04_cursor(#[case] moves: &[i32], #[case] cursor: usize, #[case] at: &str) {
    let l = lib(
        &["/a", "/b", "/c"],
        &["/b/s"],
        vec![
            song("/b/s/1.bms", 1, KeyMode::K7, 1, 0),
            song("/b/s/2.bms", 2, KeyMode::K7, 2, 0),
        ],
    );
    let mut s = SelectState::new(&l);
    for m in moves {
        s.scroll(*m);
    }
    assert_eq!(s.cursor(), cursor);
    assert_eq!(
        names(&l, std::slice::from_ref(s.current().unwrap())),
        vec![at]
    );
    // 開ける場所なら開いて先頭、譜面は決定、戻ると元の位置
    match s.decide(&l) {
        Decided::Opened => {
            assert_eq!((s.cursor(), s.depth()), (0, 1));
            s.scroll(1);
            assert_eq!(s.decide(&l), Decided::Song(1));
            assert!(s.back());
            assert_eq!((s.cursor(), s.depth()), (cursor, 0));
            assert!(!s.back());
        }
        other => {
            assert_eq!(other, Decided::Nothing);
            assert_eq!(s.depth(), 0);
        }
    }
}

/// DOM-SEL-05: スクロールのくり返し（(時刻, 上, 下) → 動く量）
#[rstest]
#[case::press_moves(vec![(0, KeyState::Pressed, KeyState::Off)], vec![-1])]
#[case::hold_waits_first(vec![(0, KeyState::Pressed, KeyState::Off), (279, KeyState::Held, KeyState::Off), (281, KeyState::Held, KeyState::Off)], vec![-1, 0, -1])]
#[case::hold_repeats(vec![(0, KeyState::Off, KeyState::Pressed), (281, KeyState::Off, KeyState::Held), (331, KeyState::Off, KeyState::Held), (332, KeyState::Off, KeyState::Held)], vec![1, 1, 0, 1])]
#[case::press_during_wait_ignored(vec![(0, KeyState::Pressed, KeyState::Off), (100, KeyState::Off, KeyState::Pressed)], vec![-1, 0])]
#[case::up_press_wins(vec![(0, KeyState::Pressed, KeyState::Pressed)], vec![-1])]
#[case::press_beats_hold(vec![(0, KeyState::Held, KeyState::Pressed)], vec![1])]
fn dom_sel_05_scroll_repeat(
    #[case] frames: Vec<(i64, KeyState, KeyState)>,
    #[case] want: Vec<i32>,
) {
    let mut r = ScrollRepeat::default();
    let got: Vec<i32> = frames
        .into_iter()
        .map(|(t, u, d)| r.update(t, u, d))
        .collect();
    assert_eq!(got, want);
}
