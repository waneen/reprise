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
        select_skin: Some("LR2files/Theme/x/select.csv".into()),
        keyconfig_skin: Some("LR2files/Theme/x/keyconfig.csv".into()),
        result_skin: Some("LR2files/Theme/x/result.csv".into()),
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
    s.bindings.set(Side::P1, 0, 1, Some(Source::Axis { axis: "LeftStickX".into(), plus: true }));
    s.bindings.set(Side::P1, 2, 3, Some(Source::Button("Other(3)".into())));
    s.bindings.set(Side::P2, 9, 0, Some(Source::Key("Enter".into())));
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
#[case::one_slot(r#"{"keys": {"key1": [{"button": "South"}]}}"#, { let mut s = Settings::default(); s.bindings.slots[2] = vec![Some(Source::Button("South".into()))]; s })]
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

/// SJS-05: 選曲スキン
#[rstest]
#[case::path(
    r#"{"select_skin": "LR2files/Theme/x/select.csv"}"#,
    Some(PathBuf::from("LR2files/Theme/x/select.csv"))
)]
#[case::null(r#"{"select_skin": null}"#, None)]
#[case::missing("{}", None)]
fn sjs_05_select_skin(#[case] text: &str, #[case] want: Option<PathBuf>) {
    let v: Value = serde_json::from_str(text).unwrap();
    let s = from_json(&v).unwrap();
    assert_eq!(s.select_skin, want);
    assert_eq!(
        to_json(&s)["select_skin"],
        v.get("select_skin").cloned().unwrap_or(Value::Null)
    );
}

/// SJS-06: キー設定スキン
#[rstest]
#[case::path(
    r#"{"keyconfig_skin": "LR2files/Theme/x/keyconfig.csv"}"#,
    Some(PathBuf::from("LR2files/Theme/x/keyconfig.csv"))
)]
#[case::null(r#"{"keyconfig_skin": null}"#, None)]
#[case::missing("{}", None)]
fn sjs_06_keyconfig_skin(#[case] text: &str, #[case] want: Option<PathBuf>) {
    let v: Value = serde_json::from_str(text).unwrap();
    let s = from_json(&v).unwrap();
    assert_eq!(s.keyconfig_skin, want);
    assert_eq!(
        to_json(&s)["keyconfig_skin"],
        v.get("keyconfig_skin").cloned().unwrap_or(Value::Null)
    );
}

/// SJS-08: リザルトスキン
#[rstest]
#[case::path(
    r#"{"result_skin": "LR2files/Theme/x/result.csv"}"#,
    Some(PathBuf::from("LR2files/Theme/x/result.csv"))
)]
#[case::null(r#"{"result_skin": null}"#, None)]
#[case::missing("{}", None)]
fn sjs_08_result_skin(#[case] text: &str, #[case] want: Option<PathBuf>) {
    let v: Value = serde_json::from_str(text).unwrap();
    let s = from_json(&v).unwrap();
    assert_eq!(s.result_skin, want);
    assert_eq!(
        to_json(&s)["result_skin"],
        v.get("result_skin").cloned().unwrap_or(Value::Null)
    );
}

fn k(name: &str) -> Option<Source> {
    Some(Source::Key(name.into()))
}

/// SJS-07: 割り当ての位置（null は空き、16 番目より後は無視）と 2P
#[rstest]
#[case::holes(r#"{"keys": {"key1": [null, {"key": "KeyA"}]}}"#, Side::P1, 2, vec![None, k("KeyA")])]
#[case::trailing_null(r#"{"keys": {"key1": [{"key": "KeyA"}, null]}}"#, Side::P1, 2, vec![k("KeyA")])]
#[case::empty(r#"{"keys": {"key1": []}}"#, Side::P1, 2, vec![])]
#[case::beyond_16(&format!(r#"{{"keys": {{"key1": [{}{{"key": "KeyB"}}]}}}}"#, "null, ".repeat(16)), Side::P1, 2, vec![])]
#[case::p2(r#"{"keys": {"p2_select": [{"button": "East"}]}}"#, Side::P2, 10, vec![Some(Source::Button("East".into()))])]
fn sjs_07_places(
    #[case] text: &str,
    #[case] side: Side,
    #[case] slot: usize,
    #[case] want: Vec<Option<Source>>,
) {
    let v: Value = serde_json::from_str(text).unwrap();
    let s = from_json(&v).unwrap();
    let got = if side == Side::P1 {
        &s.bindings.slots[slot]
    } else {
        &s.bindings.p2[slot]
    };
    assert_eq!(got, &want);
    let back = from_json(&to_json(&s)).unwrap();
    assert_eq!(back.bindings, s.bindings);
    let name = if side == Side::P1 {
        "key1"
    } else {
        "p2_select"
    };
    assert_eq!(
        to_json(&s)["keys"][name].as_array().unwrap().len(),
        want.len()
    );
}
