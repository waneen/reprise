//! 腐敗防止層：汎用 `reprise-chart-core` のモデル → ゲームの `domain::chart::Chart`。
//! どの記法かは知らない（`ChartDecoder` を注入されるだけ）。
//!
//! # 仕様ルール（テストと 1 対 1 対応）
//! | ID | ルール |
//! |---|---|
//! | ADP-CH-01 | 拡張子はデコーダの `extensions` と大小無視で照合する |
//! | ADP-CH-02 | `ChartHash` は元のバイト列の MD5 と SHA-256 |
//! | ADP-CH-03 | 位置→拍: Σ_{k<m} 4·len_k + 4·len_m·frac（len は小節長、既定 1） |
//! | ADP-CH-04 | STOP は 1/192 小節（4/4）単位なので 拍 = d / 48 |
//! | ADP-CH-05 | 鍵盤構成: Beat5→K5, Beat7→K7, Beat10→K10, Beat14→K14, Popn9→K9 |
//! | ADP-CH-06 | ノーツ種別・BGM・BGA・音/画像定義・メタ情報をそのまま写す（LN の終端も拍に変換） |
//! | ADP-CH-07 | `#RANDOM` は注入された `LegacyMt` で引く（`GetRand(n-1)+1`） |
//! | ADP-CH-08 | 小節長の変化は `TimingEvent::Measure` として小節番号順に出す |
//! | ADP-CH-09 | 音ファイルの解決（LR2 の `FindAltSound`）: 譜面のフォルダで書かれた名前、無ければ拡張子を ogg → wav → flac → mp3 に替えた名前（名前に既にその拡張子を含むものは飛ばす）、それも無ければ 1 つ上のフォルダの書かれた名前。名前の `\` は区切りとみなし、大小は区別しない |
use application::ports::{ChartLoader, PortError};
use domain::{chart::*, random::LegacyMt};
use md5::{Digest, Md5};
use reprise_chart_core as rc;
use reprise_chart_core::{ChartDecoder, DecodeHint, RandomSource};
use sha2::Sha256;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// domain の乱数を汎用 crate の trait に適合させる（ADP-CH-07）
struct LegacyRng<'a>(&'a mut LegacyMt);
impl RandomSource for LegacyRng<'_> {
    fn next_inclusive(&mut self, max: u32) -> u32 {
        self.0.get_rand(max)
    }
}

pub struct DecoderLoader<D: ChartDecoder>(pub D);

fn extension(p: &Path) -> Option<String> {
    p.extension()
        .and_then(|e| e.to_str())
        .map(str::to_ascii_lowercase)
}

impl<D: ChartDecoder> ChartLoader for DecoderLoader<D> {
    fn can_load(&self, p: &Path) -> bool {
        extension(p).is_some_and(|e| self.0.extensions().contains(&e.as_str()))
    }
    fn load(&self, p: &Path, bytes: &[u8], rng: &mut LegacyMt) -> Result<Chart, PortError> {
        let hint = DecodeHint {
            extension: extension(p),
        };
        let (doc, _diag) = self
            .0
            .decode(bytes, &hint)
            .map_err(|e| PortError::Parse(format!("{e:?}")))?;
        let resolved = doc.resolve(&mut LegacyRng(rng));
        Ok(map_chart(resolved, hash(bytes)))
    }
}

/// ADP-CH-02
pub fn hash(bytes: &[u8]) -> ChartHash {
    ChartHash {
        md5: Md5::digest(bytes).into(),
        sha256: Some(Sha256::digest(bytes).into()),
    }
}

/// ADP-CH-03: 小節長から拍位置を求める
struct BeatMap {
    /// prefix[m] = 小節 m の先頭の拍
    prefix: Vec<f64>,
    lengths: BTreeMap<u32, f64>,
}

impl BeatMap {
    fn new(lengths: BTreeMap<u32, f64>, last_measure: u32) -> Self {
        let mut prefix = Vec::with_capacity(last_measure as usize + 2);
        let mut acc = 0.0;
        for m in 0..=last_measure + 1 {
            prefix.push(acc);
            acc += 4.0 * lengths.get(&m).copied().unwrap_or(1.0);
        }
        Self { prefix, lengths }
    }
    fn beat(&self, p: rc::Position) -> Beat {
        let len = self.lengths.get(&p.measure).copied().unwrap_or(1.0);
        Beat(self.prefix[p.measure as usize] + 4.0 * len * p.fraction)
    }
}

fn key_mode(l: rc::KeyLayout) -> KeyMode {
    match l {
        rc::KeyLayout::Beat5 => KeyMode::K5,
        rc::KeyLayout::Beat7 => KeyMode::K7,
        rc::KeyLayout::Beat10 => KeyMode::K10,
        rc::KeyLayout::Beat14 => KeyMode::K14,
        rc::KeyLayout::Popn9 => KeyMode::K9,
    }
}

