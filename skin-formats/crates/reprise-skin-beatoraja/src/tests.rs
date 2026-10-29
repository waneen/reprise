//! 仕様ルール（lib.rs の表）とテスト関数は 1 対 1。関数名の接頭辞がルール ID。
//! スキン素材は同梱せず、テスト内で最小の JSON を組み立てる。
use super::*;
use rstest::rstest;
use std::collections::BTreeMap;
use std::path::PathBuf;

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

const MAIN: &str = "skin/play.json";
const HEAD: &str = r#""type": 0, "name": "S", "author": "A", "w": 1280, "h": 720"#;

fn decode_with(body: &str, extra: &[(&str, &[u8])], s: &SkinSettings) -> SkinDocument {
    let mut files = vec![(MAIN, body.as_bytes())];
    files.extend_from_slice(extra);
    BeatorajaJsonDecoder
        .decode(Path::new(MAIN), s, &mut MemFs::with(&files))
        .expect("decode")
}
fn decode(body: &str) -> SkinDocument {
    decode_with(body, &[], &SkinSettings::default())
}
fn header(body: &str) -> Result<SkinHeader, SkinError> {
    BeatorajaJsonDecoder.read_header(
        Path::new(MAIN),
        &mut MemFs::with(&[(MAIN, body.as_bytes())]),
    )
}
/// 画像 1 つと、`dst` だけ差し替えた destination 1 つ
fn with_dst(dst_attrs: &str, dst: &str) -> String {
    format!(
        r#"{{{HEAD},
        "source": [{{"id": "s", "path": "a.png"}}],
        "image": [{{"id": "i", "src": "s", "x": 0, "y": 0, "w": 10, "h": 10}}],
        "destination": [{{"id": "i" {dst_attrs}, "dst": [{dst}]}}]}}"#
    )
}
fn first(d: &SkinDocument) -> &Element {
    &d.elements[0]
}
fn ids(d: &SkinDocument) -> Vec<String> {
    d.elements.iter().map(|e| e.command.clone()).collect()
}
const A_PNG: &[(&str, &[u8])] = &[("skin/a.png", b"")];

/// BRJ-ENC-01: 文字コード
#[rstest]
#[case::utf8(b"{\"type\":0,\"name\":\"\xE6\x9B\xB2\"}".to_vec())]
#[case::utf8_bom(b"\xEF\xBB\xBF{\"type\":0,\"name\":\"\xE6\x9B\xB2\"}".to_vec())]
#[case::shift_jis(b"{\"type\":0,\"name\":\"\x8B\xC8\"}".to_vec())]
fn brj_enc_01_encoding(#[case] bytes: Vec<u8>) {
    let h = BeatorajaJsonDecoder
        .read_header(Path::new(MAIN), &mut MemFs::with(&[(MAIN, &bytes)]))
        .unwrap();
    assert_eq!(h.title, "曲");
}

/// BRJ-JSN-01: ゆるい JSON
#[rstest]
#[case::line_comment("{\"type\": 0, // c\n \"name\": \"x\"}", Some("x"))]
#[case::block_comment("{/* c */ \"type\": 0, \"name\": \"x\"}", Some("x"))]
#[case::trailing_commas("{\"type\": 0, \"name\": \"x\", \"property\": [],}", Some("x"))]
#[case::unquoted_key_and_value("{type: 0, name: hello world}", Some("hello world"))]
#[case::duplicate_last_wins("{\"type\": 0, \"name\": \"a\", \"name\": \"b\"}", Some("b"))]
#[case::numeric_string_for_int("{\"type\": \"0\", \"name\": \"x\"}", Some("x"))]
#[case::broken("{\"type\": 0,\n \"name\": ", None)]
fn brj_jsn_01_lenient_json(#[case] text: &str, #[case] name: Option<&str>) {
    let got = header(text);
    match name {
        Some(n) => assert_eq!(got.unwrap().title, n),
        None => assert!(matches!(got, Err(SkinError::Syntax { line: 2, .. }))),
    }
}

