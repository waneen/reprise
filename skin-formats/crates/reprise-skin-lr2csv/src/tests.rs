//! 仕様ルール（lib.rs の表）とテスト関数は 1 対 1。関数名の接頭辞がルール ID。
//! スキン素材は同梱せず、テスト内で最小の csv を組み立てる。
use super::*;
use rstest::rstest;
use std::collections::BTreeMap;
use std::path::PathBuf;

// ---- テスト用の部品 ----------------------------------------------------------

/// メモリ上のファイル。`*` は辞書順で最初に一致したものを選ぶ（決定的）
#[derive(Default)]
struct MemFs(BTreeMap<String, Vec<u8>>);
impl MemFs {
    fn with(files: &[(&str, &[u8])]) -> Self {
        Self(
            files
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_vec()))
                .collect(),
        )
    }
}
impl FileResolver for MemFs {
    fn resolve(&mut self, pattern: &str) -> Option<PathBuf> {
        match pattern.split_once('*') {
            None => self.0.contains_key(pattern).then(|| PathBuf::from(pattern)),
            Some((pre, post)) => self
                .0
                .keys()
                .find(|k| {
                    k.starts_with(pre) && k.ends_with(post) && k.len() >= pre.len() + post.len()
                })
                .map(PathBuf::from),
        }
    }
    fn read(&mut self, path: &Path) -> Result<Vec<u8>, String> {
        self.0
            .get(path.to_str().unwrap())
            .cloned()
            .ok_or_else(|| "not found".into())
    }
}

const MAIN: &str = "skin.lr2skin";
const INFO: &str = "#INFORMATION,0,Title,Maker,thumb.png\n";

fn decode_full(
    dialect: Dialect,
    body: &str,
    extra: &[(&str, &[u8])],
    settings: &SkinSettings,
) -> SkinDocument {
    let mut files = vec![(MAIN, body.as_bytes())];
    files.extend_from_slice(extra);
    let mut fs = MemFs::with(&files);
    Lr2CsvDecoder { dialect }
        .decode(Path::new(MAIN), settings, &mut fs)
        .expect("decode")
}
fn lr2(body: &str) -> SkinDocument {
    decode_full(Dialect::Lr2, body, &[], &SkinSettings::default())
}
fn standard(body: &str) -> SkinDocument {
    decode_full(Dialect::Standard, body, &[], &SkinSettings::default())
}
fn with_ops(dialect: Dialect, body: &str, ops: &[i32]) -> SkinDocument {
    let s = SkinSettings {
        options: ops.iter().copied().collect(),
        ..Default::default()
    };
    decode_full(dialect, body, &[], &s)
}
/// ディレクティブ名の並び（#IF 等の結果を観測するのに使う）
fn directives(d: &SkinDocument) -> Vec<String> {
    d.directives.iter().map(|x| x.command.clone()).collect()
}
fn header_of(text: &str) -> Result<SkinHeader, SkinError> {
    let mut fs = MemFs::with(&[(MAIN, text.as_bytes())]);
    Lr2CsvDecoder::default().read_header(Path::new(MAIN), &mut fs)
}
const IMG: &str = "#SRC_IMAGE,0,0,0,0,10,10,1,1,0,0\n";

// ---- SKN-ENC / SKN-CSV ---------------------------------------------------------

/// SKN-ENC-01: 文字コード
#[rstest]
#[case::utf8_bom(b"\xEF\xBB\xBF#INFORMATION,0,\xE6\x9B\xB2\n".to_vec(), "曲")]
#[case::utf8(b"#INFORMATION,0,\xE6\x9B\xB2\n".to_vec(), "曲")]
#[case::shift_jis(b"#INFORMATION,0,\x8B\xC8\n".to_vec(), "曲")]
fn skn_enc_01_encoding(#[case] bytes: Vec<u8>, #[case] title: &str) {
    let mut fs = MemFs::with(&[(MAIN, &bytes)]);
    let h = Lr2CsvDecoder::default()
        .read_header(Path::new(MAIN), &mut fs)
        .unwrap();
    assert_eq!(h.title, title);
}

/// SKN-CSV-01: 項目分割と数値化
#[rstest]
#[case::quoted("#X,\"a,b\"", vec!["\"a", "b\""])]
#[case::quote_stripped("#X,\"ab\"", vec!["ab"])]
#[case::empty_field("#XYZ,,c", vec!["", "c"])]
#[case::truncated_at_30(&format!("#X{}", ",1".repeat(40)), vec!["1"; 29])]
fn skn_csv_01_split_fields(#[case] line: &str, #[case] args: Vec<&str>) {
    let d = lr2(&format!("{line}\n"));
    assert_eq!(d.directives[0].args, args);
}

