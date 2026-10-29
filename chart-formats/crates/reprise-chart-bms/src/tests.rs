//! 仕様ルール（lib.rs の表）とテスト関数は 1 対 1。関数名の接頭辞がルール ID。
use super::*;
use rstest::rstest;

// ---- テスト用の部品 ----------------------------------------------------------

/// 決められた値を順に返す乱数（尽きたら 0）
struct Seq(Vec<u32>);
impl RandomSource for Seq {
    fn next_inclusive(&mut self, _max: u32) -> u32 {
        if self.0.is_empty() {
            0
        } else {
            self.0.remove(0)
        }
    }
}

fn decode_with(src: &[u8], dialect: Dialect, ext: &str) -> (ChartDocument, Vec<Diagnostic>) {
    let d = BmsDecoder::new(BmsOptions {
        dialect,
        ..Default::default()
    });
    let hint = DecodeHint {
        extension: Some(ext.into()),
    };
    d.decode(src, &hint).expect("decode")
}
fn resolve(src: &str, dialect: Dialect, rng: &[u32]) -> ResolvedChart {
    decode_with(src.as_bytes(), dialect, "bms")
        .0
        .resolve(&mut Seq(rng.to_vec()))
}
fn lr2(src: &str) -> ResolvedChart {
    resolve(src, Dialect::Lr2, &[])
}
fn diags(src: &str, dialect: Dialect) -> Vec<Diagnostic> {
    decode_with(src.as_bytes(), dialect, "bms").1
}
fn bgm(r: &ResolvedChart) -> Vec<(u32, f64, u32)> {
    r.events
        .iter()
        .filter_map(|e| match e {
            Event::Bgm { pos, sound } => Some((pos.measure, pos.fraction, sound.0)),
            _ => None,
        })
        .collect()
}
fn notes(r: &ResolvedChart) -> Vec<(u8, NoteKind)> {
    r.events
        .iter()
        .filter_map(|e| match e {
            Event::Note { lane, kind, .. } => Some((lane.0, kind.clone())),
            _ => None,
        })
        .collect()
}
fn lanes(r: &ResolvedChart) -> Vec<u8> {
    notes(r).into_iter().map(|n| n.0).collect()
}

// ---- BMS-ENC ----------------------------------------------------------------

/// BMS-ENC-01: BOM 付き UTF-8
#[rstest]
#[case::japanese("曲名")]
#[case::ascii("Title")]
fn bms_enc_01_utf8_bom(#[case] title: &str) {
    let mut src = b"\xEF\xBB\xBF".to_vec();
    src.extend(format!("#TITLE {title}\n").as_bytes());
    let r = decode_with(&src, Dialect::Lr2, "bms")
        .0
        .resolve(&mut Seq(vec![]));
    assert_eq!(r.meta.title, title);
}

/// BMS-ENC-02: BOM 無し UTF-8
#[rstest]
#[case::japanese("曲名")]
#[case::emoji("♪")]
fn bms_enc_02_valid_utf8_without_bom(#[case] title: &str) {
    let r = lr2(&format!("#TITLE {title}\n"));
    assert_eq!(r.meta.title, title);
}

/// BMS-ENC-03: それ以外は Shift_JIS
#[rstest]
#[case::kanji(b"\x8B\xC8\x96\xBC".as_slice(), "曲名")] // 「曲名」
#[case::halfwidth_kana(b"\xB6\xC5".as_slice(), "ｶﾅ")]
fn bms_enc_03_fallback_shift_jis(#[case] sjis: &[u8], #[case] expected: &str) {
    let mut src = b"#TITLE ".to_vec();
    src.extend(sjis);
    src.push(b'\n');
    let r = decode_with(&src, Dialect::Lr2, "bms")
        .0
        .resolve(&mut Seq(vec![]));
    assert_eq!(r.meta.title, expected);
}

// ---- BMS-LEX ----------------------------------------------------------------