pub fn map_chart(r: rc::ResolvedChart, hash: ChartHash) -> Chart {
    let mut lengths = BTreeMap::new();
    let mut last = 0;
    for e in &r.events {
        last = last.max(e.position().measure);
        if let rc::Event::Note {
            kind: rc::NoteKind::Long { end, .. },
            ..
        } = e
        {
            last = last.max(end.measure);
        }
        if let rc::Event::MeasureLength { measure, ratio } = e {
            lengths.insert(*measure, *ratio);
        }
    }
    let bm = BeatMap::new(lengths.clone(), last);
    let sid = |id: rc::ObjId| SoundId(id.0);

    let mut notes = vec![];
    let mut bgm = vec![];
    let mut bga = vec![];
    let mut timing = vec![];
    for e in r.events {
        match e {
            rc::Event::Note {
                pos,
                lane,
                kind,
                sound,
            } => {
                let kind = match kind {
                    rc::NoteKind::Normal => NoteKind::Normal,
                    rc::NoteKind::Invisible => NoteKind::Invisible,
                    rc::NoteKind::Mine { damage_raw } => NoteKind::Mine { damage_raw },
                    rc::NoteKind::Long { end, end_sound } => NoteKind::Long {
                        end: bm.beat(end),
                        end_sound: end_sound.map(sid),
                    },
                };
                notes.push(Note {
                    beat: bm.beat(pos),
                    lane: Lane(lane.0),
                    kind,
                    sound: sound.map(sid),
                });
            }
            rc::Event::Bgm { pos, sound } => bgm.push((bm.beat(pos), sid(sound))),
            rc::Event::Bga { pos, layer, image } => {
                let layer = match layer {
                    rc::BgaLayer::Base => BgaLayer::Base,
                    rc::BgaLayer::Poor => BgaLayer::Poor,
                    rc::BgaLayer::Layer => BgaLayer::Layer,
                };
                bga.push((bm.beat(pos), layer, ImageId(image.0)));
            }
            rc::Event::Bpm { pos, bpm } => timing.push(TimingEvent::Bpm {
                beat: bm.beat(pos),
                bpm,
            }),
            // ADP-CH-04
            rc::Event::Stop {
                pos,
                duration_192th,
            } => timing.push(TimingEvent::Stop {
                beat: bm.beat(pos),
                beats: duration_192th / 48.0,
            }),
            // ADP-CH-08
            rc::Event::MeasureLength { measure, ratio } => timing.push(TimingEvent::Measure {
                index: measure,
                length: ratio,
            }),
        }
    }
    let m = r.meta;
    Chart {
        hash,
        meta: ChartMeta {
            title: m.title,
            subtitle: m.subtitle,
            artist: m.artist,
            subartist: m.subartist,
            genre: m.genre,
            initial_bpm: m.initial_bpm,
            level: m.play_level,
            total: m.total,
            judge_rank: m.rank,
            judge_rank_percent: m.judge_rank_percent,
            total_percent: m.total_percent,
            chart_name: m.chart_name,
        },
        key_mode: key_mode(r.layout),
        notes,
        bgm,
        bga,
        timing,
        sounds: r.sounds.into_iter().map(|(k, v)| (sid(k), v)).collect(),
        images: r
            .images
            .into_iter()
            .map(|(k, v)| (ImageId(k.0), v))
            .collect(),
    }
}

/// フォルダの中から名前を大小無視で探す（同じ表記があればそれを優先）
fn find_ci(dir: &Path, name: &str) -> Option<PathBuf> {
    let mut list: Vec<(String, PathBuf)> = std::fs::read_dir(dir)
        .ok()?
        .flatten()
        .map(|e| (e.file_name().to_string_lossy().into_owned(), e.path()))
        .collect();
    list.sort();
    list.iter()
        .find(|(n, _)| n == name)
        .or_else(|| list.iter().find(|(n, _)| n.eq_ignore_ascii_case(name)))
        .map(|(_, p)| p.clone())
}

/// `dir` からの相対パス（`/` か `\` 区切り）を 1 段ずつ大小無視で辿る
fn find_rel(dir: &Path, rel: &str) -> Option<PathBuf> {
    let mut cur = dir.to_path_buf();
    for part in rel
        .split(['/', '\\'])
        .filter(|s| !s.is_empty() && *s != ".")
    {
        cur = if part == ".." {
            cur.parent()?.to_path_buf()
        } else {
            find_ci(&cur, part)?
        };
    }
    cur.is_file().then_some(cur)
}

/// 譜面の音ファイルを探す（ADP-CH-09）
pub fn find_sound_file(chart_dir: &Path, name: &str) -> Option<PathBuf> {
    if let Some(p) = find_rel(chart_dir, name) {
        return Some(p);
    }
    let lower = name.to_ascii_lowercase();
    let stem = match name.rfind('.') {
        Some(i) => &name[..i],
        None => name,
    };
    for ext in ["ogg", "wav", "flac", "mp3"] {
        if lower.contains(&format!(".{ext}")) {
            continue;
        }
        if let Some(p) = find_rel(chart_dir, &format!("{stem}.{ext}")) {
            return Some(p);
        }
    }
    find_rel(chart_dir.parent()?, name)
}

#[cfg(test)]
mod tests;