/// BRJ-HDR-01: ヘッダ
#[rstest]
#[case::full(r#"{"type": 5, "name": "N", "author": "A", "w": 1920, "h": 1080}"#, Some((5, "N", "A", (1920, 1080))))]
#[case::default_resolution(r#"{"type": 7}"#, Some((7, "", "", (1280, 720))))]
#[case::no_type(r#"{"name": "N"}"#, None)]
#[case::negative_type(r#"{"type": -1}"#, None)]
fn brj_hdr_01_header(#[case] text: &str, #[case] expected: Option<(i32, &str, &str, (u32, u32))>) {
    match (header(text), expected) {
        (Ok(h), Some(e)) => assert_eq!(
            (h.kind, h.title.as_str(), h.maker.as_str(), h.resolution),
            e
        ),
        (Err(SkinError::NotASkin(_)), None) => {}
        (got, _) => panic!("{got:?}"),
    }
}

/// BRJ-HDR-02: property / filepath
#[rstest]
#[case::property(
    r#""property": [{"name": "Gauge", "item": [{"name": "L", "op": 900}, {"name": "R", "op": 901}], "def": "R"}]"#,
    Custom::Choice { name: "Gauge".into(), items: vec![("L".into(), 900), ("R".into(), 901)], default: Some("R".into()) }
)]
#[case::filepath(
    r#""filepath": [{"name": "BG", "path": "bg\\*.png", "def": "blue.png"}]"#,
    Custom::File { name: "BG".into(), pattern: "bg/*.png".into(), default: "blue.png".into() }
)]
fn brj_hdr_02_customs(#[case] fields: &str, #[case] expected: Custom) {
    let h = header(&format!("{{{HEAD}, {fields}}}")).unwrap();
    assert_eq!(h.customs, vec![expected]);
}

/// BRJ-HDR-03: その他のヘッダ値
#[rstest]
#[case::play_values(r#""input": 500, "scene": 3000, "close": 1500"#, vec![("INPUT", "500"), ("SCENE", "3000"), ("CLOSE", "1500")])]
#[case::none_written("", vec![])]
fn brj_hdr_03_header_directives(#[case] fields: &str, #[case] expected: Vec<(&str, &str)>) {
    let sep = if fields.is_empty() { "" } else { "," };
    let d = decode(&format!("{{{HEAD}{sep} {fields}}}"));
    let got: Vec<(String, String)> = d
        .directives
        .iter()
        .map(|x| (x.command.clone(), x.args[0].clone()))
        .collect();
    let expected: Vec<(String, String)> = expected
        .into_iter()
        .map(|(a, b)| (a.to_string(), b.to_string()))
        .collect();
    assert_eq!(got, expected);
}

