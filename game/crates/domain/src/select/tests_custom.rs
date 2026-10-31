//! DOM-SEL-23..25（カスタムフォルダと難易度表）のテスト。仕様ルール（lib.rs の表）とテスト関数は 1 対 1。
use super::*;
use rstest::rstest;

/// 譜面（パス, MD5 の 1 バイト目, 鍵盤数, 難易度）
fn song(path: &str, md5: u8, key_mode: KeyMode, difficulty: i32) -> SongInfo {
    SongInfo {
        path: path.into(),
        hash: ChartHash {
            md5: [md5; 16],
            sha256: Some([md5.wrapping_add(100); 32]),
        },
        title: path.into(),
        subtitle: String::new(),
        artist: String::new(),
        genre: String::new(),
        level: 1,
        difficulty,
        key_mode,
        notes: 100,
    }
}

fn custom(path: &str, title: &str, command: &str, max: i32) -> CustomFolder {
    CustomFolder {
        path: path.into(),
        title: title.into(),
        command: command.into(),
        max,
        kind: 2,
        ..Default::default()
    }
}

/// カスタムフォルダの SQL の結果を決めておく
struct Fixed(Option<CustomResult>);

impl Lookup for Fixed {
    fn record(&self, _: &ChartHash) -> SortRecord {
        SortRecord::default()
    }
    fn custom(&self, _: &CustomFolder) -> Option<CustomResult> {
        self.0.clone()
    }
}

fn names(l: &Library, bars: &[Bar]) -> Vec<String> {
    bars.iter()
        .map(|b| match b {
            Bar::Folder { title, .. } => format!("[{title}]"),
            Bar::Song(i) => l.songs()[*i].path.to_string_lossy().into_owned(),
            Bar::Custom(c) => format!("<{}>", l.custom()[*c].title),
            Bar::Table(t) => format!("{{{}}}", l.tables()[*t].name),
            Bar::Level(t, v) => format!("{{{}}}", l.tables()[*t].levels[*v].name),
            Bar::Missing(t, v, k) => format!("?{}", l.tables()[*t].levels[*v].charts[*k].title),
        })
        .collect()
}

fn base() -> Library {
    Library::new(
        vec!["/b".into(), "/a".into()],
        vec![
            "/a/x".into(),
            "/a/x/s1".into(),
            "/a/y".into(),
            "/a/y/s2".into(),
        ],
        vec![
            song("/a/x/s1/1.bms", 1, KeyMode::K7, 1),
            song("/a/x/s1/2.bms", 2, KeyMode::K5, 3),
            song("/a/y/s2/3.bms", 3, KeyMode::K7, 2),
            song("/a/y/s2/4.bms", 4, KeyMode::K14, 4),
        ],
    )
}

/// DOM-SEL-23: カスタムフォルダの並ぶところ
#[rstest]
#[case::root("", vec!["<a.lr2folder>", "[a]", "[b]"])]
#[case::inside_folder("/a", vec!["<m>", "[x]", "[y]"])]
#[case::not_beside_songs("/a/x", vec!["/a/x/s1/2.bms", "/a/x/s1/1.bms"])]
fn dom_sel_23_custom_listed(#[case] folder: &str, #[case] want: Vec<&str>) {
    let mut roots = vec![
        PathBuf::from("/b"),
        PathBuf::from("/a"),
        PathBuf::from("/0/a.lr2folder"),
    ];
    roots.sort();
    let l = Library::new(
        roots,
        vec![
            "/a/x".into(),
            "/a/x/s1".into(),
            "/a/y".into(),
            "/a/y/s2".into(),
        ],
        base().songs().to_vec(),
    )
    .with_custom(vec![
        custom("/0/a.lr2folder", "a.lr2folder", "clear = 1", 0),
        custom("/a/m.lr2folder", "m", "clear = 1", 0),
        custom("/a/x/z.lr2folder", "z", "clear = 1", 0),
    ]);
    let mut filter = ListFilter {
        sort: 0,
        key: 0,
        difficulty: 0,
    };
    let bars = if folder.is_empty() {
        l.root_bars()
    } else {
        l.open(Path::new(folder), &mut filter, &Fixed(None))
            .unwrap()
            .bars
    };
    assert_eq!(names(&l, &bars), want);
}