/// BMS-LEX-01: `#` で始まらない行は無視
#[rstest]
#[case::comment("comment #00101:01")]
#[case::empty("")]
#[case::semicolon("; #00101:01")]
fn bms_lex_01_non_hash_lines_ignored(#[case] line: &str) {
    assert!(bgm(&lr2(&format!("{line}\n"))).is_empty());
}

/// BMS-LEX-02: ヘッダ名は大小無視、値は最初の空白の後
#[rstest]
#[case::upper("#TITLE abc", "abc")]
#[case::lower("#title abc", "abc")]
#[case::tab_separator("#TITLE\tabc", "abc")]
#[case::value_with_spaces("#TITLE a b  c", "a b  c")]
#[case::leading_spaces_line("   #TITLE abc  ", "abc")]
fn bms_lex_02_header_key_and_value(#[case] line: &str, #[case] expected: &str) {
    assert_eq!(lr2(line).meta.title, expected);
}

/// BMS-LEX-03: オブジェクト列の位置
#[rstest]
#[case::single("#00101:01", vec![(1, 0.0, 1)])]
#[case::quarters("#00201:01000200", vec![(2, 0.0, 1), (2, 0.5, 2)])]
#[case::odd_tail_ignored("#00101:0102X", vec![(1, 0.0, 1), (1, 0.5, 2)])]
#[case::spaces_ignored("#00101:01 02", vec![(1, 0.0, 1), (1, 0.5, 2)])]
fn bms_lex_03_object_positions(#[case] line: &str, #[case] expected: Vec<(u32, f64, u32)>) {
    assert_eq!(bgm(&lr2(line)), expected);
}

/// BMS-LEX-04: 36 進 / `#BASE 62`
#[rstest]
#[case::base36_upper("#00101:ZZ", 35 * 36 + 35)]
#[case::base36_lower_is_same("#00101:zz", 35 * 36 + 35)]
#[case::base62_lower("#BASE 62\n#00101:zz", 61 * 62 + 61)]
#[case::base62_upper("#BASE 62\n#00101:ZZ", 35 * 62 + 35)]
fn bms_lex_04_id_radix(#[case] src: &str, #[case] expected: u32) {
    assert_eq!(bgm(&lr2(src))[0].2, expected);
}

/// BMS-LEX-05: `00` は空
#[rstest]
#[case::all_empty("#00101:0000", 0)]
#[case::one_of_two("#00101:0001", 1)]
fn bms_lex_05_zero_is_empty(#[case] line: &str, #[case] count: usize) {
    assert_eq!(bgm(&lr2(line)).len(), count);
}

// ---- BMS-CH -----------------------------------------------------------------

/// BMS-CH-01: 同じ小節の BGM 行は積み重なる
#[rstest]
#[case::one_line("#00101:01", 1)]
#[case::two_lines_same_measure("#00101:01\n#00101:02", 2)]
fn bms_ch_01_bgm_lines_accumulate(#[case] src: &str, #[case] count: usize) {
    assert_eq!(bgm(&lr2(src)).len(), count);
}

/// BMS-CH-02: 小節長
#[rstest]
#[case::half("#00102:0.5", Some(0.5))]
#[case::integer("#00102:2", Some(2.0))]
#[case::zero_ignored("#00102:0", None)]
#[case::negative_ignored("#00102:-1", None)]
fn bms_ch_02_measure_length(#[case] line: &str, #[case] expected: Option<f64>) {
    let got = lr2(line).events.iter().find_map(|e| match e {
        Event::MeasureLength { ratio, .. } => Some(*ratio),
        _ => None,
    });
    assert_eq!(got, expected);
}

/// BMS-CH-03: 16 進 BPM
#[rstest]
#[case::hex_ff("#00103:FF", 255.0)]
#[case::hex_78("#00103:78", 120.0)]
fn bms_ch_03_hex_bpm(#[case] line: &str, #[case] bpm: f64) {
    assert_eq!(
        lr2(line).events[0],
        Event::Bpm {
            pos: Position::new(1, 0.0),
            bpm
        }
    );
}