/// SKN-CSV-04: atol と `!` 否定
#[rstest]
#[case::plain("12", 12)]
#[case::trailing_text("12abc", 12)]
#[case::not_a_number("abc", 0)]
#[case::bang_negates("!900", -900)]
#[case::minus("-5", -5)]
fn skn_csv_04_numeric_fields(#[case] field: &str, #[case] expected: i32) {
    assert_eq!(crate::csv::split(&format!("#X,{field}")).v(1), expected);
}

/// SKN-CSV-02: `#` で始まらない行は無視
#[rstest]
#[case::comment("// #STARTINPUT,1")]
#[case::text("STARTINPUT,1")]
fn skn_csv_02_non_command_lines_ignored(#[case] line: &str) {
    assert!(lr2(&format!("{line}\n")).directives.is_empty());
}

/// SKN-CSV-03: (Lr2) 5 文字以下の行は無視
#[rstest]
#[case::lr2_short_else_ignored(Dialect::Lr2, "#IF,901\n#A_LONG,1\n#ELSE\n#B_LONG,1\n#ENDIF\n", vec!["A_LONG", "B_LONG"])]
#[case::lr2_else_with_comma_works(Dialect::Lr2, "#IF,901\n#A_LONG,1\n#ELSE,\n#B_LONG,1\n#ENDIF\n", vec!["A_LONG"])]
#[case::standard_short_else_works(Dialect::Standard, "#IF,901\n#A_LONG,1\n#ELSE\n#B_LONG,1\n#ENDIF\n", vec!["A_LONG"])]
fn skn_csv_03_short_lines(
    #[case] dialect: Dialect,
    #[case] body: &str,
    #[case] expected: Vec<&str>,
) {
    assert_eq!(directives(&with_ops(dialect, body, &[901])), expected);
}

// ---- SKN-HDR ----------------------------------------------------------------

/// SKN-HDR-01: #INFORMATION
#[rstest]
#[case::basic("#INFORMATION,7,T,M,th.png", 7, "T", "M", "th.png", (640, 480))]
#[case::hd("#INFORMATION,0,T,M,th.png,0,1280,720", 0, "T", "M", "th.png", (1280, 720))]
#[case::clamped_up("#INFORMATION,0,T,M,,0,320,240", 0, "T", "M", "", (640, 480))]
fn skn_hdr_01_information(
    #[case] line: &str,
    #[case] kind: i32,
    #[case] title: &str,
    #[case] maker: &str,
    #[case] thumb: &str,
    #[case] res: (u32, u32),
) {
    let h = header_of(line).unwrap();
    assert_eq!(
        (
            h.kind,
            h.title.as_str(),
            h.maker.as_str(),
            h.thumbnail.as_str(),
            h.resolution
        ),
        (kind, title, maker, thumb, res)
    );
}

/// SKN-HDR-02: #RESOLUTION
#[rstest]
#[case::sd("#RESOLUTION,0", (640, 480))]
#[case::hd("#RESOLUTION,1", (1280, 720))]
#[case::fhd("#RESOLUTION,2", (1920, 1080))]
#[case::uhd("#RESOLUTION,3", (3840, 2160))]
#[case::custom("#RESOLUTION,2560,1440", (2560, 1440))]
fn skn_hdr_02_resolution(#[case] line: &str, #[case] res: (u32, u32)) {
    assert_eq!(
        header_of(&format!("{INFO}{line}\n")).unwrap().resolution,
        res
    );
}

/// SKN-HDR-03: #CUSTOMOPTION
#[rstest]
#[case::labels("#CUSTOMOPTION,Gauge,900,A,B,C", Some((900, vec!["A", "B", "C"])))]
#[case::stops_at_empty("#CUSTOMOPTION,Gauge,900,A,,C", Some((900, vec!["A"])))]
#[case::op_1000_ignored("#CUSTOMOPTION,Gauge,1000,A", None)]
fn skn_hdr_03_custom_option(#[case] line: &str, #[case] expected: Option<(i32, Vec<&str>)>) {
    let h = header_of(&format!("{INFO}{line}\n")).unwrap();
    let got = h.customs.first().map(|c| match c {
        Custom::Option {
            start_op, labels, ..
        } => (*start_op, labels.clone()),
        _ => panic!("not option"),
    });
    assert_eq!(
        got,
        expected.map(|(s, l)| (s, l.into_iter().map(String::from).collect()))
    );
}

