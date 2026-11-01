//! 仕様ルール（lib.rs の表）とテスト関数は 1 対 1。関数名の接頭辞がルール ID。
use super::*;
use rstest::rstest;
use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::net::TcpListener;

/// BT-01: ヘッダの URL
#[rstest]
#[case::double(r#"<meta name="bmstable" content="header.json">"#, Some("header.json"))]
#[case::single_upper("<META NAME='BMSTable' CONTENT='./h.json' />", Some("./h.json"))]
#[case::spaces(r#"<meta name = "bmstable"  content = "a b.json">"#, Some("a b.json"))]
#[case::last_wins(
    "<meta name=\"bmstable\" content=\"1.json\">\n<meta name=\"bmstable\" content=\"2.json\">",
    Some("2.json")
)]
#[case::other_meta(r#"<meta name="viewport" content="x">"#, None)]
#[case::not_meta(r#"<link name="bmstable" content="x.json">"#, None)]
#[case::attr_prefix(r#"<meta data-name="bmstable" content="x.json">"#, None)]
fn bt_01_header_url(#[case] html: &str, #[case] want: Option<&str>) {
    assert_eq!(header_url(html).as_deref(), want);
}

/// (名前, 記号, tag, data_url, level_order)
type HeaderWant<'a> = (&'a str, &'a str, &'a str, Vec<&'a str>, Vec<&'a str>);

/// BT-02: ヘッダ
#[rstest]
#[case::full(
    r#"{"name":"N","symbol":"★","tag":"S","data_url":"d.json","level_order":[1,"2",3.5]}"#,
    Some(("N", "★", "S", vec!["d.json"], vec!["1", "2", "3.5"]))
)]
#[case::tag_from_symbol(r#"{"name":"N","symbol":"▼","data_url":["a.json",3,"b.json"]}"#, Some(("N", "▼", "▼", vec!["a.json", "b.json"], vec![])))]
#[case::no_symbol(r#"{"name":"N","data_url":"d.json"}"#, None)]
#[case::name_not_string(r#"{"name":1,"symbol":"x"}"#, None)]
#[case::not_json("<html>", None)]
fn bt_02_header(#[case] text: &str, #[case] want: Option<HeaderWant>) {
    let got = parse_header(text)
        .ok()
        .map(|h| (h.name, h.symbol, h.tag, h.data_url, h.level_order));
    let want = want.map(|(n, s, t, d, l)| {
        (
            n.to_string(),
            s.to_string(),
            t.to_string(),
            d.iter().map(|x| x.to_string()).collect::<Vec<_>>(),
            l.iter().map(|x| x.to_string()).collect::<Vec<_>>(),
        )
    });
    assert_eq!(got, want);
    if let Ok(h) = parse_header(
        r#"{"name":"N","symbol":"s","data_url":["a","b"],"data_rule":[{"1":"2","x":""}, 3]}"#,
    ) {
        assert_eq!(
            h.data_rule,
            vec![
                vec![
                    ("1".to_string(), "2".to_string()),
                    ("x".to_string(), String::new())
                ],
                vec![]
            ]
        );
    }
}

const MD5: &str = "0123456789ABCDEF0123456789abcdef";
const SHA: &str = "ABCDEF0123456789abcdef0123456789abcdef0123456789abcdef0123456789";

/// BT-03: データ（(レベル, MD5, SHA-256, 題名)）
#[rstest]
#[case::md5(&format!(r#"[{{"level":"3","md5":"{MD5}","title":"T","artist":"A"}}]"#), Some(vec![("3", MD5.to_ascii_lowercase(), String::new(), "T")]))]
#[case::sha_number_level(&format!(r#"[{{"level":12,"sha256":"{SHA}"}}]"#), Some(vec![("12", String::new(), SHA.to_ascii_lowercase(), "")]))]
#[case::no_level(&format!(r#"[{{"md5":"{MD5}"}}]"#), Some(vec![]))]
#[case::short_hash(r#"[{"level":"1","md5":"0123456789abcdef01234567"}]"#, Some(vec![]))]
#[case::bad_type(&format!(r#"[{{"level":"1","md5":"{MD5}","title":5}}, 3]"#), Some(vec![]))]
#[case::not_array(r#"{"level":1}"#, None)]
fn bt_03_data(#[case] text: &str, #[case] want: Option<Vec<(&str, String, String, &str)>>) {
    let got = parse_data(text).ok().map(|rows| {
        rows.into_iter()
            .map(|r| (r.level, r.chart.md5, r.chart.sha256, r.chart.title))
            .collect::<Vec<_>>()
    });
    let want = want.map(|w| {
        w.into_iter()
            .map(|(l, m, s, t)| (l.to_string(), m, s, t.to_string()))
            .collect::<Vec<_>>()
    });
    assert_eq!(got, want);
}

fn row(level: &str, md5: &str, title: &str) -> Row {
    Row {
        level: level.into(),
        chart: TableChart {
            md5: md5.into(),
            sha256: String::new(),
            title: title.into(),
            artist: String::new(),
        },
    }
}