const PROP: &str = r#""property": [{"name": "G", "item": [{"name": "L", "op": 900}, {"name": "R", "op": 901}], "def": "R"}]"#;
fn probe(op: i32) -> String {
    format!(r#"{{{HEAD}, {PROP}, "destination": [{{"if": {op}, "value": {{"id": "HIT"}}}}]}}"#)
}

/// BRJ-OPT-01: 有効な条件
#[rstest]
#[case::def_item(None, vec![], 901, true)]
#[case::def_excludes_other(None, vec![], 900, false)]
#[case::chosen(Some(900), vec![], 900, true)]
#[case::chosen_not_in_items_falls_back(Some(950), vec![], 901, true)]
#[case::from_settings(None, vec![42], 42, true)]
fn brj_opt_01_enabled_options(
    #[case] choice: Option<i32>,
    #[case] ops: Vec<i32>,
    #[case] op: i32,
    #[case] hit: bool,
) {
    let mut s = SkinSettings {
        options: ops.into_iter().collect(),
        ..Default::default()
    };
    if let Some(c) = choice {
        s.custom.insert(0, CustomChoice::Option(c));
    }
    let d = decode_with(&probe(op), &[], &s);
    assert_eq!(ids(&d) == vec!["HIT"], hit);
    assert_eq!(d.options.contains(&op), hit);
}

/// BRJ-IF-01: 配列の条件付き要素
#[rstest]
#[case::value_true(r#"{"if": 901, "value": {"id": "A"}}"#, vec!["A"])]
#[case::value_false(r#"{"if": 900, "value": {"id": "A"}}"#, vec![])]
#[case::values_spliced(r#"{"if": 901, "values": [{"id": "A"}, {"id": "B"}]}"#, vec!["A", "B"])]
#[case::if_without_value_is_plain(r#"{"if": 900, "id": "A"}"#, vec!["A"])]
fn brj_if_01_conditional_elements(#[case] elem: &str, #[case] expected: Vec<&str>) {
    let d = decode(&format!(r#"{{{HEAD}, {PROP}, "destination": [{elem}]}}"#));
    assert_eq!(ids(&d), expected);
}

/// BRJ-IF-02: 条件の評価
#[rstest]
#[case::positive_on("901", true)]
#[case::positive_off("900", false)]
#[case::negative_off("-900", true)]
#[case::and_all("[901, 0]", true)]
#[case::and_one_false("[901, 900]", false)]
#[case::or_inner("[[900, 901]]", true)]
#[case::string_false("\"x\"", false)]
fn brj_if_02_condition_eval(#[case] cond: &str, #[case] hit: bool) {
    let s = SkinSettings {
        options: [0].into_iter().collect(),
        ..Default::default()
    };
    let d = decode_with(
        &format!(
            r#"{{{HEAD}, {PROP}, "destination": [{{"if": {cond}, "value": {{"id": "HIT"}}}}]}}"#
        ),
        &[],
        &s,
    );
    assert_eq!(ids(&d) == vec!["HIT"], hit);
    // ヘッダの読み取りでは条件は空: 正の条件の property は消える
    let h = header(&format!(
        r#"{{{HEAD}, "property": [{{"if": 901, "value": {{"name": "X", "item": []}}}}]}}"#
    ))
    .unwrap();
    assert!(h.customs.is_empty());
}

/// BRJ-INC-01: include
#[rstest]
#[case::array_element(r#""destination": [{"id": "A"}, {"include": "parts/d.json"}]"#, br#"[{"id": "B"}, {"id": "C"}]"#.as_slice(), vec!["A", "B", "C"])]
#[case::field_value(r#""destination": {"include": "parts/d.json"}"#, br#"[{"id": "B"}]"#.as_slice(), vec!["B"])]
#[case::nested_relative(r#""destination": [{"include": "parts/d.json"}]"#, br#"[{"include": "e.json"}]"#.as_slice(), vec!["E"])]
#[case::missing(r#""destination": [{"include": "parts/none.json"}]"#, b"[]".as_slice(), vec![])]
fn brj_inc_01_include(#[case] field: &str, #[case] inc: &[u8], #[case] expected: Vec<&str>) {
    let d = decode_with(
        &format!("{{{HEAD}, {field}}}"),
        &[
            ("skin/parts/d.json", inc),
            ("skin/parts/e.json", br#"[{"id": "E"}]"#),
        ],
        &SkinSettings::default(),
    );
    assert_eq!(ids(&d), expected);
}

/// BRJ-SRC-01: 画像の解決
#[rstest]
#[case::found("a.png", ImageEntry::File("skin/a.png".into()), 0)]
#[case::backslash_subdir("img\\b.png", ImageEntry::File("skin/img/b.png".into()), 0)]
#[case::wildcard("img/*.png", ImageEntry::File("skin/img/b.png".into()), 0)]
#[case::missing("none.png", ImageEntry::Missing("none.png".into()), 1)]
fn brj_src_01_sources(#[case] path: &str, #[case] expected: ImageEntry, #[case] diags: usize) {
    let path = path.replace('\\', "\\\\");
    let d = decode_with(
        &format!(r#"{{{HEAD}, "source": [{{"id": "s", "path": "{path}"}}]}}"#),
        &[("skin/a.png", b""), ("skin/img/b.png", b"")],
        &SkinSettings::default(),
    );
    assert_eq!((d.images, d.diagnostics.len()), (vec![expected], diags));
}

/// BRJ-CUS-01: filepath の置換
#[rstest]
#[case::default_used(None, "bg/*.png", "skin/bg/red.png")]
#[case::chosen(Some("blue.png"), "bg/*.png", "skin/bg/blue.png")]
#[case::random_left_to_resolver(Some("Random"), "bg/*.png", "skin/bg/blue.png")]
#[case::longer_path(Some("blue"), "bg/*/x.png", "skin/bg/blue/x.png")]
fn brj_cus_01_filepath_substitution(
    #[case] choice: Option<&str>,
    #[case] src: &str,
    #[case] expected: &str,
) {
    let pattern = if src.contains("/x.png") {
        "bg/*"
    } else {
        "bg/*.png"
    };
    let def = if src.contains("/x.png") {
        "red"
    } else {
        "red.png"
    };
    let mut s = SkinSettings::default();
    if let Some(c) = choice {
        s.custom.insert(0, CustomChoice::File(c.into()));
    }
    let d = decode_with(
        &format!(
            r#"{{{HEAD}, "filepath": [{{"name": "BG", "path": "{pattern}", "def": "{def}"}}],
            "source": [{{"id": "s", "path": "{src}"}}]}}"#
        ),
        &[
            ("skin/bg/blue.png", b""),
            ("skin/bg/red.png", b""),
            ("skin/bg/blue/x.png", b""),
        ],
        &s,
    );
    assert_eq!(d.images, vec![ImageEntry::File(expected.into())]);
}

/// BRJ-FNT-01: フォント
#[rstest]
#[case::found("f.ttf", FontEntry::File("skin/f.ttf".into()))]
#[case::missing("x.fnt", FontEntry::Missing("x.fnt".into()))]
fn brj_fnt_01_fonts(#[case] path: &str, #[case] expected: FontEntry) {
    let d = decode_with(
        &format!(
            r#"{{{HEAD}, "font": [{{"id": "z"}}, {{"id": "f", "path": "{path}"}}],
            "text": [{{"id": "t", "font": "f", "size": 20}}],
            "destination": [{{"id": "t", "dst": [{{}}]}}]}}"#
        ),
        &[("skin/f.ttf", b"")],
        &SkinSettings::default(),
    );
    assert_eq!(d.fonts, vec![expected]);
    assert!(matches!(
        first(&d).kind,
        ElementKind::Text {
            font: FontRef(0),
            ..
        }
    ));
}

/// BRJ-OBJ-01: id の引き当て
#[rstest]
#[case::image(r#""image": [{"id": "x", "src": "s"}]"#, "IMAGE")]
#[case::value(r#""value": [{"id": "x", "src": "s"}]"#, "NUMBER")]
#[case::text(r#""text": [{"id": "x"}]"#, "TEXT")]
#[case::slider(r#""slider": [{"id": "x", "src": "s"}]"#, "SLIDER")]
#[case::graph(r#""graph": [{"id": "x", "src": "s"}]"#, "BARGRAPH")]
#[case::image_wins_over_value(
    r#""value": [{"id": "x", "src": "s"}], "image": [{"id": "x", "src": "s"}]"#,
    "IMAGE"
)]
#[case::unknown_is_other("", "x")]
fn brj_obj_01_lookup(#[case] defs: &str, #[case] command: &str) {
    let sep = if defs.is_empty() { "" } else { "," };
    let d = decode_with(
        &format!(
            r#"{{{HEAD}, "source": [{{"id": "s", "path": "a.png"}}]{sep} {defs},
            "destination": [{{"id": "x", "dst": [{{}}]}}]}}"#
        ),
        A_PNG,
        &SkinSettings::default(),
    );
    assert_eq!(first(&d).command, command);
}

