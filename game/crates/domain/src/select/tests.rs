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
        notes: 100,
    }
}

/// 絞り込まない（すべての鍵盤数・すべての難易度・ディレクトリ順）
const ALL: ListFilter = ListFilter {
    sort: 0,
    key: 0,
    difficulty: 0,
};

fn no_record(_: &ChartHash) -> SortRecord {
    SortRecord::default()
}

/// 絞り込まずに開く
fn open_all(l: &Library, folder: &str) -> Option<Vec<Bar>> {
    l.open(Path::new(folder), &mut ALL.clone(), &no_record)
        .map(|x| x.bars)
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
    let got = open_all(&l, folder).map(|b| names(&l, &b));
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
    assert_eq!(names(&l, &open_all(&l, "/r").unwrap()), want);
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
    let mut f = ALL;
    match s.decide(&l, &mut f, &no_record) {
        Decided::Opened => {
            assert_eq!((s.cursor(), s.depth()), (0, 1));
            s.scroll(1);
            assert_eq!(s.decide(&l, &mut f, &no_record), Decided::Song(1));
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

/// 一覧 /r の譜面を開いた名前と、開いた後の絞り込み
fn open_with(songs: Vec<SongInfo>, mut f: ListFilter) -> (Vec<String>, ListFilter) {
    let l = lib(&["/r"], &[], songs);
    let bars = l
        .open(Path::new("/r"), &mut f, &no_record)
        .map(|x| x.bars)
        .unwrap_or_default();
    (names(&l, &bars), f)
}

fn keys(key: i32) -> ListFilter {
    ListFilter { key, ..ALL }
}

/// DOM-SEL-12: 鍵盤数の絞り込み（1 つも残らなければ先の鍵盤数へ）
#[rstest]
#[case::all(0, vec!["/r/a/5.bms", "/r/a/7.bms", "/r/a/9.bms", "/r/a/14.bms"], 0)]
#[case::single(1, vec!["/r/a/5.bms", "/r/a/7.bms"], 1)]
#[case::seven(2, vec!["/r/a/7.bms"], 2)]
#[case::five(3, vec!["/r/a/5.bms"], 3)]
#[case::double(4, vec!["/r/a/14.bms"], 4)]
#[case::fourteen(5, vec!["/r/a/14.bms"], 5)]
#[case::ten_none_goes_to_nine(6, vec!["/r/a/9.bms"], 7)]
#[case::nine(7, vec!["/r/a/9.bms"], 7)]
fn dom_sel_12_key_filter(#[case] key: i32, #[case] want: Vec<&str>, #[case] after: i32) {
    let songs = vec![
        song("/r/a/7.bms", 1, KeyMode::K7, 1, 2),
        song("/r/a/5.bms", 2, KeyMode::K5, 1, 2),
        song("/r/a/14.bms", 3, KeyMode::K14, 1, 2),
        song("/r/a/9.bms", 4, KeyMode::K9, 1, 2),
    ];
    let (got, f) = open_with(songs, keys(key));
    assert_eq!(
        (got, f.key),
        (want.into_iter().map(String::from).collect(), after)
    );
}

/// DOM-SEL-13: BUTTON 10・11・12 は 1 つ先へ回す（最大の次は 0）
#[rstest]
#[case::difficulty(FilterKind::Difficulty, 4, 5)]
#[case::difficulty_wraps(FilterKind::Difficulty, 5, 0)]
#[case::key(FilterKind::Key, 1, 2)]
#[case::key_wraps(FilterKind::Key, 7, 0)]
#[case::sort(FilterKind::Sort, 0, 1)]
#[case::sort_wraps(FilterKind::Sort, 4, 0)]
fn dom_sel_13_next(#[case] kind: FilterKind, #[case] from: i32, #[case] want: i32) {
    let mut f = ListFilter {
        sort: from,
        key: from,
        difficulty: from,
    };
    f.next(kind);
    let got = match kind {
        FilterKind::Difficulty => f.difficulty,
        FilterKind::Key => f.key,
        FilterKind::Sort => f.sort,
    };
    assert_eq!(got, want);
}

/// DOM-SEL-14: 難易度の絞り込み（選んだ難易度の譜面がどこにも無ければ先の難易度へ。無ければすべて）
#[rstest]
#[case::all(0, vec!["/r/a/1.bms", "/r/a/2.bms", "/r/a/3.bms", "/r/a/3b.bms", "/r/b/4.bms"], 0)]
#[case::beginner(1, vec!["/r/a/1.bms", "/r/b/4.bms"], 1)]
#[case::normal_or_lowest(2, vec!["/r/a/2.bms", "/r/b/4.bms"], 2)]
#[case::same_difficulty_all(3, vec!["/r/a/3.bms", "/r/a/3b.bms", "/r/b/4.bms"], 3)]
#[case::highest_below(4, vec!["/r/a/3.bms", "/r/a/3b.bms", "/r/b/4.bms"], 4)]
#[case::none_goes_to_all(5, vec!["/r/a/1.bms", "/r/a/2.bms", "/r/a/3.bms", "/r/a/3b.bms", "/r/b/4.bms"], 0)]
fn dom_sel_14_difficulty_filter(#[case] d: i32, #[case] want: Vec<&str>, #[case] after: i32) {
    let songs = vec![
        song("/r/a/1.bms", 1, KeyMode::K7, 1, 1),
        song("/r/a/2.bms", 2, KeyMode::K7, 5, 2),
        song("/r/a/3.bms", 3, KeyMode::K7, 8, 3),
        song("/r/a/3b.bms", 4, KeyMode::K7, 9, 3),
        song("/r/b/4.bms", 5, KeyMode::K7, 12, 4),
    ];
    let f = ListFilter {
        difficulty: d,
        ..ALL
    };
    let (got, f) = open_with(songs, f);
    assert_eq!(
        (got, f.difficulty),
        (want.into_iter().map(String::from).collect(), after)
    );
}

/// DOM-SEL-15: 並べ替え（記録は MD5 の 1 バイト目で引く: ランプ, EX スコア）
#[rstest]
#[case::directory(0, vec!["/r/a/y.bms", "/r/a/x.bms", "/r/b/z.bms", "/r/b/w.bms"])]
#[case::level(1, vec!["/r/b/z.bms", "/r/a/y.bms", "/r/a/x.bms", "/r/b/w.bms"])]
#[case::title(2, vec!["/r/b/z.bms", "/r/b/w.bms", "/r/a/y.bms", "/r/a/x.bms"])]
#[case::clear(3, vec!["/r/a/y.bms", "/r/b/z.bms", "/r/b/w.bms", "/r/a/x.bms"])]
#[case::score(4, vec!["/r/a/x.bms", "/r/b/w.bms", "/r/b/z.bms", "/r/a/y.bms"])]
fn dom_sel_15_sort(#[case] sort: i32, #[case] want: Vec<&str>) {
    let mut s = vec![
        song("/r/a/x.bms", 1, KeyMode::K7, 9, 3),
        song("/r/a/y.bms", 2, KeyMode::K7, 5, 2),
        song("/r/b/z.bms", 3, KeyMode::K7, 1, 1),
        song("/r/b/w.bms", 4, KeyMode::K7, 2, 5),
    ];
    // 題名: x = "Beta"・y = "beta"（大文字で同じ → レベル順）・z = "alpha"・w = "Alpha"（大文字で同じ → INSANE は後ろ）
    for (i, t) in ["Beta", "beta", "alpha", "Alpha"].iter().enumerate() {
        s[i].title = t.to_string();
    }
    let rec = |h: &ChartHash| match h.md5[0] {
        1 => SortRecord {
            clear: 5,
            ex_score: 10,
        },
        2 => SortRecord {
            clear: 1,
            ex_score: 150,
        },
        3 => SortRecord {
            clear: 3,
            ex_score: 100,
        },
        _ => SortRecord {
            clear: 3,
            ex_score: 50,
        },
    };
    let l = lib(&["/r"], &[], s);
    let mut f = ListFilter { sort, ..ALL };
    let bars = l.open(Path::new("/r"), &mut f, &rec).unwrap().bars;
    assert_eq!(names(&l, &bars), want);
}

/// DOM-SEL-16: → と SELECT の難易度の替え方（今の絞り込み, 曲の難易度, 単位の難易度ごとの数）
#[rstest]
#[case::next_higher(0, 2, [1, 1, 0, 1, 0], 4)]
#[case::from_filtered(4, 2, [1, 1, 1, 0, 0], 3)]
#[case::top_from_all_goes_lowest(0, 4, [0, 1, 0, 1, 0], 2)]
#[case::top_from_filter_goes_all(2, 4, [0, 1, 0, 1, 0], 0)]
#[case::insane_has_no_next(3, 5, [1, 0, 0, 0, 1], 0)]
#[case::same_difficulty_twice_goes_next(0, 3, [0, 0, 2, 1, 0], 4)]
fn dom_sel_16_next_difficulty(
    #[case] filter: i32,
    #[case] d: i32,
    #[case] exist: [i32; 5],
    #[case] want: i32,
) {
    let g = Group { count: 0, exist };
    assert_eq!(filter::next_difficulty(filter, d, &g), want);
}

/// DOM-SEL-17: `#DIFFICULTY` の無い譜面の難易度（パス, 鍵盤数, ノーツ数, 難易度）
#[rstest]
#[case::first_is_normal(vec![("/r/a/1.bms", KeyMode::K7, 100, -1)], vec![2])]
#[case::by_notes(vec![("/r/a/1.bms", KeyMode::K7, 900, -1), ("/r/a/2.bms", KeyMode::K7, 300, -1), ("/r/a/3.bms", KeyMode::K7, 600, -1)], vec![4, 2, 3])]
#[case::never_insane(vec![("/r/a/1.bms", KeyMode::K7, 1, -1), ("/r/a/2.bms", KeyMode::K7, 2, -1), ("/r/a/3.bms", KeyMode::K7, 3, -1), ("/r/a/4.bms", KeyMode::K7, 4, -1)], vec![2, 3, 4, 4])]
#[case::after_defined(vec![("/r/a/1.bms", KeyMode::K7, 1, 1), ("/r/a/2.bms", KeyMode::K7, 2, -1), ("/r/a/3.bms", KeyMode::K7, 3, 5), ("/r/a/4.bms", KeyMode::K7, 4, -1)], vec![1, 2, 5, 5])]
#[case::other_key_mode_restarts(vec![("/r/a/1.bms", KeyMode::K7, 1, 3), ("/r/a/2.bms", KeyMode::K14, 2, -1)], vec![3, 2])]
#[case::other_folder_restarts(vec![("/r/a/1.bms", KeyMode::K7, 1, 3), ("/r/b/2.bms", KeyMode::K7, 2, -1)], vec![3, 2])]
#[case::zero_is_defined(vec![("/r/a/1.bms", KeyMode::K7, 1, 0)], vec![0])]
fn dom_sel_17_undefined_difficulty(
    #[case] songs: Vec<(&str, KeyMode, u32, i32)>,
    #[case] want: Vec<i32>,
) {
    let songs = songs
        .into_iter()
        .enumerate()
        .map(|(i, (p, km, n, d))| SongInfo {
            notes: n,
            ..song(p, i as u8, km, 1, d)
        })
        .collect();
    let l = lib(&["/r"], &[], songs);
    let got: Vec<i32> = l.songs().iter().map(|s| s.difficulty).collect();
    assert_eq!(got, want);
}

fn h(b: u8) -> ChartHash {
    ChartHash {
        md5: [b; 16],
        sha256: None,
    }
}

/// DOM-SEL-18: プレビュー音（フレームごとの (タイマー 11 の経過, 選んでいる曲の MD5)）
#[rstest]
#[case::waits_500ms(vec![(Some(499), Some(1)), (Some(500), Some(1))], vec![PreviewAction::None, PreviewAction::Start(1)])]
#[case::not_for_folder(vec![(Some(900), None)], vec![PreviewAction::None])]
#[case::timer_stopped(vec![(None, Some(1))], vec![PreviewAction::None])]
#[case::plays_once(vec![(Some(500), Some(1)), (Some(9000), Some(1))], vec![PreviewAction::Start(1), PreviewAction::None])]
#[case::stops_on_change(vec![(Some(500), Some(1)), (Some(0), Some(2)), (Some(499), Some(2)), (Some(500), Some(2))], vec![PreviewAction::Start(1), PreviewAction::Stop, PreviewAction::None, PreviewAction::Start(2)])]
#[case::stops_on_folder(vec![(Some(500), Some(1)), (Some(0), None), (Some(900), None)], vec![PreviewAction::Start(1), PreviewAction::Stop, PreviewAction::None])]
fn dom_sel_18_preview(
    #[case] frames: Vec<(Option<i64>, Option<u8>)>,
    #[case] want: Vec<PreviewAction>,
) {
    let mut p = Preview::default();
    let got: Vec<PreviewAction> = frames
        .into_iter()
        .map(|(t, c)| p.update(t, c.map(|b| (b as usize, h(b)))))
        .collect();
    assert_eq!(got, want);
}

/// DOM-SEL-19: パネル 1（フレームごとの (START を押した, 離した)）
#[rstest]
#[case::hold_opens_release_closes(vec![(true, false), (false, false), (false, true)], vec![Some(PanelChange::Opened), None, Some(PanelChange::Closed)], false)]
#[case::press_again_closes(vec![(true, false), (true, false)], vec![Some(PanelChange::Opened), Some(PanelChange::Closed)], false)]
#[case::release_while_closed(vec![(false, true)], vec![None], false)]
#[case::stays_open(vec![(true, false)], vec![Some(PanelChange::Opened)], true)]
fn dom_sel_19_panel(
    #[case] frames: Vec<(bool, bool)>,
    #[case] want: Vec<Option<PanelChange>>,
    #[case] open: bool,
) {
    let mut p = Panel::default();
    let got: Vec<_> = frames.into_iter().map(|(a, b)| p.update(a, b)).collect();
    assert_eq!((got, p.is_open()), (want, open));
}

/// DOM-SEL-20: 絞り込みを替えた後のカーソル（開いて選んだ譜面, 替えた後の絞り込み, 鍵盤数を替えたか → 選ぶ譜面）
#[rstest]
#[case::same_path(ALL, "/r/b/2.bms", ListFilter { difficulty: 2, ..ALL }, false, "/r/b/2.bms")]
#[case::same_folder_first(ALL, "/r/a/1.bms", ListFilter { difficulty: 3, ..ALL }, false, "/r/a/3.bms")]
#[case::key_changed_same_folder(ALL, "/r/a/5.bms", keys(2), true, "/r/a/1.bms")]
#[case::otherwise_top(ALL, "/r/a/5.bms", keys(2), false, "/r/a/1.bms")]
#[case::otherwise_top_b(ListFilter { sort: 2, ..ALL }, "/r/a/5.bms", ListFilter { sort: 2, key: 2, difficulty: 0 }, false, "/r/b/2.bms")]
fn dom_sel_20_refilter_cursor(
    #[case] first: ListFilter,
    #[case] at: &str,
    #[case] then: ListFilter,
    #[case] key_changed: bool,
    #[case] want: &str,
) {
    let mut songs = vec![
        song("/r/a/1.bms", 1, KeyMode::K7, 1, 1),
        song("/r/a/2.bms", 2, KeyMode::K7, 5, 2),
        song("/r/a/3.bms", 3, KeyMode::K7, 8, 3),
        song("/r/a/5.bms", 4, KeyMode::K5, 3, 2),
        song("/r/b/2.bms", 5, KeyMode::K7, 4, 2),
    ];
    // 題名順では /r/b/2.bms が先頭
    for (i, t) in ["m", "n", "o", "p", "a"].iter().enumerate() {
        songs[i].title = t.to_string();
    }
    let l = lib(&["/r"], &[], songs);
    let mut s = SelectState::new(&l);
    let mut f = first;
    assert_eq!(s.decide(&l, &mut f, &no_record), Decided::Opened);
    while names(&l, std::slice::from_ref(s.current().unwrap())) != vec![at] {
        s.scroll(1);
    }
    let mut f = then;
    s.refilter(&l, &mut f, &no_record, key_changed);
    assert_eq!(
        names(&l, std::slice::from_ref(s.current().unwrap())),
        vec![want]
    );
}

/// DOM-SEL-21: → と SELECT は曲を選んでいるときだけ難易度を替えて一覧を作り直す
#[rstest]
#[case::folder_does_nothing(false, None)]
#[case::song_reloads(true, Some((4, vec!["/r/a/2.bms"])))]
fn dom_sel_21_change_difficulty(#[case] open: bool, #[case] want: Option<(i32, Vec<&str>)>) {
    let l = lib(
        &["/r"],
        &[],
        vec![
            song("/r/a/1.bms", 1, KeyMode::K7, 1, 2),
            song("/r/a/2.bms", 2, KeyMode::K7, 5, 4),
        ],
    );
    let mut s = SelectState::new(&l);
    let mut f = ALL;
    if open {
        s.decide(&l, &mut f, &no_record);
    }
    let changed = s.change_difficulty(&l, &mut f, &no_record);
    let got = changed.then(|| (f.difficulty, names(&l, s.bars())));
    assert_eq!(
        got,
        want.map(|(d, v)| (d, v.into_iter().map(String::from).collect()))
    );
}