/// BT-04: 表を作る（レベルのフォルダ名 → 題名）
#[rstest]
#[case::order_of_data(vec![], vec![], vec![Some(vec![row("2", "aaaaaaaaaaa1", "a"), row("1", "aaaaaaaaaaa2", "b"), row("2", "aaaaaaaaaaa3", "c")])], vec![("★2", vec!["a", "c"]), ("★1", vec!["b"])])]
#[case::level_order(vec!["1", "2", "3"], vec![], vec![Some(vec![row("2", "aaaaaaaaaaa1", "a"), row("1", "aaaaaaaaaaa2", "b"), row("9", "aaaaaaaaaaa4", "z")])], vec![("★1", vec!["b"]), ("★2", vec!["a"]), ("★3", vec![])])]
#[case::dedupe_across(vec![], vec![], vec![Some(vec![row("1", "aaaaaaaaaaa1", "a")]), None, Some(vec![row("1", "aaaaaaaaaaa1", "dup"), row("1", "short", "s1"), row("1", "short", "s2")])], vec![("★1", vec!["a", "s1", "s2"])])]
#[case::rule(vec!["1", "2"], vec![vec![], vec![("x", "2"), ("y", "")]], vec![Some(vec![row("1", "aaaaaaaaaaa1", "a")]), Some(vec![row("x", "aaaaaaaaaaa2", "b"), row("y", "aaaaaaaaaaa3", "c")])], vec![("★1", vec!["a"]), ("★2", vec!["b"])])]
fn bt_04_build(
    #[case] level_order: Vec<&str>,
    #[case] rule: Vec<Vec<(&str, &str)>>,
    #[case] datas: Vec<Option<Vec<Row>>>,
    #[case] want: Vec<(&str, Vec<&str>)>,
) {
    let h = Header {
        name: "N".into(),
        symbol: "s".into(),
        tag: "★".into(),
        data_url: vec![],
        data_rule: rule
            .iter()
            .map(|r| {
                r.iter()
                    .map(|(a, b)| (a.to_string(), b.to_string()))
                    .collect()
            })
            .collect(),
        level_order: level_order.iter().map(|s| s.to_string()).collect(),
    };
    let t = build("https://t.invalid/", &h, &datas);
    assert_eq!(
        (t.url.as_str(), t.name.as_str()),
        ("https://t.invalid/", "N")
    );
    let got: Vec<(String, Vec<String>)> = t
        .levels
        .iter()
        .map(|l| {
            (
                l.name.clone(),
                l.charts.iter().map(|c| c.title.clone()).collect(),
            )
        })
        .collect();
    let want: Vec<(String, Vec<String>)> = want
        .iter()
        .map(|(n, v)| (n.to_string(), v.iter().map(|s| s.to_string()).collect()))
        .collect();
    assert_eq!(got, want);
}

/// パス → 中身 を返す HTTP サーバー（無いパスは 404）。ポートを返す
fn serve(files: BTreeMap<&'static str, String>) -> u16 {
    let l = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = l.local_addr().unwrap().port();
    std::thread::spawn(move || {
        for s in l.incoming() {
            let Ok(mut s) = s else { continue };
            let mut buf = vec![0u8; 4096];
            let n = s.read(&mut buf).unwrap_or(0);
            let req = String::from_utf8_lossy(&buf[..n]).into_owned();
            let path = req.split_whitespace().nth(1).unwrap_or("/").to_string();
            let (status, body) = match files.get(path.as_str()) {
                Some(b) => ("200 OK", b.clone()),
                None => ("404 Not Found", String::new()),
            };
            let _ = write!(
                s,
                "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
        }
    });
    port
}

fn local(dir: &std::path::Path) -> HttpTables {
    let agent: ureq::Agent = ureq::Agent::config_builder().proxy(None).build().into();
    HttpTables::with_agent(dir.to_path_buf(), agent)
}

/// BT-05: 取ってくる（URL のパス → (表の名前, レベルと譜面の数)。取れなければ None）
#[rstest]
#[case::html("/t/index.html", Some(("T", vec![("★1", 2), ("★2", 1)])))]
#[case::header_json("/t/h.json", Some(("T", vec![("★1", 2), ("★2", 1)])))]
#[case::no_meta("/plain.html", None)]
#[case::no_data("/bad/h.json", None)]
#[case::missing("/none.html", None)]
fn bt_05_fetch(#[case] path: &str, #[case] want: Option<(&str, Vec<(&str, usize)>)>) {
    let d1 = format!(
        r#"[{{"level":"1","md5":"{}"}},{{"level":"1","md5":"{}"}}]"#,
        "1".repeat(32),
        "2".repeat(32)
    );
    let d2 = format!(r#"[{{"level":"2","md5":"{}"}}]"#, "3".repeat(32));
    let port = serve(BTreeMap::from([
        (
            "/t/index.html",
            r#"<html><head><meta name="bmstable" content="h.json"></head></html>"#.to_string(),
        ),
        (
            "/t/h.json",
            r#"{"name":"T","symbol":"★","data_url":["data/1.json","/abs/2.json","gone.json"]}"#
                .to_string(),
        ),
        ("/t/data/1.json", d1),
        ("/abs/2.json", d2),
        ("/plain.html", "<html></html>".to_string()),
        (
            "/bad/h.json",
            r#"{"name":"B","symbol":"b","data_url":"gone.json"}"#.to_string(),
        ),
    ]));
    let dir = tempfile::tempdir().unwrap();
    let src = local(dir.path());
    let url = format!("http://127.0.0.1:{port}{path}");
    let summary = |t: &DifficultyTable| {
        (
            t.name.clone(),
            t.levels
                .iter()
                .map(|l| (l.name.clone(), l.charts.len()))
                .collect::<Vec<_>>(),
        )
    };
    let want = want.map(|(n, v)| {
        (
            n.to_string(),
            v.iter()
                .map(|(a, b)| (a.to_string(), *b))
                .collect::<Vec<_>>(),
        )
    });
    assert_eq!(src.cached(&url), None);
    let got = src.fetch(&url).ok();
    assert_eq!(got.as_ref().map(summary), want);
    // 手元に保存したものを読み戻す（取れなければ何も無い）
    assert_eq!(src.cached(&url), got);
    assert_eq!(src.cached("http://127.0.0.1:1/other.html"), None);
}