/// SKN-HDR-04: #CUSTOMFILE
#[rstest]
#[case::basic("#CUSTOMFILE,BG,Theme/bg/*.png,default", "Theme/bg/*.png", "default")]
#[case::backslash("#CUSTOMFILE,BG,Theme\\bg\\*.png,x", "Theme/bg/*.png", "x")]
fn skn_hdr_04_custom_file(#[case] line: &str, #[case] pattern: &str, #[case] default: &str) {
    let h = header_of(&format!("{INFO}{line}\n")).unwrap();
    assert_eq!(
        h.customs,
        vec![Custom::File {
            name: "BG".into(),
            pattern: pattern.into(),
            default: default.into()
        }]
    );
}

/// SKN-HDR-05: #ENDOFHEADER で止まる
#[rstest]
#[case::before_end("#CUSTOMOPTION,A,900,x\n#ENDOFHEADER\n", 1)]
#[case::after_end("#ENDOFHEADER\n#CUSTOMOPTION,A,900,x\n", 0)]
fn skn_hdr_05_endofheader(#[case] body: &str, #[case] count: usize) {
    assert_eq!(
        header_of(&format!("{INFO}{body}")).unwrap().customs.len(),
        count
    );
}

/// SKN-HDR-06: スキンでないもの
#[rstest]
#[case::no_information("#IMAGE,a.png\n", false)]
#[case::negative_kind("#INFORMATION,-1,T\n", false)]
#[case::valid("#INFORMATION,5,T\n", true)]
fn skn_hdr_06_not_a_skin(#[case] text: &str, #[case] ok: bool) {
    assert_eq!(header_of(text).is_ok(), ok);
}

// ---- SKN-OPT / SKN-IF / SKN-SET ---------------------------------------------------

/// SKN-OPT-01: 初期条件
#[rstest]
#[case::default_first_choice(None, vec![], 900, true)]
#[case::chosen_second(Some(901), vec![], 901, true)]
#[case::chosen_out_of_range_falls_back(Some(950), vec![], 900, true)]
#[case::op0_always_true(None, vec![], 0, true)]
#[case::op999_always_false(None, vec![999], 999, false)]
#[case::from_settings(None, vec![42], 42, true)]
fn skn_opt_01_initial_options(
    #[case] choice: Option<i32>,
    #[case] ops: Vec<i32>,
    #[case] probe: i32,
    #[case] expected: bool,
) {
    let body = format!("{INFO}#CUSTOMOPTION,A,900,x,y\n#IF,{probe},0\n#HIT,1\n#ENDIF\n");
    let mut s = SkinSettings {
        options: ops.into_iter().collect(),
        ..Default::default()
    };
    if let Some(c) = choice {
        s.custom.insert(0, CustomChoice::Option(c));
    }
    let d = decode_full(Dialect::Lr2, &body, &[], &s);
    assert_eq!(directives(&d).contains(&"HIT".to_string()), expected);
}

/// SKN-OPT-02: 文書の options は読み込み終了時点で真の条件
#[rstest]
#[case::custom_choice("", 901, true)]
#[case::other_choice_false("", 900, false)]
#[case::setoption_on("#SETOPTION,950,1\n", 950, true)]
#[case::setoption_off("#SETOPTION,901,0\n", 901, false)]
fn skn_opt_02_final_options(#[case] body: &str, #[case] probe: i32, #[case] expected: bool) {
    let mut s = SkinSettings::default();
    s.custom.insert(0, CustomChoice::Option(901));
    let d = decode_full(
        Dialect::Lr2,
        &format!("{INFO}#CUSTOMOPTION,A,900,x,y\n{body}"),
        &[],
        &s,
    );
    assert_eq!(d.options.contains(&probe), expected);
}

/// SKN-IF-01: すべて真で真
#[rstest]
#[case::single_true(Dialect::Lr2, "#IF,10", true)]
#[case::single_false(Dialect::Lr2, "#IF,11", false)]
#[case::all_true(Dialect::Lr2, "#IF,10,12", true)]
#[case::one_false(Dialect::Lr2, "#IF,10,11", false)]
#[case::lr2_negative_is_false(Dialect::Lr2, "#IF,!11", false)]
#[case::over_999_false(Dialect::Lr2, "#IF,1000", false)]
fn skn_if_01_all_listed_true(#[case] dialect: Dialect, #[case] cond: &str, #[case] expected: bool) {
    let d = with_ops(dialect, &format!("{cond}\n#HIT,1\n#ENDIF\n"), &[10, 12]);
    assert_eq!(directives(&d) == vec!["HIT"], expected);
}