/// BMS-CH-04: ch08/ch09 は定義参照
#[rstest]
#[case::exbpm("#BPM01 155.5\n#00108:01", Event::Bpm { pos: Position::new(1, 0.0), bpm: 155.5 })]
#[case::exbpm_alias("#EXBPM01 90\n#00108:01", Event::Bpm { pos: Position::new(1, 0.0), bpm: 90.0 })]
#[case::stop("#STOP01 96\n#00109:01", Event::Stop { pos: Position::new(1, 0.0), duration_192th: 96.0 })]
fn bms_ch_04_reference_channels(#[case] src: &str, #[case] expected: Event) {
    assert_eq!(lr2(src).events[0], expected);
}

/// BMS-CH-05: BGA レイヤ
#[rstest]
#[case::base("04", BgaLayer::Base)]
#[case::poor("06", BgaLayer::Poor)]
#[case::layer("07", BgaLayer::Layer)]
fn bms_ch_05_bga_layers(#[case] ch: &str, #[case] layer: BgaLayer) {
    let r = lr2(&format!("#001{ch}:01"));
    assert_eq!(
        r.events[0],
        Event::Bga {
            pos: Position::new(1, 0.0),
            layer,
            image: ObjId(1)
        }
    );
}

/// BMS-CH-06: BMS のレーン割り当て
#[rstest]
#[case::key1("11", Some(1))]
#[case::key5("15", Some(5))]
#[case::scratch("16", Some(0))]
#[case::free_zone_dropped("17", None)]
#[case::key6("18", Some(6))]
#[case::key7("19", Some(7))]
#[case::p2_key1("21", Some(11))]
#[case::p2_scratch("26", Some(10))]
#[case::p2_key7("29", Some(17))]
fn bms_ch_06_bms_lanes(#[case] ch: &str, #[case] lane: Option<u8>) {
    let r = lr2(&format!("#001{ch}:01"));
    assert_eq!(lanes(&r).first().copied(), lane);
}

/// BMS-CH-07: PMS のレーン割り当て
#[rstest]
#[case::k1("11", Some(1))]
#[case::k5("15", Some(5))]
#[case::bme_type_18("18", Some(6))]
#[case::bme_type_19("19", Some(7))]
#[case::bme_type_16("16", Some(8))]
#[case::bme_type_17("17", Some(9))]
#[case::k6("22", Some(6))]
#[case::k9("25", Some(9))]
#[case::p2_21_unused("21", None)]
fn bms_ch_07_pms_lanes(#[case] ch: &str, #[case] lane: Option<u8>) {
    let r = decode_with(format!("#001{ch}:01").as_bytes(), Dialect::Lr2, "pms")
        .0
        .resolve(&mut Seq(vec![]));
    assert_eq!(lanes(&r).first().copied(), lane);
}

/// BMS-CH-08: オブジェクト種別
#[rstest]
#[case::normal_1p("11", 1, NoteKind::Normal)]
#[case::normal_2p("21", 11, NoteKind::Normal)]
#[case::invisible_1p("31", 1, NoteKind::Invisible)]
#[case::invisible_2p("41", 11, NoteKind::Invisible)]
#[case::mine_1p("D1", 1, NoteKind::Mine { damage_raw: 36 + 10 })]
#[case::mine_2p("E1", 11, NoteKind::Mine { damage_raw: 36 + 10 })]
fn bms_ch_08_object_kinds(#[case] ch: &str, #[case] lane: u8, #[case] kind: NoteKind) {
    let r = lr2(&format!("#001{ch}:1A"));
    assert_eq!(notes(&r), vec![(lane, kind)]);
}

/// BMS-CH-10: 5x/6x は LN
#[rstest]
#[case::ln_1p("51", 1)]
#[case::ln_2p("61", 11)]
fn bms_ch_10_ln_channels(#[case] ch: &str, #[case] lane: u8) {
    let r = lr2(&format!("#001{ch}:0102"));
    assert_eq!(
        notes(&r),
        vec![(
            lane,
            NoteKind::Long {
                end: Position::new(1, 0.5),
                end_sound: Some(ObjId(2))
            }
        )]
    );
}

