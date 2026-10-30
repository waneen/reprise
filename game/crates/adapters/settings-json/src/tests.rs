//! 仕様ルール（lib.rs の表）とテスト関数は 1 対 1。関数名の接頭辞がルール ID。
use super::*;
use rstest::rstest;

fn file(dir: &tempfile::TempDir) -> JsonSettingsFile {
    JsonSettingsFile {
        path: dir.path().join("sub/config.json"),
    }
}

/// SJS-01: ファイルが無ければ None
#[rstest]
#[case::missing_file("config.json")]
#[case::missing_dir("none/config.json")]
fn sjs_01_missing(#[case] rel: &str) {
    let t = tempfile::tempdir().unwrap();
    let f = JsonSettingsFile {
        path: t.path().join(rel),
    };
    assert_eq!(f.load().unwrap(), None);
}

/// SJS-02: 保存して読み直すと同じ（オートは保存しない）
#[rstest]
#[case::defaults(Settings::default())]
#[case::everything({
    let mut s = Settings {
        songs: vec!["/a".into(), "C:\\b".into()],
        skin: Some("LR2files/Theme/x/play.csv".into()),
        skin_root: Some("/lr2".into()),
        gauge: GaugeType::Hazard,
        ..Default::default()
    };
    s.options.hi_speed = 250;
    s.options.hs_fix = HsFix::Constant;
    s.options.lane_cover = true;
    s.options.cover = 30;
    s.options.judge_offset = -12;
    s.options.auto = true;
    s.bindings.analog_scratch_ms = Some(80);
    s.bindings.bind(0, Source::Axis { axis: "LeftStickX".into(), plus: true });
    s.bindings.bind(2, Source::Button("Other(3)".into()));
    s
})]
fn sjs_02_round_trip(#[case] s: Settings) {
    let t = tempfile::tempdir().unwrap();
    let f = file(&t);
    f.save(&s).unwrap();
    let mut want = s.clone();
    want.options.auto = false;
    assert_eq!(f.load().unwrap(), Some(want));
}

/// SJS-03: 書かれていない項目は既定値、知らない項目は無視、割り当ては書かれた操作だけ
#[rstest]
#[case::empty("{}", Settings::default())]
#[case::unknown_ignored(r#"{"zzz": 1, "hi_speed": 300}"#, { let mut s = Settings::default(); s.options.hi_speed = 300; s })]
#[case::clamped(r#"{"hi_speed": 5000}"#, { let mut s = Settings::default(); s.options.hi_speed = 900; s })]
#[case::one_slot(r#"{"keys": {"key1": [{"button": "South"}]}}"#, { let mut s = Settings::default(); s.bindings.slots[2] = vec![Source::Button("South".into())]; s })]
#[case::null_skin(r#"{"skin": null}"#, Settings::default())]
fn sjs_03_defaults(#[case] text: &str, #[case] want: Settings) {
    let v: Value = serde_json::from_str(text).unwrap();
    assert_eq!(from_json(&v).unwrap(), want);
}

/// SJS-04: 読めないものはエラー
#[rstest]
#[case::not_json("{", true)]
#[case::not_object("[]", false)]
#[case::bad_gauge(r#"{"gauge": "exhard"}"#, false)]
#[case::bad_hs_fix(r#"{"hs_fix": 3}"#, false)]
#[case::bad_source(r#"{"keys": {"start": [{"mouse": 1}]}}"#, false)]
#[case::bad_type(r#"{"hi_speed": "fast"}"#, false)]
fn sjs_04_errors(#[case] text: &str, #[case] via_file: bool) {
    let t = tempfile::tempdir().unwrap();
    let f = file(&t);
    std::fs::create_dir_all(f.path.parent().unwrap()).unwrap();
    std::fs::write(&f.path, text).unwrap();
    assert!(matches!(f.load(), Err(PortError::Parse(_))));
    if !via_file {
        let v: Value = serde_json::from_str(text).unwrap();
        assert!(matches!(from_json(&v), Err(PortError::Parse(_))));
    }
}