/// SKN-IF-02: 最初に真の腕だけ
#[rstest]
#[case::if_true(vec![10], vec!["ONE"])]
#[case::elseif_true(vec![20], vec!["TWO"])]
#[case::both_true_first_wins(vec![10, 20], vec!["ONE"])]
#[case::else_taken(vec![], vec!["THREE"])]
fn skn_if_02_first_true_arm(#[case] ops: Vec<i32>, #[case] expected: Vec<&str>) {
    let body = "#IF,10\n#ONE,1\n#ELSEIF,20\n#TWO,1\n#ELSE,\n#THREE,1\n#ENDIF\n";
    assert_eq!(directives(&with_ops(Dialect::Lr2, body, &ops)), expected);
}

/// SKN-IF-03: (Lr2) 外側が偽でも内側は自分の条件で判定
#[rstest]
#[case::inner_true_leaks(vec![20], vec!["INNER"])]
#[case::inner_false(vec![], vec![])]
#[case::outer_true(vec![10, 20], vec!["OUTER", "INNER"])]
fn skn_if_03_lr2_nested_quirk(#[case] ops: Vec<i32>, #[case] expected: Vec<&str>) {
    let body = "#IF,10\n#OUTER,1\n#IF,20\n#INNER,1\n#ENDIF\n#ENDIF\n";
    assert_eq!(directives(&with_ops(Dialect::Lr2, body, &ops)), expected);
}

/// SKN-IF-04: (Standard) 正しい入れ子と否定
#[rstest]
#[case::inner_true_but_outer_false(vec![20], "#IF,10\n#IF,20\n#HIT,1\n#ENDIF\n#ENDIF\n", false)]
#[case::both_true(vec![10, 20], "#IF,10\n#IF,20\n#HIT,1\n#ENDIF\n#ENDIF\n", true)]
#[case::negation_true(vec![], "#IF,!10\n#HIT,1\n#ENDIF\n", true)]
#[case::negation_false(vec![10], "#IF,!10\n#HIT,1\n#ENDIF\n", false)]
#[case::else_under_false_parent(vec![], "#IF,10\n#IF,20\n#ELSE\n#HIT,1\n#ENDIF\n#ENDIF\n", false)]
fn skn_if_04_standard_nesting_and_negation(
    #[case] ops: Vec<i32>,
    #[case] body: &str,
    #[case] hit: bool,
) {
    assert_eq!(
        directives(&with_ops(Dialect::Standard, body, &ops)) == vec!["HIT"],
        hit
    );
}

/// SKN-SET-01: #SETOPTION
#[rstest]
#[case::set_true("#SETOPTION,950,1\n", true)]
#[case::set_then_clear("#SETOPTION,950,1\n#SETOPTION,950,0\n", false)]
#[case::out_of_range_ignored("#SETOPTION,1500,1\n#SETOPTION,950,0\n", false)]
fn skn_set_01_setoption(#[case] pre: &str, #[case] hit: bool) {
    let d = lr2(&format!("{pre}#IF,950\n#HIT,1\n#ENDIF\n"));
    assert_eq!(directives(&d) == vec!["HIT"], hit);
}

// ---- SKN-CUS / SKN-IMG / SKN-FNT / SKN-INC ------------------------------------------

const BG_FILES: &[(&str, &[u8])] = &[("bg/blue.png", b""), ("bg/red.png", b"")];

/// SKN-CUS-01: CUSTOMFILE の置換
#[rstest]
#[case::chosen(Some("red"), "bg/*.png", ImageEntry::File("bg/red.png".into()))]
#[case::default_used(None, "bg/*.png", ImageEntry::File("bg/blue.png".into()))]
#[case::error_not_substituted(Some("ERROR"), "bg/*.png", ImageEntry::File("bg/blue.png".into()))]
#[case::prefix_mismatch(Some("red"), "other/*.png", ImageEntry::Missing("other/*.png".into()))]
fn skn_cus_01_custom_file_substitution(
    #[case] choice: Option<&str>,
    #[case] image: &str,
    #[case] expected: ImageEntry,
) {
    let body = format!("{INFO}#CUSTOMFILE,BG,bg/*.png,blue\n#IMAGE,{image}\n");
    let mut s = SkinSettings::default();
    if let Some(c) = choice {
        s.custom.insert(0, CustomChoice::File(c.into()));
    }
    let d = decode_full(Dialect::Lr2, &body, BG_FILES, &s);
    assert_eq!(d.images, vec![expected]);
}

