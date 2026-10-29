//! 仕様ルール（lib.rs の表）とテスト関数は 1 対 1。関数名の接頭辞がルール ID。
use super::*;
use rstest::rstest;
use serde_json::json;

struct NoRng;
impl RandomSource for NoRng {
    fn next_inclusive(&mut self, _max: u32) -> u32 {
        0
    }
}

/// 最小の bmson に `patch` を重ねる（`info` はキー単位で上書き）
fn bmson(patch: Value) -> Value {
    let mut base = json!({
        "version": "1.0.0",
        "info": { "title": "T", "init_bpm": 120.0 },
        "sound_channels": [],
    });
    for (k, v) in patch.as_object().unwrap() {
        if k == "info" {
            for (ik, iv) in v.as_object().unwrap() {
                base["info"][ik] = iv.clone();
            }
        } else {
            base[k] = v.clone();
        }
    }
    base
}
fn decode(v: &Value) -> Result<(ChartDocument, Vec<Diagnostic>), DecodeError> {
    BmsonDecoder.decode(v.to_string().as_bytes(), &DecodeHint::default())
}
fn resolve(v: Value) -> ResolvedChart {
    decode(&v).expect("decode").0.resolve(&mut NoRng)
}
fn notes(r: &ResolvedChart) -> Vec<(u8, Position, NoteKind)> {
    r.events
        .iter()
        .filter_map(|e| match e {
            Event::Note {
                lane, pos, kind, ..
            } => Some((lane.0, *pos, kind.clone())),
            _ => None,
        })
        .collect()
}
fn one_note(x: i64, mode: &str) -> Value {
    bmson(json!({
        "info": { "mode_hint": mode },
        "sound_channels": [{ "name": "a.wav", "notes": [{ "x": x, "y": 0, "l": 0, "c": false }] }],
    }))
}
fn p(m: u32, f: f64) -> Position {
    Position::new(m, f)
}

/// BMSON-JSN-01: JSON の読み取り
#[rstest]
#[case::plain(b"{\"version\":\"1.0.0\",\"info\":{\"init_bpm\":120}}".to_vec(), true)]
#[case::bom(b"\xEF\xBB\xBF{\"version\":\"1.0.0\",\"info\":{\"init_bpm\":120}}".to_vec(), true)]
#[case::broken(b"{\"version\":\"1.0.0\",\n\"info\":".to_vec(), false)]
fn bmson_jsn_01_json(#[case] bytes: Vec<u8>, #[case] ok: bool) {
    let r = BmsonDecoder.decode(&bytes, &DecodeHint::default());
    match ok {
        true => assert!(r.is_ok()),
        false => assert!(matches!(r, Err(DecodeError::Syntax { line: 2, .. }))),
    }
}

/// BMSON-VER-01: version
#[rstest]
#[case::v1(json!("1.0.0"), true)]
#[case::v1_minor(json!("1.2.0"), true)]
#[case::v2(json!("2.0.0"), false)]
#[case::null(Value::Null, false)]
fn bmson_ver_01_version(#[case] version: Value, #[case] ok: bool) {
    let mut v = bmson(json!({}));
    v["version"] = version;
    assert_eq!(decode(&v).is_ok(), ok);
    let mut legacy = bmson(json!({}));
    legacy.as_object_mut().unwrap().remove("version");
    assert!(decode(&legacy).is_err());
}

/// BMSON-INF-01: メタ情報
#[rstest]
#[case::title(json!({"title": "曲"}), |m: &Metadata| m.title == "曲")]
#[case::subtitle(json!({"subtitle": "sub"}), |m: &Metadata| m.subtitle == "sub")]
#[case::artist(json!({"artist": "a"}), |m: &Metadata| m.artist == "a")]
#[case::genre(json!({"genre": "g"}), |m: &Metadata| m.genre == "g")]
#[case::chart_name(json!({"chart_name": "HYPER"}), |m: &Metadata| m.chart_name == "HYPER")]
#[case::level(json!({"level": 12}), |m: &Metadata| m.play_level == Some(12))]
#[case::back_image(json!({"back_image": "b.png"}), |m: &Metadata| m.back_bmp.as_deref() == Some("b.png"))]
#[case::eyecatch(json!({"eyecatch_image": "e.png"}), |m: &Metadata| m.stage_file.as_deref() == Some("e.png"))]
#[case::banner(json!({"banner_image": "n.png"}), |m: &Metadata| m.banner.as_deref() == Some("n.png"))]
#[case::preview(json!({"preview_music": "p.ogg"}), |m: &Metadata| m.preview.as_deref() == Some("p.ogg"))]
#[case::empty_image_ignored(json!({"banner_image": ""}), |m: &Metadata| m.banner.is_none())]
fn bmson_inf_01_metadata(#[case] info: Value, #[case] check: fn(&Metadata) -> bool) {
    assert!(check(&resolve(bmson(json!({ "info": info }))).meta));
}