/// BRJ-OBJ-02: 切り出し
#[rstest]
#[case::full(r#""x": 1, "y": 2, "w": 3, "h": 4, "divx": 2, "divy": 5, "cycle": 100, "timer": 41"#, Source { image: ImageRef(0), rect: [1, 2, 3, 4], divide: (2, 5), cycle_ms: 100, timer: Some(TimerRef(41)), params: [0; 5] })]
#[case::defaults_and_min_div(r#""divx": 0, "timer": -1"#, Source { image: ImageRef(0), rect: [0, 0, 0, 0], divide: (1, 1), cycle_ms: 0, timer: None, params: [0; 5] })]
fn brj_obj_02_source_rect(#[case] attrs: &str, #[case] expected: Source) {
    let d = decode_with(
        &format!(
            r#"{{{HEAD}, "source": [{{"id": "s", "path": "a.png"}}],
            "image": [{{"id": "i", "src": "s", {attrs}}}],
            "destination": [{{"id": "i", "dst": [{{}}]}}]}}"#
        ),
        A_PNG,
        &SkinSettings::default(),
    );
    assert_eq!(first(&d).source, Some(expected));
}

/// BRJ-OBJ-03: 種別ごとの引数
#[rstest]
#[case::value(r#""value": [{"id": "x", "src": "s", "ref": 71, "align": 1, "digit": 4}]"#, ElementKind::Number { value: ValueRef(71), align: 1, digits: 4 })]
#[case::text(r#""text": [{"id": "x", "ref": 10, "align": 2, "editable": true}]"#, ElementKind::Text { font: FontRef(0), value: ValueRef(10), align: 2, editable: true })]
#[case::slider(r#""slider": [{"id": "x", "src": "s", "angle": 2, "range": 300, "type": 17, "changeable": false}]"#, ElementKind::Slider { direction: 2, range: 300, value: ValueRef(17), disabled: true })]
#[case::graph(r#""graph": [{"id": "x", "src": "s", "type": 102, "angle": 0}]"#, ElementKind::Bargraph { value: ValueRef(102), direction: 0 })]
fn brj_obj_03_kind_params(#[case] defs: &str, #[case] expected: ElementKind) {
    let d = decode_with(
        &format!(
            r#"{{{HEAD}, "source": [{{"id": "s", "path": "a.png"}}], {defs},
            "destination": [{{"id": "x", "dst": [{{}}]}}]}}"#
        ),
        A_PNG,
        &SkinSettings::default(),
    );
    assert_eq!(first(&d).kind, expected);
}

