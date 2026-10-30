//! 仕様ルール（lib.rs の表）とテスト関数は 1 対 1。関数名の接頭辞がルール ID。
use super::*;
use rc::{Block, ChartDocument, DecodeError, Diagnostic, RawEvent, ResolvePolicy, Statement};
use rstest::rstest;

/// 決まった文書を返すデコーダ
struct Fixed(ChartDocument);
impl ChartDecoder for Fixed {
    fn name(&self) -> &'static str {
        "fixed"
    }
    fn extensions(&self) -> &'static [&'static str] {
        &["bms", "pms"]
    }
    fn decode(
        &self,
        _b: &[u8],
        _h: &DecodeHint,
    ) -> Result<(ChartDocument, Vec<Diagnostic>), DecodeError> {
        Ok((self.0.clone(), vec![]))
    }
}

fn doc(layout: rc::KeyLayout, stmts: Vec<Statement>) -> ChartDocument {
    ChartDocument {
        layout,
        policy: ResolvePolicy::default(),
        body: vec![Block::Statements(stmts)],
    }
}
fn ev(e: RawEvent) -> Statement {
    Statement::Event(e)
}
fn pos(m: u32, f: f64) -> rc::Position {
    rc::Position::new(m, f)
}
fn load(d: ChartDocument) -> Chart {
    DecoderLoader(Fixed(d))
        .load(Path::new("a.bms"), b"", &mut LegacyMt::new(1))
        .unwrap()
}
fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

/// ADP-CH-01: 拡張子の照合
#[rstest]
#[case::lower("a.bms", true)]
#[case::upper("a.BMS", true)]
#[case::other_listed("a.pms", true)]
#[case::not_listed("a.bmson", false)]
#[case::no_extension("bms", false)]
fn adp_ch_01_extension_match(#[case] path: &str, #[case] expected: bool) {
    let l = DecoderLoader(Fixed(doc(rc::KeyLayout::Beat7, vec![])));
    assert_eq!(l.can_load(Path::new(path)), expected);
}

/// ADP-CH-02: MD5 と SHA-256（標準のテストベクタ）
#[rstest]
#[case::empty(b"".as_slice(), "d41d8cd98f00b204e9800998ecf8427e", "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855")]
#[case::abc(b"abc".as_slice(), "900150983cd24fb0d6963f7d28e17f72", "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad")]
fn adp_ch_02_hash(#[case] bytes: &[u8], #[case] md5: &str, #[case] sha256: &str) {
    let h = hash(bytes);
    assert_eq!(
        (hex(&h.md5), hex(&h.sha256.unwrap())),
        (md5.to_string(), sha256.to_string())
    );
}

/// ADP-CH-03: 位置→拍
#[rstest]
#[case::start(vec![], (0, 0.0), 0.0)]
#[case::measure1(vec![], (1, 0.0), 4.0)]
#[case::half_of_measure2(vec![], (2, 0.5), 10.0)]
#[case::after_half_length_measure(vec![(1, 0.5)], (2, 0.0), 6.0)]
#[case::inside_long_measure(vec![(1, 2.0)], (1, 0.5), 8.0)]
fn adp_ch_03_position_to_beat(
    #[case] lengths: Vec<(u32, f64)>,
    #[case] at: (u32, f64),
    #[case] beat: f64,
) {
    let mut s: Vec<Statement> = lengths
        .into_iter()
        .map(|(measure, ratio)| ev(RawEvent::MeasureLength { measure, ratio }))
        .collect();
    s.push(ev(RawEvent::Bgm {
        pos: pos(at.0, at.1),
        id: rc::ObjId(1),
    }));
    assert_eq!(load(doc(rc::KeyLayout::Beat7, s)).bgm[0].0, Beat(beat));
}

/// ADP-CH-04: STOP の単位変換
#[rstest]
#[case::one_beat(48.0, 1.0)]
#[case::whole_measure(192.0, 4.0)]
#[case::eighth(24.0, 0.5)]
fn adp_ch_04_stop_to_beats(#[case] d: f64, #[case] beats: f64) {
    let s = vec![
        Statement::Define(rc::Definition::Stop(rc::ObjId(1), d)),
        ev(RawEvent::StopRef {
            pos: pos(1, 0.0),
            id: rc::ObjId(1),
        }),
    ];
    assert_eq!(
        load(doc(rc::KeyLayout::Beat7, s)).timing,
        vec![TimingEvent::Stop {
            beat: Beat(4.0),
            beats
        }]
    );
}

/// ADP-CH-05: 鍵盤構成
#[rstest]
#[case::k5(rc::KeyLayout::Beat5, KeyMode::K5)]
#[case::k7(rc::KeyLayout::Beat7, KeyMode::K7)]
#[case::k10(rc::KeyLayout::Beat10, KeyMode::K10)]
#[case::k14(rc::KeyLayout::Beat14, KeyMode::K14)]
#[case::k9(rc::KeyLayout::Popn9, KeyMode::K9)]
fn adp_ch_05_key_mode(#[case] layout: rc::KeyLayout, #[case] expected: KeyMode) {
    assert_eq!(load(doc(layout, vec![])).key_mode, expected);
}