/// BMSON-INF-02: init_bpm は必須
#[rstest]
#[case::present(Some(150.5), true)]
#[case::missing(None, false)]
fn bmson_inf_02_init_bpm_required(#[case] bpm: Option<f64>, #[case] ok: bool) {
    let mut v = bmson(json!({}));
    match bpm {
        Some(b) => v["info"]["init_bpm"] = json!(b),
        None => {
            v["info"].as_object_mut().unwrap().remove("init_bpm");
        }
    }
    let r = decode(&v);
    assert_eq!(r.is_ok(), ok);
    if let Some(b) = bpm {
        assert_eq!(r.unwrap().0.resolve(&mut NoRng).meta.initial_bpm, Some(b));
    }
}

/// BMSON-INF-03: judge_rank / total
#[rstest]
#[case::defaults(json!({}), Some(100.0), Some(100.0))]
#[case::given(json!({"judge_rank": 80, "total": 300}), Some(80.0), Some(300.0))]
#[case::negative_total_abs(json!({"total": -150}), Some(100.0), Some(150.0))]
fn bmson_inf_03_rank_and_total(
    #[case] info: Value,
    #[case] judge: Option<f64>,
    #[case] total: Option<f64>,
) {
    let m = resolve(bmson(json!({ "info": info }))).meta;
    assert_eq!((m.judge_rank_percent, m.total_percent), (judge, total));
}

/// BMSON-RES-01: resolution
#[rstest]
#[case::default(None, 960)]
#[case::zero(Some(json!(0)), 960)]
#[case::null(Some(Value::Null), 960)]
#[case::r480(Some(json!(480)), 1920)]
#[case::negative(Some(json!(-480)), 1920)]
fn bmson_res_01_resolution(#[case] res: Option<Value>, #[case] pulses_per_measure: i64) {
    let mut v = bmson(json!({
        "sound_channels": [{ "name": "a.wav", "notes": [{ "x": 0, "y": pulses_per_measure, "l": 0, "c": false }] }],
    }));
    if let Some(r) = res {
        v["info"]["resolution"] = r;
    }
    let r = resolve(v);
    assert_eq!(r.events[0].position(), p(1, 0.0));
}

/// BMSON-POS-01: 小節線
#[rstest]
#[case::no_lines(None, 1440, p(1, 0.5))]
#[case::empty_lines(Some(json!([])), 1440, p(1, 0.5))]
#[case::with_lines(Some(json!([{"y": 0}, {"y": 480}, {"y": 1440}])), 960, p(1, 0.5))]
#[case::zero_added(Some(json!([{"y": 480}])), 240, p(0, 0.5))]
#[case::after_last_line_is_4_4(Some(json!([{"y": 0}, {"y": 480}])), 1440, p(2, 0.0))]
fn bmson_pos_01_bar_lines(
    #[case] lines: Option<Value>,
    #[case] y: i64,
    #[case] expected: Position,
) {
    let mut v = bmson(json!({
        "sound_channels": [{ "name": "a.wav", "notes": [{ "x": 0, "y": y, "l": 0, "c": false }] }],
    }));
    if let Some(l) = lines {
        v["lines"] = l;
    }
    assert_eq!(
        resolve(v).events.iter().last().unwrap().position(),
        expected
    );
}

/// BMSON-POS-02: 4/4 でない小節は小節長
#[rstest]
#[case::half_and_double(json!([{"y": 0}, {"y": 480}, {"y": 2400}]), vec![(0, 0.5), (1, 2.0)])]
#[case::all_4_4(json!([{"y": 0}, {"y": 960}]), vec![])]
fn bmson_pos_02_measure_lengths(#[case] lines: Value, #[case] expected: Vec<(u32, f64)>) {
    let r = resolve(bmson(json!({ "lines": lines })));
    let got: Vec<(u32, f64)> = r
        .events
        .iter()
        .filter_map(|e| match e {
            Event::MeasureLength { measure, ratio } => Some((*measure, *ratio)),
            _ => None,
        })
        .collect();
    assert_eq!(got, expected);
}