fn kf(t: i32, rect: [f32; 4], argb: [u8; 4]) -> (i32, [f32; 4], [u8; 4]) {
    (t, rect, argb)
}

/// BRJ-DST-01: キーフレームの補完と並び
#[rstest]
#[case::first_defaults("{}", vec![kf(0, [0.0, 720.0, 0.0, 0.0], [255, 255, 255, 255])])]
#[case::inherit_previous(
    r#"{"time": 0, "x": 10, "y": 20, "w": 30, "h": 40, "a": 128}, {"time": 500, "x": 50}"#,
    vec![kf(0, [10.0, 660.0, 30.0, 40.0], [128, 255, 255, 255]), kf(500, [50.0, 660.0, 30.0, 40.0], [128, 255, 255, 255])]
)]
#[case::sorted_by_time(
    r#"{"time": 500, "x": 1}, {"time": 0, "x": 2}"#,
    vec![kf(0, [2.0, 720.0, 0.0, 0.0], [255, 255, 255, 255]), kf(500, [1.0, 720.0, 0.0, 0.0], [255, 255, 255, 255])]
)]
fn brj_dst_01_keyframe_completion(
    #[case] dst: &str,
    #[case] expected: Vec<(i32, [f32; 4], [u8; 4])>,
) {
    let d = decode_with(&with_dst("", dst), A_PNG, &SkinSettings::default());
    let got: Vec<_> = first(&d)
        .keyframes
        .iter()
        .map(|k| (k.time_ms, k.rect, k.argb))
        .collect();
    assert_eq!(got, expected);
}

