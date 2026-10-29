//! 仕様ルール（lib.rs の表）とテスト関数は 1 対 1。関数名の接頭辞がルール ID。
use super::*;
use rstest::{fixture, rstest};
use std::fs;
use tempfile::TempDir;

fn element(command: &str, index: i32, kind: sc::ElementKind) -> sc::Element {
    sc::Element {
        command: command.into(),
        index,
        kind,
        source: None,
        timer: None,
        conditions: vec![],
        looping: None,
        keyframes: vec![],
    }
}
fn doc(kind: i32, images: Vec<sc::ImageEntry>, elements: Vec<sc::Element>) -> sc::SkinDocument {
    sc::SkinDocument {
        header: sc::SkinHeader {
            kind,
            ..Default::default()
        },
        images,
        fonts: vec![],
        elements,
        directives: vec![],
        diagnostics: vec![],
    }
}

/// ADP-SK-01: シーン種別
#[rstest]
#[case::play7(0, d::SceneKind::Play7)]
#[case::play9(4, d::SceneKind::Play9)]
#[case::select(5, d::SceneKind::Select)]
#[case::result(7, d::SceneKind::Result)]
#[case::course_result(15, d::SceneKind::CourseResult)]
#[case::other(17, d::SceneKind::Other(17))]
fn adp_sk_01_scene_kind(#[case] kind: i32, #[case] expected: d::SceneKind) {
    assert_eq!(map_skin(doc(kind, vec![], vec![])).scene, expected);
}

/// ADP-SK-02: 要素種別
#[rstest]
#[case::image("IMAGE", 0, sc::ElementKind::Image, d::ElementKind::Image)]
#[case::number("NUMBER", 0, sc::ElementKind::Number { value: sc::ValueRef(3), align: 1, digits: 4 }, d::ElementKind::Number { value_ref: 3, digits: 4, align: 1 })]
#[case::text("TEXT", 0, sc::ElementKind::Text { font: sc::FontRef(2), value: sc::ValueRef(10), align: 0, editable: false }, d::ElementKind::Text { value_ref: 10, font: 2, align: 0 })]
#[case::slider("SLIDER", 0, sc::ElementKind::Slider { direction: 2, range: 100, value: sc::ValueRef(1), disabled: false }, d::ElementKind::Slider { value_ref: 1, direction: 2, range: 100 })]
#[case::bargraph("BARGRAPH", 0, sc::ElementKind::Bargraph { value: sc::ValueRef(5), direction: 1 }, d::ElementKind::Bar { value_ref: 5, direction: 1 })]
#[case::button("BUTTON", 0, sc::ElementKind::Button { action: 7, click: 0, panel: 0, plus_only: false }, d::ElementKind::Button { action: 7 })]
#[case::note("NOTE", 3, sc::ElementKind::Other, d::ElementKind::NoteLane { lane: 3 })]
#[case::other("JUDGELINE", 0, sc::ElementKind::Other, d::ElementKind::Other { command: "JUDGELINE".into(), index: 0 })]
fn adp_sk_02_element_kind(
    #[case] command: &str,
    #[case] index: i32,
    #[case] kind: sc::ElementKind,
    #[case] expected: d::ElementKind,
) {
    let s = map_skin(doc(0, vec![], vec![element(command, index, kind)]));
    assert_eq!(s.elements[0].kind, expected);
}

/// ADP-SK-03: 画像
#[rstest]
#[case::file(sc::ImageEntry::File("a.png".into()), Some(PathBuf::from("a.png")))]
#[case::continue_kw(sc::ImageEntry::Continue, None)]
#[case::missing(sc::ImageEntry::Missing("x.png".into()), None)]
fn adp_sk_03_images(#[case] entry: sc::ImageEntry, #[case] expected: Option<PathBuf>) {
    assert_eq!(map_skin(doc(0, vec![entry], vec![])).images, vec![expected]);
}