/// DOM-SEL-24: カスタムフォルダを開く
#[rstest]
// 鍵盤数（7 鍵）で絞り込み、難易度（3）は絞り込まず、ディレクトリ順に並べる
#[case::key_filter(
    "clear = 1", 0, false, vec!["/a/y/s2/3.bms", "/a/x/s1/1.bms", "/a/y/s2/4.bms"], 2,
    Some(vec!["/a/x/s1/1.bms", "/a/y/s2/3.bms"]), 2
)]
// 7 鍵が無ければ鍵盤数を先へ回す
#[case::key_rotates(
    "clear = 1", 0, false, vec!["/a/y/s2/4.bms"], 2, Some(vec!["/a/y/s2/4.bms"]), 4
)]
// 上限は同じ MD5 を除いてから先頭から（SQL の順）
#[case::max(
    "clear = 1", 2, false, vec!["/a/y/s2/3.bms", "/a/y/s2/3.bms", "/a/x/s1/1.bms", "/a/x/s1/2.bms"], 0,
    Some(vec!["/a/x/s1/1.bms", "/a/y/s2/3.bms"]), 0
)]
// 並びが決まっていれば絞り込みも並べ替えもしない
#[case::ordered(
    "1 ORDER BY level", 0, true, vec!["/a/y/s2/4.bms", "/a/x/s1/2.bms", "/a/x/s1/1.bms"], 2,
    Some(vec!["/a/y/s2/4.bms", "/a/x/s1/2.bms", "/a/x/s1/1.bms"]), 2
)]
#[case::ordered_max(
    "1 = 1", 1, true, vec!["/a/x/s1/2.bms", "/a/x/s1/1.bms"], 2, Some(vec!["/a/x/s1/2.bms"]), 2
)]
// 曲の一覧に無いパスは飛ばす。何も残らなければ開けない
#[case::empty("clear = 1", 0, false, vec!["/z/none.bms"], 0, None, 0)]
// command が 2 バイト以下・(null) なら SQL を引かず、普通のフォルダとして開く（中に何も無いので開けない）
#[case::short_command("1=", 0, false, vec!["/a/x/s1/1.bms"], 0, None, 0)]
#[case::null_command("(null)", 0, false, vec!["/a/x/s1/1.bms"], 0, None, 0)]
fn dom_sel_24_custom_open(
    #[case] command: &str,
    #[case] max: i32,
    #[case] ordered: bool,
    #[case] result: Vec<&str>,
    #[case] key: i32,
    #[case] want: Option<Vec<&str>>,
    #[case] key_after: i32,
) {
    let l = base().with_custom(vec![custom("/a/c.lr2folder", "c", command, max)]);
    let look = Fixed(Some(CustomResult {
        paths: result.iter().map(PathBuf::from).collect(),
        ordered,
    }));
    let mut filter = ListFilter {
        sort: 0,
        key,
        difficulty: 3,
    };
    let got = l
        .open(Path::new("/a/c.lr2folder"), &mut filter, &look)
        .map(|x| names(&l, &x.bars));
    assert_eq!(got, want.map(|w| w.iter().map(|s| s.to_string()).collect()));
    assert_eq!((filter.key, filter.difficulty), (key_after, 3));
    // SQL を引けなければ開けない
    assert_eq!(
        l.open(Path::new("/a/c.lr2folder"), &mut filter, &Fixed(None)),
        None
    );
}

fn chart(md5: &str, sha256: &str, title: &str) -> TableChart {
    TableChart {
        md5: md5.into(),
        sha256: sha256.into(),
        title: title.into(),
        artist: String::new(),
    }
}

fn hex1(b: u8, n: usize) -> String {
    format!("{b:02x}").repeat(n)
}

fn tables() -> Library {
    base().with_tables(vec![
        DifficultyTable {
            url: "https://t1/".into(),
            name: "T1".into(),
            levels: vec![
                TableLevel {
                    name: "★1".into(),
                    charts: vec![
                        chart(&hex1(3, 16), "", "three"),
                        chart("", &hex1(101, 32), "one by sha"),
                        chart(&hex1(9, 16), "", "nine"),
                        chart(&hex1(9, 16), &hex1(102, 32), "sha fallback"),
                    ],
                },
                TableLevel {
                    name: "★2".into(),
                    charts: vec![],
                },
            ],
        },
        DifficultyTable {
            url: "https://t2/".into(),
            name: "T2".into(),
            levels: vec![],
        },
    ])
}

/// DOM-SEL-25: 難易度表
#[rstest]
#[case::root(&[], Some(vec!["[a]", "[b]", "{T1}", "{T2}"]))]
#[case::levels(&[2], Some(vec!["{★1}", "{★2}"]))]
// 表の順。MD5、無ければ SHA-256 で曲を引き、無ければ「無い譜面」
#[case::charts(&[2, 0], Some(vec!["/a/y/s2/3.bms", "/a/x/s1/1.bms", "?nine", "/a/x/s1/2.bms"]))]
// 譜面の無いレベル・レベルの無い表は開けない
#[case::empty_level(&[2, 1], None)]
#[case::empty_table(&[3], None)]
fn dom_sel_25_tables(#[case] path: &[usize], #[case] want: Option<Vec<&str>>) {
    let l = tables();
    let mut st = SelectState::new(&l);
    // 鍵盤数・難易度の絞り込みは効かない
    let mut filter = ListFilter {
        sort: 2,
        key: 3,
        difficulty: 5,
    };
    let look = Fixed(None);
    let mut opened = true;
    for &i in path {
        st.set_cursor(i);
        opened = st.decide(&l, &mut filter, &look) == Decided::Opened;
    }
    let got = opened.then(|| names(&l, st.bars()));
    assert_eq!(got, want.map(|w| w.iter().map(|s| s.to_string()).collect()));
    if opened && path.len() == 2 {
        // 無い譜面は決定しても何もしない
        st.set_cursor(2);
        assert_eq!(st.decide(&l, &mut filter, &look), Decided::Nothing);
        // 表を読み直しても同じ段・同じバー
        let old = l.clone();
        st.reload(&old, &l, &mut filter, &look);
        assert_eq!((st.depth(), st.cursor()), (2, 2));
    }
    assert_eq!(
        filter,
        ListFilter {
            sort: 2,
            key: 3,
            difficulty: 5
        }
    );
}