/// BMSON-MOD-01: mode_hint → 鍵盤構成
#[rstest]
#[case::default(None, Some(KeyLayout::Beat7))]
#[case::beat5(Some("beat-5k"), Some(KeyLayout::Beat5))]
#[case::beat10(Some("beat-10k"), Some(KeyLayout::Beat10))]
#[case::beat14(Some("beat-14k"), Some(KeyLayout::Beat14))]
#[case::popn5(Some("popn-5k"), Some(KeyLayout::Popn9))]
#[case::popn9(Some("popn-9k"), Some(KeyLayout::Popn9))]
#[case::keyboard_unsupported(Some("keyboard-24k"), None)]
fn bmson_mod_01_layout(#[case] mode: Option<&str>, #[case] expected: Option<KeyLayout>) {
    let mut v = bmson(json!({}));
    if let Some(m) = mode {
        v["info"]["mode_hint"] = json!(m);
    }
    assert_eq!(decode(&v).ok().map(|d| d.0.layout), expected);
}

/// BMSON-MOD-02: レーン
#[rstest]
#[case::k7_key1("beat-7k", 1, Some(1))]
#[case::k7_key7("beat-7k", 7, Some(7))]
#[case::k7_scratch("beat-7k", 8, Some(0))]
#[case::k7_no_2p("beat-7k", 9, None)]
#[case::k5_key6_invalid("beat-5k", 6, None)]
#[case::k14_2p_key1("beat-14k", 9, Some(11))]
#[case::k14_2p_key7("beat-14k", 15, Some(17))]
#[case::k14_2p_scratch("beat-14k", 16, Some(10))]
#[case::k10_2p_key5("beat-10k", 13, Some(15))]
#[case::k10_2p_key6_invalid("beat-10k", 14, None)]
#[case::popn9_9("popn-9k", 9, Some(9))]
#[case::popn5_6_invalid("popn-5k", 6, None)]
fn bmson_mod_02_lanes(#[case] mode: &str, #[case] x: i64, #[case] lane: Option<u8>) {
    let d = decode(&one_note(x, mode)).unwrap();
    let r = d.0.resolve(&mut NoRng);
    assert_eq!(notes(&r).first().map(|n| n.0), lane);
    assert_eq!(d.1.is_empty(), lane.is_some());
}

/// BMSON-NOT-01: BGM と鍵盤ノーツ
#[rstest]
#[case::x0_bgm(json!({"x": 0, "y": 0, "l": 0, "c": false}), true)]
#[case::x_null_bgm(json!({"x": null, "y": 0, "l": 0, "c": false}), true)]
#[case::x_missing_bgm(json!({"y": 0, "l": 0, "c": false}), true)]
#[case::x1_playable(json!({"x": 1, "y": 0, "l": 0, "c": false}), false)]
fn bmson_not_01_bgm_or_playable(#[case] note: Value, #[case] bgm: bool) {
    let r = resolve(bmson(json!({
        "sound_channels": [{ "name": "a.wav", "notes": [note] }],
    })));
    let expected = match bgm {
        true => Event::Bgm {
            pos: p(0, 0.0),
            sound: ObjId(1),
        },
        false => Event::Note {
            pos: p(0, 0.0),
            lane: Lane(1),
            kind: NoteKind::Normal,
            sound: Some(ObjId(1)),
        },
    };
    assert_eq!(r.events, vec![expected]);
    assert_eq!(r.sounds.get(&ObjId(1)).map(String::as_str), Some("a.wav"));
}

/// BMSON-NOT-02: l > 0 は LN
#[rstest]
#[case::one_beat(240, NoteKind::Long { end: p(0, 0.25), end_sound: None })]
#[case::across_measure(1200, NoteKind::Long { end: p(1, 0.25), end_sound: None })]
#[case::zero_is_normal(0, NoteKind::Normal)]
fn bmson_not_02_long_notes(#[case] l: i64, #[case] kind: NoteKind) {
    let r = resolve(bmson(json!({
        "sound_channels": [{ "name": "a.wav", "notes": [{"x": 1, "y": 0, "l": l, "c": false}] }],
    })));
    assert_eq!(notes(&r), vec![(1, p(0, 0.0), kind)]);
}