/// SKN-CUS-02: RANDOM はその場で 1 回だけ選ぶ
#[rstest]
#[case::two_images_same_pick(2)]
#[case::three_images_same_pick(3)]
fn skn_cus_02_random_picked_once(#[case] n: usize) {
    let mut body = format!("{INFO}#CUSTOMFILE,BG,bg/*.png,RANDOM\n");
    for _ in 0..n {
        body.push_str("#IMAGE,bg/*.png\n");
    }
    let d = decode_full(Dialect::Lr2, &body, BG_FILES, &SkinSettings::default());
    let first = d.images[0].clone();
    assert!(matches!(first, ImageEntry::File(_)));
    assert!(d.images.iter().all(|i| *i == first));
}

/// SKN-IMG-01: #IMAGE
#[rstest]
#[case::found("#IMAGE,a.png", ImageEntry::File("a.png".into()), 0)]
#[case::continue_kw("#IMAGE,CONTINUE", ImageEntry::Continue, 0)]
#[case::missing("#IMAGE,nope.png", ImageEntry::Missing("nope.png".into()), 1)]
fn skn_img_01_image(#[case] line: &str, #[case] expected: ImageEntry, #[case] diag: usize) {
    let d = decode_full(
        Dialect::Lr2,
        &format!("{line}\n"),
        &[("a.png", b"")],
        &SkinSettings::default(),
    );
    assert_eq!(d.images, vec![expected]);
    assert_eq!(d.diagnostics.len(), diag);
}

/// SKN-IMG-02: `\` → `/`
#[rstest]
#[case::backslash("#IMAGE,dir\\a.png")]
#[case::mixed("#IMAGE,dir/a.png")]
fn skn_img_02_backslash_normalized(#[case] line: &str) {
    let d = decode_full(
        Dialect::Lr2,
        &format!("{line}\n"),
        &[("dir/a.png", b"")],
        &SkinSettings::default(),
    );
    assert_eq!(d.images, vec![ImageEntry::File("dir/a.png".into())]);
}

/// SKN-FNT-01: `#FONT` は `fonts`、`#LR2FONT` は `image_fonts` に別々に並ぶ
#[rstest]
#[case::system_only("#FONT,24,3,1\n", vec![FontEntry::System { size: 24, thickness: 3, kind: 1 }], 0)]
#[case::separate_numbering("#LR2FONT,f.lr2font\n#FONT,24,3,1\n#FONT,10,0,0\n", vec![FontEntry::System { size: 24, thickness: 3, kind: 1 }, FontEntry::System { size: 10, thickness: 0, kind: 0 }], 1)]
fn skn_fnt_01_fonts(#[case] body: &str, #[case] fonts: Vec<FontEntry>, #[case] image_fonts: usize) {
    let d = decode_full(
        Dialect::Lr2,
        body,
        &[("f.lr2font", b"#S,10\n")],
        &SkinSettings::default(),
    );
    assert_eq!(d.fonts, fonts);
    assert_eq!(d.image_fonts.len(), image_fonts);
}

/// SKN-FNT-02: `.lr2font` を読み、画像はフォントファイルと同じフォルダから探す
#[rstest]
#[case::loaded("#LR2FONT,font/a.lr2font\n", ImageFontEntry::Loaded(ImageFont {
    size: 20,
    margin: 1,
    textures: vec![(0, ImageEntry::File("font/a.png".into())), (1, ImageEntry::Missing("font/b.png".into()))],
    glyphs: vec![Glyph { ch: 'A', texture: 0, rect: [1, 2, 3, 4] }],
}), 1)]
#[case::missing("#LR2FONT,font/x.lr2font\n", ImageFontEntry::Missing("font/x.lr2font".into()), 1)]
fn skn_fnt_02_image_font(
    #[case] body: &str,
    #[case] expected: ImageFontEntry,
    #[case] diags: usize,
) {
    let d = decode_full(
        Dialect::Lr2,
        body,
        &[
            (
                "font/a.lr2font",
                b"#S,20\n#M,1\n#T,0,a.png\n#T,1,b.png\n#R,65,0,1,2,3,4\n",
            ),
            ("font/a.png", b""),
        ],
        &SkinSettings::default(),
    );
    assert_eq!(d.image_fonts, vec![expected]);
    assert_eq!(d.diagnostics.len(), diags);
}