/// BRJ-DST-02: 左下原点 → 上端原点
#[rstest]
#[case::bottom(0, 100, 620.0)]
#[case::top(620, 100, 0.0)]
#[case::middle(310, 100, 310.0)]
fn brj_dst_02_flip_y(#[case] y: i64, #[case] h: i64, #[case] expected: f32) {
    let d = decode_with(
        &with_dst("", &format!(r#"{{"y": {y}, "h": {h}}}"#)),
        A_PNG,
        &SkinSettings::default(),
    );
    assert_eq!(first(&d).keyframes[0].rect[1], expected);
}

/// BRJ-DST-03: destination 単位の属性
#[rstest]
#[case::numeric_timer(
    r#", "timer": 41, "loop": 500, "filter": 1, "center": 5"#,
    Some(TimerRef(41)),
    Some(500),
    true,
    5,
    0
)]
#[case::negative_timer(r#", "timer": -1"#, None, Some(0), false, 0, 0)]
#[case::lua_timer(r#", "timer": "function() return 0 end""#, None, Some(0), false, 0, 1)]
fn brj_dst_03_destination_attributes(
    #[case] attrs: &str,
    #[case] timer: Option<TimerRef>,
    #[case] looping: Option<i32>,
    #[case] filter: bool,
    #[case] center: u8,
    #[case] diags: usize,
) {
    let d = decode_with(
        &with_dst(attrs, "{}, {\"time\": 100}"),
        A_PNG,
        &SkinSettings::default(),
    );
    let e = first(&d);
    assert_eq!((e.timer, e.looping), (timer, looping));
    assert!(e
        .keyframes
        .iter()
        .all(|k| k.filter == filter && k.center == center));
    assert_eq!(d.diagnostics.len(), diags);
}

/// BRJ-DST-04: op
#[rstest]
#[case::custom_true(r#"[901]"#, true, vec![])]
#[case::custom_false(r#"[900]"#, false, vec![])]
#[case::custom_negated(r#"[-900]"#, true, vec![])]
#[case::builtin_kept(r#"[41, -42]"#, true, vec![ConditionRef(41), ConditionRef(-42)])]
#[case::string_ignored(r#"["OPTION_X"]"#, true, vec![])]
fn brj_dst_04_op(#[case] op: &str, #[case] present: bool, #[case] conditions: Vec<ConditionRef>) {
    let d = decode_with(
        &format!(
            r#"{{{HEAD}, {PROP}, "destination": [{{"id": "x", "op": {op}, "dst": [{{}}]}}]}}"#
        ),
        &[],
        &SkinSettings::default(),
    );
    assert_eq!(d.elements.len(), present as usize);
    if present {
        assert_eq!(first(&d).conditions, conditions);
    }
}

/// BRJ-BLD-01: 合成
#[rstest]
#[case::zero(0, Blend::Alpha, 0)]
#[case::alpha(1, Blend::Alpha, 0)]
#[case::add(2, Blend::Add, 0)]
#[case::sub(3, Blend::Subtract, 0)]
#[case::mul(4, Blend::Multiply, 0)]
#[case::invert(9, Blend::InvertDest, 0)]
#[case::unknown(5, Blend::Alpha, 1)]
fn brj_bld_01_blend(#[case] n: i32, #[case] expected: Blend, #[case] diags: usize) {
    let d = decode_with(
        &with_dst(&format!(r#", "blend": {n}"#), "{}"),
        A_PNG,
        &SkinSettings::default(),
    );
    assert_eq!(first(&d).keyframes[0].blend, expected);
    assert_eq!(d.diagnostics.len(), diags);
}