/// BMSON-SND-01: 音声パス
#[rstest]
#[case::plain("a.wav", true)]
#[case::subdir("snd/a.wav", true)]
#[case::backslash_subdir("snd\\a.wav", true)]
#[case::absolute_unix("/etc/a.wav", false)]
#[case::absolute_windows("C:\\a.wav", false)]
#[case::parent("../a.wav", false)]
#[case::nul("a\u{0}.wav", false)]
fn bmson_snd_01_sound_paths(#[case] name: &str, #[case] kept: bool) {
    let (doc, diags) = decode(&bmson(json!({
        "sound_channels": [{ "name": name, "notes": [{"x": 0, "y": 0, "l": 0, "c": false}] }],
    })))
    .unwrap();
    let r = doc.resolve(&mut NoRng);
    assert_eq!(
        (r.sounds.len(), r.events.len(), diags.len()),
        match kept {
            true => (1, 1, 0),
            false => (0, 0, 1),
        }
    );
}

/// BMSON-BPM-01: BPM 変化
#[rstest]
#[case::single(json!([{"y": 960, "bpm": 180.0}]), vec![(p(1, 0.0), 180.0)])]
#[case::same_position_document_order(json!([{"y": 0, "bpm": 90.0}, {"y": 0, "bpm": 200.0}]), vec![(p(0, 0.0), 90.0), (p(0, 0.0), 200.0)])]
fn bmson_bpm_01_bpm_events(#[case] evs: Value, #[case] expected: Vec<(Position, f64)>) {
    let r = resolve(bmson(json!({ "bpm_events": evs })));
    let got: Vec<(Position, f64)> = r
        .events
        .iter()
        .filter_map(|e| match e {
            Event::Bpm { pos, bpm } => Some((*pos, *bpm)),
            _ => None,
        })
        .collect();
    assert_eq!(got, expected);
}

/// BMSON-STP-01: STOP の単位
#[rstest]
#[case::one_beat_r240(240, None, 48.0)]
#[case::whole_measure(960, None, 192.0)]
#[case::r480_one_beat(480, Some(480), 48.0)]
fn bmson_stp_01_stop_duration(#[case] d: i64, #[case] res: Option<i64>, #[case] expected: f64) {
    let mut v = bmson(json!({ "stop_events": [{"y": 0, "duration": d}] }));
    if let Some(r) = res {
        v["info"]["resolution"] = json!(r);
    }
    assert_eq!(
        resolve(v).events,
        vec![Event::Stop {
            pos: p(0, 0.0),
            duration_192th: expected
        }]
    );
}

/// BMSON-BGA-01: BGA
#[rstest]
#[case::base("bga_events", BgaLayer::Base)]
#[case::layer("layer_events", BgaLayer::Layer)]
#[case::poor("poor_events", BgaLayer::Poor)]
fn bmson_bga_01_bga(#[case] key: &str, #[case] layer: BgaLayer) {
    let mut bga = json!({ "bga_header": [{"id": 3, "name": "bg.png"}] });
    bga[key] = json!([{"y": 960, "id": 3}]);
    let r = resolve(bmson(json!({ "bga": bga })));
    assert_eq!(r.images.get(&ObjId(3)).map(String::as_str), Some("bg.png"));
    assert_eq!(
        r.events,
        vec![Event::Bga {
            pos: p(1, 0.0),
            layer,
            image: ObjId(3)
        }]
    );
}

/// BMSON-ORD-01: 同じ位置はノーツ・BGA → BPM → STOP
#[rstest]
#[case::written_reversed(json!({
    "stop_events": [{"y": 0, "duration": 240}],
    "bpm_events": [{"y": 0, "bpm": 200.0}],
    "sound_channels": [{"name": "a.wav", "notes": [{"x": 1, "y": 0, "l": 0, "c": false}]}],
}))]
fn bmson_ord_01_same_pulse_order(#[case] patch: Value) {
    let r = resolve(bmson(patch));
    let kinds: Vec<&str> = r
        .events
        .iter()
        .map(|e| match e {
            Event::Note { .. } => "note",
            Event::Bpm { .. } => "bpm",
            Event::Stop { .. } => "stop",
            _ => "other",
        })
        .collect();
    assert_eq!(kinds, vec!["note", "bpm", "stop"]);
}