/// SKN-LR2F-01: `.lr2font` の命令
#[rstest]
#[case::all("#S,24\n#M,2\n#T,0,a.png\n#R,65,0,1,2,3,4\n", 24, 2, vec![(0, "a.png")], vec![Glyph { ch: 'A', texture: 0, rect: [1, 2, 3, 4] }])]
#[case::later_wins("#S,1\n#S,2\n#T,0,a.png\n#T,0,b\\c.png\n#R,65,0,1,1,1,1\n#R,65,1,2,2,2,2\n", 2, 0, vec![(0, "b/c.png")], vec![Glyph { ch: 'A', texture: 1, rect: [2, 2, 2, 2] }])]
#[case::ignored_lines(" #S,5\nS,6\n#X,1\n#R,-1,0,1,1,1,1\n", 0, 0, vec![], vec![])]
#[case::second_letter_only("#SIZE,9\n", 9, 0, vec![], vec![])]
fn skn_lr2f_01_commands(
    #[case] text: &str,
    #[case] size: i32,
    #[case] margin: i32,
    #[case] textures: Vec<(i32, &str)>,
    #[case] glyphs: Vec<Glyph>,
) {
    let f = lr2font::parse(text);
    assert_eq!((f.size, f.margin), (size, margin));
    assert_eq!(
        f.textures,
        textures
            .into_iter()
            .map(|(i, s)| (i, s.to_string()))
            .collect::<Vec<_>>()
    );
    assert_eq!(f.glyphs, glyphs);
}

/// SKN-LR2F-02: 文字番号
#[rstest]
#[case::ascii(65, Some('A'))]
#[case::half_kana(177, Some('ｱ'))]
#[case::hiragana(608, Some('あ'))]
#[case::kanji(2687, Some('漢'))]
#[case::high_kanji(8223, Some('燿'))]
#[case::undefined_byte(160, None)]
#[case::negative(-1, None)]
#[case::too_large(15307, None)]
fn skn_lr2f_02_char_code(#[case] id: i32, #[case] want: Option<char>) {
    assert_eq!(lr2font::glyph_char(id), want);
}

/// SKN-INC-01: #INCLUDE
#[rstest]
#[case::expanded_in_place("#A_LONG,1\n#INCLUDE,inc.csv\n#C_LONG,1\n", b"#B_LONG,1\n".as_slice(), vec!["A_LONG", "B_LONG", "C_LONG"])]
#[case::if_state_independent("#IF,10\n#INCLUDE,inc.csv\n#ENDIF\n", b"#ENDIF\n#B_LONG,1\n".as_slice(), vec!["B_LONG"])]
#[case::setoption_shared("#INCLUDE,inc.csv\n#IF,950\n#HIT_LONG,1\n#ENDIF\n", b"#SETOPTION,950,1\n".as_slice(), vec!["HIT_LONG"])]
#[case::self_include_stops_at_depth("#INCLUDE,inc.csv\n", b"#INCLUDE,inc.csv\n".as_slice(), vec![])]
fn skn_inc_01_include(#[case] main: &str, #[case] inc: &[u8], #[case] expected: Vec<&str>) {
    let d = decode_full(
        Dialect::Lr2,
        main,
        &[("inc.csv", inc)],
        &ops_settings(&[10]),
    );
    assert_eq!(directives(&d), expected);
}
fn ops_settings(ops: &[i32]) -> SkinSettings {
    SkinSettings {
        options: ops.iter().copied().collect(),
        ..Default::default()
    }
}

// ---- SKN-SRC / SKN-DST / SKN-BLD / SKN-DIR -----------------------------------------

/// SKN-SRC-01: SRC の共通項目
#[rstest]
#[case::basic("#SRC_IMAGE,0,2,10,20,30,40,2,3,100,5", Source { image: ImageRef(2), rect: [10, 20, 30, 40], divide: (2, 3), cycle_ms: 100, timer: Some(TimerRef(5)), params: [0; 5] })]
#[case::div_min_1_and_negative_xy("#SRC_IMAGE,0,1,-5,-5,-1,-1,0,0,0,0", Source { image: ImageRef(1), rect: [0, 0, -1, -1], divide: (1, 1), cycle_ms: 0, timer: Some(TimerRef(0)), params: [0; 5] })]
fn skn_src_01_common_fields(#[case] line: &str, #[case] expected: Source) {
    let d = lr2(&format!("{line}\n"));
    assert_eq!(d.elements[0].source, Some(expected));
}