/// ADP-CH-06: 写像
#[rstest]
#[case::normal(rc::RawObject::Normal, NoteKind::Normal, Some(SoundId(5)))]
#[case::invisible(rc::RawObject::Invisible, NoteKind::Invisible, Some(SoundId(5)))]
#[case::mine(rc::RawObject::Mine, NoteKind::Mine { damage_raw: 5 }, None)]
fn adp_ch_06_note_mapping(
    #[case] object: rc::RawObject,
    #[case] kind: NoteKind,
    #[case] sound: Option<SoundId>,
) {
    let s = vec![
        ev(RawEvent::Note {
            pos: pos(1, 0.5),
            lane: rc::Lane(3),
            object,
            id: rc::ObjId(5),
        }),
        Statement::Define(rc::Definition::Sound(rc::ObjId(5), "k.wav".into())),
        Statement::Define(rc::Definition::Meta(rc::MetaField::Title("T".into()))),
        Statement::Define(rc::Definition::Meta(rc::MetaField::Subartist("S".into()))),
        Statement::Define(rc::Definition::Meta(rc::MetaField::JudgeRankPercent(80.0))),
    ];
    let c = load(doc(rc::KeyLayout::Beat7, s));
    assert_eq!(
        c.notes,
        vec![Note {
            beat: Beat(6.0),
            lane: Lane(3),
            kind,
            sound
        }]
    );
    assert_eq!(c.sounds, vec![(SoundId(5), "k.wav".to_string())]);
    assert_eq!(c.meta.title, "T");
    assert_eq!(c.meta.subartist, "S");
    assert_eq!(c.meta.judge_rank_percent, Some(80.0));
}

/// ADP-CH-07: #RANDOM は LegacyMt（GetRand(n-1)+1）で引く
#[rstest]
#[case::seed_a(1)]
#[case::seed_b(2)]
#[case::seed_c(391)]
fn adp_ch_07_random_uses_legacy_mt(#[case] seed: u32) {
    let arms = (1..=4)
        .map(|k| {
            (
                k,
                vec![Block::Statements(vec![ev(RawEvent::Bgm {
                    pos: pos(1, 0.0),
                    id: rc::ObjId(k),
                })])],
            )
        })
        .collect();
    let d = ChartDocument {
        layout: rc::KeyLayout::Beat7,
        policy: ResolvePolicy::default(),
        body: vec![Block::Random {
            generator: rc::Generator::Draw { max: 4 },
            body: vec![Block::If {
                arms,
                otherwise: None,
            }],
        }],
    };
    let c = DecoderLoader(Fixed(d))
        .load(Path::new("a.bms"), b"", &mut LegacyMt::new(seed))
        .unwrap();
    let expected = LegacyMt::new(seed).get_rand(3) + 1;
    assert_eq!(c.bgm, vec![(Beat(4.0), SoundId(expected))]);
}

/// ADP-CH-08: 小節長は Measure として出す
#[rstest]
#[case::one(vec![(3, 0.75)], vec![TimingEvent::Measure { index: 3, length: 0.75 }])]
#[case::sorted(vec![(5, 2.0), (1, 0.5)], vec![TimingEvent::Measure { index: 1, length: 0.5 }, TimingEvent::Measure { index: 5, length: 2.0 }])]
fn adp_ch_08_measure_events(#[case] lengths: Vec<(u32, f64)>, #[case] expected: Vec<TimingEvent>) {
    let s = lengths
        .into_iter()
        .map(|(measure, ratio)| ev(RawEvent::MeasureLength { measure, ratio }))
        .collect();
    assert_eq!(load(doc(rc::KeyLayout::Beat7, s)).timing, expected);
}

/// ADP-CH-09: 音ファイルの解決（書かれた名前 → 拡張子違い ogg/wav/flac/mp3 → 1 つ上のフォルダ、大小無視）
#[rstest]
#[case::exact(&["song/a.wav"], "a.wav", Some("song/a.wav"))]
#[case::case_insensitive(&["song/A.WAV"], "a.wav", Some("song/A.WAV"))]
#[case::wav_to_ogg(&["song/a.ogg"], "a.wav", Some("song/a.ogg"))]
#[case::ogg_before_flac(&["song/a.flac", "song/a.ogg"], "a.wav", Some("song/a.ogg"))]
#[case::no_extension(&["song/a.mp3"], "a", Some("song/a.mp3"))]
#[case::subdir_backslash(&["song/snd/a.wav"], "snd\\a.wav", Some("song/snd/a.wav"))]
#[case::parent_dir(&["a.wav"], "a.wav", Some("a.wav"))]
#[case::missing(&["song/b.wav"], "a.wav", None)]
fn adp_ch_09_find_sound_file(
    #[case] files: &[&str],
    #[case] name: &str,
    #[case] want: Option<&str>,
) {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(tmp.path().join("song")).unwrap();
    for f in files {
        let p = tmp.path().join(f);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, b"x").unwrap();
    }
    let got = find_sound_file(&tmp.path().join("song"), name);
    assert_eq!(got, want.map(|w| tmp.path().join(w)));
}