/// BMS-CH-09: 未対応チャンネルは診断して無視（チャンネルごとに 1 回）
#[rstest]
#[case::scroll("#001SC:01", 1)]
#[case::ch05_twice("#00105:01\n#00205:01", 1)]
#[case::two_kinds("#00105:01\n#001SC:01", 2)]
fn bms_ch_09_unsupported_channels(#[case] src: &str, #[case] diag_count: usize) {
    assert!(lr2(src).events.is_empty());
    let d = diags(src, Dialect::Lr2);
    assert_eq!(
        d.iter()
            .filter(|d| d.message.contains("チャンネル"))
            .count(),
        diag_count
    );
}

// ---- BMS-HDR ----------------------------------------------------------------

/// BMS-HDR-01: メタ情報
#[rstest]
#[case::title("#TITLE t", |m: &Metadata| m.title == "t")]
#[case::subtitle("#SUBTITLE s", |m: &Metadata| m.subtitle == "s")]
#[case::artist("#ARTIST a", |m: &Metadata| m.artist == "a")]
#[case::subartist("#SUBARTIST b", |m: &Metadata| m.subartist == "b")]
#[case::genre("#GENRE g", |m: &Metadata| m.genre == "g")]
#[case::bpm("#BPM 150.5", |m: &Metadata| m.initial_bpm == Some(150.5))]
#[case::playlevel("#PLAYLEVEL 12", |m: &Metadata| m.play_level == Some(12))]
#[case::difficulty("#DIFFICULTY 4", |m: &Metadata| m.difficulty == Some(4))]
#[case::rank("#RANK 2", |m: &Metadata| m.rank == Some(2))]
#[case::stagefile("#STAGEFILE s.png", |m: &Metadata| m.stage_file.as_deref() == Some("s.png"))]
#[case::banner("#BANNER b.png", |m: &Metadata| m.banner.as_deref() == Some("b.png"))]
#[case::backbmp("#BACKBMP k.png", |m: &Metadata| m.back_bmp.as_deref() == Some("k.png"))]
#[case::preview("#PREVIEW p.ogg", |m: &Metadata| m.preview.as_deref() == Some("p.ogg"))]
fn bms_hdr_01_metadata(#[case] line: &str, #[case] check: fn(&Metadata) -> bool) {
    assert!(check(&lr2(line).meta));
}

/// BMS-HDR-02: 定義ヘッダ
#[rstest]
#[case::wav("#WAV0A kick.wav", |r: &ResolvedChart| r.sounds.get(&ObjId(10)).map(String::as_str) == Some("kick.wav"))]
#[case::wav_with_space("#WAV01 my kick.wav", |r: &ResolvedChart| r.sounds.get(&ObjId(1)).map(String::as_str) == Some("my kick.wav"))]
#[case::bmp("#BMP01 bg.bmp", |r: &ResolvedChart| r.images.get(&ObjId(1)).map(String::as_str) == Some("bg.bmp"))]
#[case::stop_zero_rejected("#STOP01 0\n#00109:01", |r: &ResolvedChart| r.events.is_empty())]
fn bms_hdr_02_definitions(#[case] src: &str, #[case] check: fn(&ResolvedChart) -> bool) {
    assert!(check(&lr2(src)));
}

/// BMS-HDR-03: `#LNOBJ`
#[rstest]
#[case::lnobj_closes("#LNOBJ ZZ\n#00111:01ZZ", vec![(1, NoteKind::Long { end: Position::new(1, 0.5), end_sound: Some(ObjId(1295)) })])]
#[case::without_lnobj("#00111:01ZZ", vec![(1, NoteKind::Normal), (1, NoteKind::Normal)])]
fn bms_hdr_03_lnobj(#[case] src: &str, #[case] expected: Vec<(u8, NoteKind)>) {
    assert_eq!(notes(&lr2(src)), expected);
}