/// SKN-SRC-02: 種別ごとの引数
#[rstest]
#[case::image("IMAGE", ElementKind::Image)]
#[case::number("NUMBER", ElementKind::Number { value: ValueRef(7), align: 8, digits: 9 })]
#[case::slider("SLIDER", ElementKind::Slider { direction: 7, range: 8, value: ValueRef(9), disabled: true })]
#[case::bargraph("BARGRAPH", ElementKind::Bargraph { value: ValueRef(7), direction: 8 })]
#[case::button("BUTTON", ElementKind::Button { action: 7, click: 8, panel: 9, plus_only: true })]
#[case::other("NOTE", ElementKind::Other)]
fn skn_src_02_kind_params(#[case] name: &str, #[case] expected: ElementKind) {
    let d = lr2(&format!("#SRC_{name},0,0,0,0,1,1,1,1,0,0,7,8,9,1,0\n"));
    assert_eq!(d.elements[0].kind, expected);
}

/// SKN-SRC-03: SRC_TEXT
#[rstest]
#[case::editable("#SRC_TEXT,0,2,10,1,1", FontRef(2), ValueRef(10), 1, true)]
#[case::plain("#SRC_TEXT,0,0,12,0", FontRef(0), ValueRef(12), 0, false)]
fn skn_src_03_text(
    #[case] line: &str,
    #[case] font: FontRef,
    #[case] value: ValueRef,
    #[case] align: i32,
    #[case] editable: bool,
) {
    let d = lr2(&format!("{line}\n"));
    assert_eq!(
        d.elements[0].kind,
        ElementKind::Text {
            font,
            value,
            align,
            editable
        }
    );
    assert_eq!(d.elements[0].source, None);
}

/// SKN-DST-01: DST の対応付け
#[rstest]
#[case::latest_same_name(format!("{IMG}{IMG}#DST_IMAGE,0,0,1,1,1,1\n"), vec![0, 1])]
#[case::same_index_preferred("#SRC_NOTE,1,0,0,0,1,1,1,1,0,0\n#SRC_NOTE,2,0,0,0,1,1,1,1,0,0\n#DST_NOTE,1,0,1,1,1,1\n".to_string(), vec![1, 0])]
#[case::no_src_ignored("#DST_IMAGE,0,0,1,1,1,1\n".to_string(), vec![])]
fn skn_dst_01_attach_to_src(#[case] body: String, #[case] keyframes: Vec<usize>) {
    let d = lr2(&body);
    assert_eq!(
        d.elements
            .iter()
            .map(|e| e.keyframes.len())
            .collect::<Vec<_>>(),
        keyframes
    );
}

/// SKN-DST-02: 最初の DST の属性
#[rstest]
#[case::first_line_only(
    "#DST_IMAGE,0,0,0,0,1,1,0,255,255,255,255,1,0,0,0,500,41,900,-901,0\n#DST_IMAGE,0,100,0,0,1,1,0,255,255,255,255,1,0,0,0,1,2,3,4,5,6\n",
    Some(500), Some(TimerRef(41)), vec![ConditionRef(900), ConditionRef(-901)], [0, 0]
)]
#[case::zeros_dropped("#DST_IMAGE,0,0,0,0,1,1,0,255,255,255,255,1,0,0,0,0,0,0,0,0\n", Some(0), Some(TimerRef(0)), vec![], [0, 0])]
#[case::op4_op5_not_conditions("#DST_IMAGE,0,0,0,0,1,1,0,255,255,255,255,1,0,0,0,0,0,0,0,32,1,2\n", Some(0), Some(TimerRef(0)), vec![ConditionRef(32)], [1, 2])]
fn skn_dst_02_first_dst_attributes(
    #[case] dsts: &str,
    #[case] looping: Option<i32>,
    #[case] timer: Option<TimerRef>,
    #[case] conditions: Vec<ConditionRef>,
    #[case] extra: [i32; 2],
) {
    let e = &lr2(&format!("{IMG}{dsts}")).elements[0];
    assert_eq!(
        (e.looping, e.timer, e.conditions.clone(), e.dst_extra),
        (looping, timer, conditions, extra)
    );
}

