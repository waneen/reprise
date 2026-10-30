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

/// DOM-SEL-06: 一覧の動き（(時刻, 操作) → 目標・向き）。操作: ('u'|'d', 押下) のキー、('w', 回転) のホイール
#[rstest]
#[case::key_down(vec![(0, 'd', 1)], 1000, 2, 300)]
#[case::key_up(vec![(0, 'u', 1)], -1000, 1, 300)]
#[case::wheel_up(vec![(0, 'w', 1)], -1000, 1, 200)]
#[case::wheel_down_two(vec![(0, 'w', -2)], 2000, 2, 200)]
#[case::wheel_blocks_key(vec![(0, 'w', -1), (100, 'd', 1)], 1000, 2, 200)]
#[case::key_after_wait(vec![(0, 'w', -1), (201, 'd', 1)], 2000, 2, 501)]
fn dom_sel_06_bar_scroll_input(
    #[case] ops: Vec<(i64, char, i32)>,
    #[case] target: i64,
    #[case] direction: i32,
    #[case] end: i64,
) {
    let mut s = BarScroll::new(0, (0, 0), 0, 10, -1000);
    for (t, k, v) in ops {
        match k {
            'u' => {
                s.keys(t, KeyState::Pressed, KeyState::Off);
            }
            'd' => {
                s.keys(t, KeyState::Off, KeyState::Pressed);
            }
            _ => s.wheel(t, v),
        }
    }
    assert_eq!(
        (s.target, s.direction, s.repeat.end_ms),
        (target, direction, end)
    );
}

/// DOM-SEL-07: 位置（線形・0 の方へ切り捨て・本数で回す）と「動いた」
#[rstest]
#[case::half_way(150, 1, 3, 500, true)]
#[case::done(300, 1, 3, 1000, true)]
#[case::up_truncates_toward_zero(1, -1, 3, 2997, true)]
#[case::up_wraps(150, -1, 3, 2500, true)]
#[case::still(0, 1, 3, 0, false)]
fn dom_sel_07_position(
    #[case] now: i64,
    #[case] delta: i32,
    #[case] n: usize,
    #[case] want: i64,
    #[case] moved: bool,
) {
    let mut s = BarScroll::new(0, (0, 0), 0, n, -1000);
    if delta > 0 {
        s.keys(0, KeyState::Off, KeyState::Pressed);
    } else {
        s.keys(0, KeyState::Pressed, KeyState::Off);
    }
    if now == 0 {
        s = BarScroll::new(0, (0, 0), 0, n, 0);
    }
    assert_eq!(s.update(now, n), moved);
    assert_eq!(s.position(), want);
    // 0 の方への切り捨て: 1ms 目の上向きは −3.33… → −3
    if now == 1 && delta < 0 {
        assert_eq!(s.top, -3);
    }
}

/// DOM-SEL-08: カーソル（端数があり下向きなら次）
#[rstest]
#[case::start(5, 5, 0, 0, 10, 0)]
#[case::slot_above_center(3, 5, 4000, 0, 10, 2)]
#[case::down_fraction_next(5, 5, 4500, 2, 10, 5)]
#[case::up_fraction_stays(5, 5, 4500, 1, 10, 4)]
#[case::wraps(0, 5, 1000, 0, 3, 2)]
fn dom_sel_08_cursor(
    #[case] slot: i32,
    #[case] center: i32,
    #[case] position: i64,
    #[case] direction: i32,
    #[case] n: usize,
    #[case] want: usize,
) {
    let mut s = BarScroll::new(center, (0, 29), 0, n, 0);
    s.slot = slot;
    s.position = position;
    s.direction = direction;
    assert_eq!(s.cursor(n), want);
}

/// DOM-SEL-09: 画面に入った・開いた・戻ったときは止めてカーソルを置く
#[rstest]
#[case::enter(7, None, 3, 10, 3000, 3)]
#[case::other_slot(7, Some(9), 4, 10, 2000, 4)]
#[case::wraps(7, Some(2), 1, 3, 0, 1)]
fn dom_sel_09_jump(
    #[case] center: i32,
    #[case] slot: Option<i32>,
    #[case] cursor: usize,
    #[case] n: usize,
    #[case] position: i64,
    #[case] want: usize,
) {
    let mut s = BarScroll::new(center, (0, 29), cursor, n, 0);
    assert_eq!(s.slot(), center);
    if let Some(slot) = slot {
        s.slot = slot;
        s.jump(cursor, n, 50);
    }
    assert!(s.still());
    assert_eq!((s.position(), s.cursor(n)), (position, want));
    assert!(!s.update(1000, n));
}

/// DOM-SEL-10: マウスで押し続けたスロット（範囲 3..=6）
#[rstest]
#[case::inside(5, true, 5, 0)]
#[case::before(1, true, 4, -4000)]
#[case::after(8, true, 4, 4000)]
#[case::moving_ignored(8, false, 4, 1000)]
fn dom_sel_10_mouse_hold(
    #[case] i: i32,
    #[case] still: bool,
    #[case] slot: i32,
    #[case] target: i64,
) {
    let mut s = BarScroll::new(4, (3, 6), 0, 10, -1000);
    if !still {
        s.keys(0, KeyState::Off, KeyState::Pressed);
    }
    s.mouse_hold(10, i);
    assert_eq!((s.slot(), s.target), (slot, target));
    if still && target != 0 {
        assert_eq!(s.repeat.end_ms, 210);
    }
}

/// DOM-SEL-11: マウスの左ボタンの状態（(時刻, 押しているか) → 状態）
#[rstest]
#[case::click(vec![(1000, true), (1010, true), (1100, false), (1110, false)], vec![1, 2, 3, 0])]
#[case::short_click_held_until_32ms(vec![(1000, true), (1020, false), (1030, false), (1040, false)], vec![1, 2, 2, 3])]
#[case::double_click_keeps_first_press(vec![(1000, true), (1100, false), (1150, true), (1160, false)], vec![1, 3, 1, 3])]
#[case::idle(vec![(0, false)], vec![0])]
fn dom_sel_11_mouse_button(#[case] frames: Vec<(i64, bool)>, #[case] want: Vec<u8>) {
    let mut m = MouseButton::default();
    let got: Vec<u8> = frames.into_iter().map(|(t, d)| m.update(t, d)).collect();
    assert_eq!(got, want);
}