/// BMS-HDR-04: `#TOTAL` の数値解釈
#[rstest]
#[case::lr2_truncates(Dialect::Lr2, "#TOTAL 300.9", 300.0)]
#[case::standard_keeps_fraction(Dialect::Standard, "#TOTAL 300.9", 300.9)]
fn bms_hdr_04_total(#[case] dialect: Dialect, #[case] line: &str, #[case] expected: f64) {
    assert_eq!(resolve(line, dialect, &[]).meta.total, Some(expected));
}

/// BMS-HDR-05: 未知のヘッダは診断のみ
#[rstest]
#[case::unknown("#FOOBAR 1")]
#[case::defexrank("#DEFEXRANK 100")]
fn bms_hdr_05_unknown_header_diagnostic(#[case] line: &str) {
    assert!(lr2(line).events.is_empty());
    assert!(diags(line, Dialect::Lr2)
        .iter()
        .any(|d| d.message.contains("未対応のヘッダ")));
}

// ---- BMS-RND（LR2 方言） ------------------------------------------------------

const RND_SRC: &str =
    "#RANDOM 2\n#00101:09\n#IF 1\n#00101:01\n#ENDIF\n#IF 2\n#00101:02\n#ENDIF\n#00101:03\n";

/// BMS-RND-L2-01: #RANDOM 後、最初の #IF までは捨てる
#[rstest]
#[case::rng0_branch1(0, vec![1, 3])]
#[case::rng1_branch2(1, vec![2, 3])]
fn bms_rnd_l2_01_lines_before_first_if_dropped(#[case] rng: u32, #[case] expected: Vec<u32>) {
    let ids: Vec<u32> = bgm(&resolve(RND_SRC, Dialect::Lr2, &[rng]))
        .iter()
        .map(|b| b.2)
        .collect();
    assert_eq!(ids, expected);
}

/// BMS-RND-L2-02: #IF は開いている腕を閉じ、#ENDIF 後は無条件
#[rstest]
#[case::if_without_endif_switches_arm(
    "#RANDOM 2\n#IF 1\n#00101:01\n#IF 2\n#00101:02\n#ENDIF\n#00101:03",
    0,
    vec![1, 3]
)]
#[case::after_endif_unconditional("#RANDOM 2\n#IF 2\n#00101:02\n#ENDIF\n#00101:03", 0, vec![3])]
fn bms_rnd_l2_02_if_and_endif(#[case] src: &str, #[case] rng: u32, #[case] expected: Vec<u32>) {
    let ids: Vec<u32> = bgm(&resolve(src, Dialect::Lr2, &[rng]))
        .iter()
        .map(|b| b.2)
        .collect();
    assert_eq!(ids, expected);
}

/// BMS-RND-L2-03: #ELSE 等は無視（#ELSE 以降も同じ腕に含まれる）
#[rstest]
#[case::else_branch_merged_when_true(0, vec![1, 2])]
#[case::both_dropped_when_false(1, vec![])]
fn bms_rnd_l2_03_else_ignored(#[case] rng: u32, #[case] expected: Vec<u32>) {
    let src = "#RANDOM 2\n#IF 1\n#00101:01\n#ELSE\n#00101:02\n#ENDIF";
    let ids: Vec<u32> = bgm(&resolve(src, Dialect::Lr2, &[rng]))
        .iter()
        .map(|b| b.2)
        .collect();
    assert_eq!(ids, expected);
    assert!(diags(src, Dialect::Lr2).iter().any(|d| d.line == Some(4)));
}

/// BMS-RND-L2-04: 入れ子は無く、#IF 内の #RANDOM は前の乱数を閉じる
#[rstest]
#[case::outer1_inner1(vec![0, 0], vec![1, 11])]
#[case::outer2_still_draws_inner(vec![1, 1], vec![12])]
fn bms_rnd_l2_04_no_nesting(#[case] rng: Vec<u32>, #[case] expected: Vec<u32>) {
    // LR2 では 2 つ目の #RANDOM は外側の #IF の中にあっても常に引かれる
    let src = "#RANDOM 2\n#IF 1\n#00101:01\n#RANDOM 2\n#IF 1\n#00101:0B\n#ENDIF\n#IF 2\n#00101:0C\n#ENDIF\n#ENDIF";
    let ids: Vec<u32> = bgm(&resolve(src, Dialect::Lr2, &rng))
        .iter()
        .map(|b| b.2)
        .collect();
    assert_eq!(ids, expected);
}