/// ADP-SK-04: キーフレーム等
#[rstest]
#[case::with_timer(Some(sc::TimerRef(41)), d::TimerId(41), sc::Blend::Add, d::Blend::Add)]
#[case::no_timer_is_zero(None, d::TimerId(0), sc::Blend::InvertDest, d::Blend::InvDest)]
fn adp_sk_04_keyframes_and_attributes(
    #[case] timer: Option<sc::TimerRef>,
    #[case] expected_timer: d::TimerId,
    #[case] b: sc::Blend,
    #[case] expected_blend: d::Blend,
) {
    let mut e = element("IMAGE", 0, sc::ElementKind::Image);
    e.timer = timer;
    e.conditions = vec![sc::ConditionRef(900), sc::ConditionRef(-901)];
    e.looping = Some(500);
    e.keyframes = vec![sc::Keyframe {
        time_ms: 100,
        rect: [1.0, 2.0, 3.0, 4.0],
        argb: [255, 1, 2, 3],
        angle: 45.0,
        center: 5,
        blend: b,
        filter: true,
        accel: 2,
    }];
    let got = &map_skin(doc(0, vec![], vec![e])).elements[0];
    assert_eq!(got.timer, expected_timer);
    assert_eq!(got.conditions, vec![d::OptionId(900), d::OptionId(-901)]);
    assert_eq!(got.looping, Some(500));
    assert_eq!(
        got.keyframes,
        vec![d::Keyframe {
            t_ms: 100,
            dst: [1.0, 2.0, 3.0, 4.0],
            color: [255, 1, 2, 3],
            angle: 45.0,
            center: 5,
            blend: expected_blend,
            accel: 2,
            filter: true
        }]
    );
}

// ---- FsResolver ----

#[fixture]
fn tree() -> TempDir {
    let t = tempfile::tempdir().unwrap();
    fs::create_dir_all(t.path().join("LR2files/Theme/Skin/bg")).unwrap();
    for f in ["a.png", "b.png", "c.png", "notes.txt"] {
        fs::write(t.path().join("LR2files/Theme/Skin/bg").join(f), b"x").unwrap();
    }
    fs::write(t.path().join("LR2files/Theme/Skin/Play.csv"), b"#").unwrap();
    t
}
fn rel(t: &TempDir, p: &Path) -> String {
    p.strip_prefix(t.path())
        .unwrap()
        .to_string_lossy()
        .replace('\\', "/")
}

/// ADP-SK-05: root 基準・大小無視
#[rstest]
#[case::exact("LR2files/Theme/Skin/Play.csv", "LR2files/Theme/Skin/Play.csv")]
#[case::case_insensitive("lr2files/THEME/skin/play.CSV", "LR2files/Theme/Skin/Play.csv")]
#[case::backslash("LR2files\\Theme\\Skin\\Play.csv", "LR2files/Theme/Skin/Play.csv")]
fn adp_sk_05_resolve_relative_case_insensitive(
    tree: TempDir,
    #[case] pattern: &str,
    #[case] expected: &str,
) {
    let mut rng = LegacyMt::new(1);
    let mut r = FsResolver {
        root: tree.path().to_path_buf(),
        rng: &mut rng,
    };
    let got = r.resolve(pattern).unwrap();
    assert_eq!(rel(&tree, &got), expected);
}

/// ADP-SK-06: `*` は LegacyMt で選ぶ
#[rstest]
#[case::seed_1(1)]
#[case::seed_2(2)]
#[case::seed_391(391)]
fn adp_sk_06_wildcard_picked_by_legacy_mt(tree: TempDir, #[case] seed: u32) {
    let mut rng = LegacyMt::new(seed);
    let mut r = FsResolver {
        root: tree.path().to_path_buf(),
        rng: &mut rng,
    };
    let got = r.resolve("LR2files/Theme/Skin/bg/*.png").unwrap();
    let names = ["a.png", "b.png", "c.png"];
    let expected = names[LegacyMt::new(seed).get_rand(2) as usize];
    assert_eq!(got.file_name().unwrap(), expected);
}

/// ADP-SK-07: 一致しなければ None
#[rstest]
#[case::missing_file("LR2files/Theme/Skin/none.png")]
#[case::missing_dir("LR2files/Nope/a.png")]
#[case::wildcard_no_hit("LR2files/Theme/Skin/bg/*.jpg")]
fn adp_sk_07_no_match_is_none(tree: TempDir, #[case] pattern: &str) {
    let mut rng = LegacyMt::new(1);
    let mut r = FsResolver {
        root: tree.path().to_path_buf(),
        rng: &mut rng,
    };
    assert_eq!(r.resolve(pattern), None);
}