/// SKN-DST-03: キーフレーム項目
#[rstest]
#[case::full(
    "#DST_IMAGE,0,250,1,2,3,4,5,128,64,32,16,2,1,90,4",
    Keyframe { time_ms: 250, rect: [1.0, 2.0, 3.0, 4.0], accel: 5, argb: [128, 64, 32, 16], blend: Blend::Add, filter: true, angle: 90.0, center: 4 }
)]
#[case::color_clamped(
    "#DST_IMAGE,0,0,0,0,0,0,0,300,-1,255,0,1,0,0,0",
    Keyframe { time_ms: 0, rect: [0.0; 4], accel: 0, argb: [255, 0, 255, 0], blend: Blend::Alpha, filter: false, angle: 0.0, center: 0 }
)]
fn skn_dst_03_keyframe_fields(#[case] line: &str, #[case] expected: Keyframe) {
    assert_eq!(
        lr2(&format!("{IMG}{line}\n")).elements[0].keyframes,
        vec![expected]
    );
}

/// SKN-DST-04: 接頭辞一致の SRC を複製
#[rstest]
#[case::bar_body_on("#SRC_BAR_BODY,0,0,0,0,1,1,1,1,0,0\n#DST_BAR_BODY_ON,0,0,1,1,1,1\n", vec!["BAR_BODY", "BAR_BODY_ON"])]
#[case::exact_name_preferred("#SRC_BAR_BODY,0,0,0,0,1,1,1,1,0,0\n#DST_BAR_BODY,0,0,1,1,1,1\n", vec!["BAR_BODY"])]
fn skn_dst_04_prefix_fallback(#[case] body: &str, #[case] commands: Vec<&str>) {
    let d = lr2(body);
    assert_eq!(
        d.elements
            .iter()
            .map(|e| e.command.as_str())
            .collect::<Vec<_>>(),
        commands
    );
}

/// SKN-DST-05: 描画順は最初の DST 行の順番（飛ばした行・#IF 系の行は数えない）
#[rstest]
#[case::src_then_dst(
    "#SRC_IMAGE,0,0,0,0,1,1,1,1,0,0\n#SRC_IMAGE,1,0,0,0,1,1,1,1,0,0\n#DST_IMAGE,1,0,0,0,1,1\n#DST_IMAGE,0,0,0,0,1,1\n#DST_IMAGE,1,9,0,0,1,1\n",
    vec![4, 3]
)]
#[case::skipped_lines_not_counted(
    "#SRC_IMAGE,0,0,0,0,1,1,1,1,0,0\n#IF,999\n#SETOPTION,900,1\n#ENDIF\n#DST_IMAGE,0,0,0,0,1,1\n",
    vec![2]
)]
fn skn_dst_05_order(#[case] body: &str, #[case] orders: Vec<u32>) {
    let d = lr2(body);
    assert_eq!(
        d.elements.iter().map(|e| e.order).collect::<Vec<_>>(),
        orders
    );
}

/// SKN-BLD-01: 合成番号
#[rstest]
#[case::none(0, Blend::None, 0)]
#[case::alpha(1, Blend::Alpha, 0)]
#[case::add(2, Blend::Add, 0)]
#[case::sub(3, Blend::Subtract, 0)]
#[case::mul(4, Blend::Multiply, 0)]
#[case::sub2(5, Blend::Subtract2, 0)]
#[case::inv_dest(9, Blend::InvertDest, 0)]
#[case::inv_src(10, Blend::InvertSrc, 0)]
#[case::mul_alpha(11, Blend::MultiplyAlpha, 0)]
#[case::unknown_6(6, Blend::Alpha, 1)]
fn skn_bld_01_blend(#[case] n: i32, #[case] expected: Blend, #[case] diag: usize) {
    let d = lr2(&format!(
        "{IMG}#DST_IMAGE,0,0,0,0,1,1,0,255,255,255,255,{n},0,0,0\n"
    ));
    assert_eq!(d.elements[0].keyframes[0].blend, expected);
    assert_eq!(d.diagnostics.len(), diag);
}

/// SKN-DIR-01: その他の命令は生のまま
#[rstest]
#[case::startinput("#STARTINPUT,500", "STARTINPUT", vec!["500"])]
#[case::scenetime("#SCENETIME,3000", "SCENETIME", vec!["3000"])]
#[case::no_args("#RELOADBANNER", "RELOADBANNER", vec![])]
fn skn_dir_01_directives(#[case] line: &str, #[case] command: &str, #[case] args: Vec<&str>) {
    let d = standard(&format!("{line}\n"));
    assert_eq!(
        d.directives,
        vec![Directive {
            command: command.into(),
            args: args.into_iter().map(String::from).collect()
        }]
    );
}