// ---- BMS-RND（Standard 方言） -------------------------------------------------

/// BMS-RND-STD-01: 入れ子・ELSEIF・ELSE・SETRANDOM
#[rstest]
#[case::elseif_hit("#RANDOM 3\n#IF 1\n#00101:01\n#ELSEIF 2\n#00101:02\n#ELSE\n#00101:03\n#ENDIF\n#ENDRANDOM", vec![1], vec![2])]
#[case::else_hit("#RANDOM 3\n#IF 1\n#00101:01\n#ELSEIF 2\n#00101:02\n#ELSE\n#00101:03\n#ENDIF\n#ENDRANDOM", vec![2], vec![3])]
#[case::nested_skipped_when_outer_false("#RANDOM 2\n#IF 1\n#RANDOM 2\n#IF 1\n#00101:0B\n#ENDIF\n#ENDRANDOM\n#ENDIF\n#ENDRANDOM", vec![1], vec![])]
#[case::setrandom_fixed("#SETRANDOM 2\n#IF 2\n#00101:02\n#ENDIF\n#ENDRANDOM", vec![], vec![2])]
fn bms_rnd_std_01_structured_random(
    #[case] src: &str,
    #[case] rng: Vec<u32>,
    #[case] expected: Vec<u32>,
) {
    let ids: Vec<u32> = bgm(&resolve(src, Dialect::Standard, &rng))
        .iter()
        .map(|b| b.2)
        .collect();
    assert_eq!(ids, expected);
}

/// BMS-RND-STD-02: #RANDOM と最初の #IF の間は無条件
#[rstest]
#[case::kept(0, vec![9, 1])]
#[case::kept_other_branch(1, vec![9])]
fn bms_rnd_std_02_lines_before_if_kept(#[case] rng: u32, #[case] expected: Vec<u32>) {
    let src = "#RANDOM 2\n#00101:09\n#IF 1\n#00101:01\n#ENDIF\n#ENDRANDOM";
    let ids: Vec<u32> = bgm(&resolve(src, Dialect::Standard, &[rng]))
        .iter()
        .map(|b| b.2)
        .collect();
    assert_eq!(ids, expected);
}

// ---- BMS-LAY / BMS-POL --------------------------------------------------------

/// BMS-LAY-01: 鍵盤構成の判定
#[rstest]
#[case::five("#00111:01\n#00116:01", "bms", KeyLayout::Beat5)]
#[case::seven_by_18("#00118:01", "bme", KeyLayout::Beat7)]
#[case::seven_by_ln_19("#00159:0101", "bms", KeyLayout::Beat7)]
#[case::ten("#00121:01", "bms", KeyLayout::Beat10)]
#[case::fourteen("#00121:01\n#00129:01", "bms", KeyLayout::Beat14)]
#[case::free_zone_does_not_make_seven("#00117:01", "bms", KeyLayout::Beat5)]
#[case::empty_data_ignored("#00118:00", "bms", KeyLayout::Beat5)]
#[case::pms_extension("#00111:01", "pms", KeyLayout::Popn9)]
fn bms_lay_01_layout_detection(#[case] src: &str, #[case] ext: &str, #[case] expected: KeyLayout) {
    assert_eq!(
        decode_with(src.as_bytes(), Dialect::Lr2, ext).0.layout,
        expected
    );
}

/// BMS-POL-01: 方言 → 解決ポリシー
#[rstest]
#[case::lr2(Dialect::Lr2, ResolvePolicy::LR2)]
#[case::standard(Dialect::Standard, ResolvePolicy::default())]
fn bms_pol_01_dialect_policy(#[case] dialect: Dialect, #[case] expected: ResolvePolicy) {
    assert_eq!(decode_with(b"", dialect, "bms").0.policy, expected);
}
