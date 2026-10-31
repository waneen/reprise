//! 仕様ルール（lib.rs の表）とテスト関数は 1 対 1。関数名の接頭辞がルール ID。
use super::*;
use crate::ports::PortError;
use rstest::rstest;
use std::collections::BTreeMap;
use std::sync::Mutex;

/// 手元の表（URL → 名前）と、取ってこられる表（URL → 名前）
struct Fake {
    local: Mutex<BTreeMap<String, String>>,
    remote: BTreeMap<String, String>,
    fetched: Mutex<Vec<String>>,
}

fn table(name: &str) -> DifficultyTable {
    DifficultyTable {
        url: "保存したときの URL".into(),
        name: name.into(),
        levels: vec![],
    }
}

impl TableSource for Fake {
    fn cached(&self, url: &str) -> Option<DifficultyTable> {
        self.local.lock().unwrap().get(url).map(|n| table(n))
    }
    fn fetch(&self, url: &str) -> Result<DifficultyTable, PortError> {
        self.fetched.lock().unwrap().push(url.into());
        let n = self.remote.get(url).ok_or(PortError::Unsupported)?;
        self.local.lock().unwrap().insert(url.into(), n.clone());
        Ok(table(n))
    }
}

fn fake(local: &[(&str, &str)], remote: &[(&str, &str)]) -> Fake {
    let m = |v: &[(&str, &str)]| {
        v.iter()
            .map(|(a, b)| (a.to_string(), b.to_string()))
            .collect()
    };
    Fake {
        local: Mutex::new(m(local)),
        remote: m(remote),
        fetched: Mutex::new(vec![]),
    }
}

fn urls(v: &[&str]) -> Vec<String> {
    v.iter().map(|s| s.to_string()).collect()
}

/// APP-TBL-01: 手元の表を設定の順に
#[rstest]
#[case::order(&[("a", "A"), ("b", "B")], &["b", "a"], vec![("b", "B"), ("a", "A")])]
#[case::skip_missing(&[("a", "A")], &["x", "a"], vec![("a", "A")])]
fn app_tbl_01_cached(
    #[case] local: &[(&str, &str)],
    #[case] order: &[&str],
    #[case] want: Vec<(&str, &str)>,
) {
    let f = fake(local, &[]);
    let got: Vec<(String, String)> = cached(&f, &urls(order))
        .into_iter()
        .map(|t| (t.url, t.name))
        .collect();
    let want: Vec<(String, String)> = want
        .iter()
        .map(|(a, b)| (a.to_string(), b.to_string()))
        .collect();
    assert_eq!(got, want);
}

/// APP-TBL-02: 手元に無い表だけ取ってくる
#[rstest]
#[case::only_missing(&[("a", "A")], &[("a", "A2"), ("b", "B")], vec!["b"], true, vec![("a", "A"), ("b", "B")])]
#[case::failure(&[("a", "A")], &[], vec!["b"], false, vec![("a", "A")])]
#[case::nothing_missing(&[("a", "A"), ("b", "B")], &[], vec![], false, vec![("a", "A"), ("b", "B")])]
fn app_tbl_02_fetch_missing(
    #[case] local: &[(&str, &str)],
    #[case] remote: &[(&str, &str)],
    #[case] want_fetched: Vec<&str>,
    #[case] want_got: bool,
    #[case] want: Vec<(&str, &str)>,
) {
    let f = fake(local, remote);
    let u = urls(&["a", "b"]);
    assert_eq!(fetch_missing(&f, &u), want_got);
    assert_eq!(*f.fetched.lock().unwrap(), urls(&want_fetched));
    let got: Vec<(String, String)> = cached(&f, &u)
        .into_iter()
        .map(|t| (t.url, t.name))
        .collect();
    let want: Vec<(String, String)> = want
        .iter()
        .map(|(a, b)| (a.to_string(), b.to_string()))
        .collect();
    assert_eq!(got, want);
}

/// APP-TBL-03: すべて取り直す（取れなければ手元のまま）
#[rstest]
#[case::all(&[("a", "A")], &[("a", "A2"), ("b", "B")], true, vec![("a", "A2"), ("b", "B")])]
#[case::keep_on_failure(&[("a", "A")], &[], false, vec![("a", "A")])]
fn app_tbl_03_fetch_all(
    #[case] local: &[(&str, &str)],
    #[case] remote: &[(&str, &str)],
    #[case] want_got: bool,
    #[case] want: Vec<(&str, &str)>,
) {
    let f = fake(local, remote);
    let u = urls(&["a", "b"]);
    assert_eq!(fetch_all(&f, &u), want_got);
    assert_eq!(*f.fetched.lock().unwrap(), u);
    let got: Vec<(String, String)> = cached(&f, &u)
        .into_iter()
        .map(|t| (t.url, t.name))
        .collect();
    let want: Vec<(String, String)> = want
        .iter()
        .map(|(a, b)| (a.to_string(), b.to_string()))
        .collect();
    assert_eq!(got, want);
}
