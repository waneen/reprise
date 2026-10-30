//! 腐敗防止層：汎用 `reprise-skin-core` → `domain::skin`、および実ファイルシステム用の `FileResolver`。
//!
//! # 仕様ルール（テストと 1 対 1 対応）
//! | ID | ルール |
//! |---|---|
//! | ADP-SK-01 | シーン種別: LR2 の種別番号 0..=15 を `SceneKind` に、それ以外は `Other` |
//! | ADP-SK-02 | 要素種別: IMAGE/NUMBER/TEXT/SLIDER/BARGRAPH/BUTTON は対応する種別、NOTE は `NoteLane{lane=添字}`、それ以外は `Other{command,index}` |
//! | ADP-SK-03 | 画像: 解決できたものは Some、`Continue`/`Missing` は None |
//! | ADP-SK-04 | キーフレーム・条件・タイマー（未指定は 0）・合成・SRC の追加引数・DST の追加引数・描画順をそのまま写す |
//! | ADP-SK-05 | `FsResolver`: パスは root 基準。完全一致が無ければ各階層を大小無視で探す（Windows 前提のスキンを他 OS で読むため） |
//! | ADP-SK-06 | `FsResolver`: ファイル名の `*` は一致するファイル（名前を小文字にした順）から `LegacyMt::get_rand(n-1)` で選ぶ（LR2 `GetRandomFile`） |
//! | ADP-SK-07 | `FsResolver`: 一致するファイルが無ければ None |
//! | ADP-SK-08 | 読み込み終了時点で真の条件を昇順で写す |
//! | ADP-SK-09 | 抜き色: 最後の `#TRANSCOLOR,r,g,b`（LR2 が受け付ける綴り違い `TRANSCLOLR`/`TRANSCLOLOR` も）、無ければローダの既定 |
//! | ADP-SK-10 | 場面の時間: `STARTINPUT`（beatoraja は `INPUT`）・`LOADSTART`・`LOADEND`・`PLAYSTART`・`FADEOUT`・`CLOSE`（`SKIP` も）の第 1 引数。後の行が勝ち、無ければ 0 |
use application::ports::{PortError, SkinLoader};
use domain::{random::LegacyMt, skin as d};
use reprise_skin_core as sc;
use reprise_skin_core::{FileResolver, SkinDecoder, SkinSettings};
use std::path::{Path, PathBuf};

// ---------------------------------------------------------------------------
// ファイル解決（ADP-SK-05..07）
// ---------------------------------------------------------------------------

pub struct FsResolver<'a> {
    pub root: PathBuf,
    pub rng: &'a mut LegacyMt,
}

fn entries(dir: &Path) -> Vec<(String, PathBuf, bool)> {
    let mut v: Vec<(String, PathBuf, bool)> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| {
            let name = e.file_name().to_string_lossy().into_owned();
            let is_file = e.file_type().map(|t| t.is_file()).unwrap_or(false);
            (name, e.path(), is_file)
        })
        .collect();
    v.sort_by_key(|(n, _, _)| n.to_lowercase());
    v
}

/// ディスク上の実際の名前を返す（大小を区別しない FS でも表記を揃えるため一覧から探す）
fn find_ci(dir: &Path, name: &str) -> Option<PathBuf> {
    let list = entries(dir);
    list.iter()
        .find(|(n, _, _)| n == name)
        .or_else(|| list.iter().find(|(n, _, _)| n.eq_ignore_ascii_case(name)))
        .map(|(_, p, _)| p.clone())
}

impl FileResolver for FsResolver<'_> {
    fn resolve(&mut self, pattern: &str) -> Option<PathBuf> {
        let pattern = pattern.replace('\\', "/");
        let mut parts: Vec<&str> = pattern
            .split('/')
            .filter(|s| !s.is_empty() && *s != ".")
            .collect();
        let file = parts.pop()?;
        let mut dir = if Path::new(&pattern).is_absolute() {
            PathBuf::from("/")
        } else {
            self.root.clone()
        };
        for p in parts {
            dir = if p == ".." {
                dir.join("..")
            } else {
                find_ci(&dir, p)?
            };
        }
        match file.split_once('*') {
            None => find_ci(&dir, file).filter(|p| p.is_file()),
            Some((pre, post)) => {
                let (pre, post) = (pre.to_lowercase(), post.to_lowercase());
                let hits: Vec<PathBuf> = entries(&dir)
                    .into_iter()
                    .filter(|(n, _, is_file)| {
                        let n = n.to_lowercase();
                        *is_file
                            && n.len() >= pre.len() + post.len()
                            && n.starts_with(&pre)
                            && n.ends_with(&post)
                    })
                    .map(|(_, p, _)| p)
                    .collect();
                if hits.is_empty() {
                    return None;
                }
                let i = self.rng.get_rand(hits.len() as u32 - 1) as usize;
                Some(hits[i].clone())
            }
        }
    }
    fn read(&mut self, path: &Path) -> Result<Vec<u8>, String> {
        let p = if path.is_absolute() {
            path.to_path_buf()
        } else {
            self.root.join(path)
        };
        std::fs::read(&p).map_err(|e| format!("{}: {e}", p.display()))
    }
}

// ---------------------------------------------------------------------------
// 写像（ADP-SK-01..04, 08..10）
// ---------------------------------------------------------------------------

pub fn scene_kind(kind: i32) -> d::SceneKind {
    use d::SceneKind::*;
    match kind {
        0 => Play7,
        1 => Play5,
        2 => Play14,
        3 => Play10,
        4 => Play9,
        5 => Select,
        6 => Decide,
        7 => Result,
        8 => KeyConfig,
        9 => SkinSelect,
        10 => SoundSet,
        11 => Theme,
        12 => Play7Battle,
        13 => Play5Battle,
        14 => Play9Battle,
        15 => CourseResult,
        other => Other(other),
    }
}

fn element_kind(e: &sc::Element) -> d::ElementKind {
    match &e.kind {
        sc::ElementKind::Image => d::ElementKind::Image,
        sc::ElementKind::Number {
            value,
            align,
            digits,
        } => d::ElementKind::Number {
            value_ref: value.0,
            digits: *digits,
            align: *align,
        },
        sc::ElementKind::Text {
            font, value, align, ..
        } => d::ElementKind::Text {
            value_ref: value.0,
            font: font.0,
            align: *align,
        },
        sc::ElementKind::Slider {
            direction,
            range,
            value,
            ..
        } => d::ElementKind::Slider {
            value_ref: value.0,
            direction: *direction,
            range: *range,
        },
        sc::ElementKind::Bargraph { value, direction } => d::ElementKind::Bar {
            value_ref: value.0,
            direction: *direction,
        },
        sc::ElementKind::Button { action, .. } => d::ElementKind::Button { action: *action },
        sc::ElementKind::Other if e.command == "NOTE" => d::ElementKind::NoteLane { lane: e.index },
        sc::ElementKind::Other => d::ElementKind::Other {
            command: e.command.clone(),
            index: e.index,
        },
    }
}

fn blend(b: sc::Blend) -> d::Blend {
    match b {
        sc::Blend::None => d::Blend::None,
        sc::Blend::Alpha => d::Blend::Alpha,
        sc::Blend::Add => d::Blend::Add,
        sc::Blend::Subtract => d::Blend::Sub,
        sc::Blend::Multiply => d::Blend::Mul,
        sc::Blend::Subtract2 => d::Blend::Sub2,
        sc::Blend::InvertDest => d::Blend::InvDest,
        sc::Blend::InvertSrc => d::Blend::InvSrc,
        sc::Blend::MultiplyAlpha => d::Blend::MulAlpha,
    }
}

/// LR2 の抜き色の命令（綴り違いも LR2 は受け付ける）
const TRANSCOLOR: [&str; 3] = ["TRANSCOLOR", "TRANSCLOLR", "TRANSCLOLOR"];

/// `default_color_key` は `#TRANSCOLOR` が無いときの抜き色（LR2 は (0, 255, 0)）
pub fn map_skin(doc: sc::SkinDocument, default_color_key: Option<[u8; 3]>) -> d::Skin {
    let timer = |t: Option<sc::TimerRef>| d::TimerId(t.map(|t| t.0).unwrap_or(0));
    let color_key = doc
        .directives
        .iter()
        .rev()
        .find(|d| TRANSCOLOR.contains(&d.command.as_str()))
        .map(|d| {
            let v = |i: usize| {
                d.args
                    .get(i)
                    .and_then(|s| s.trim().parse::<i32>().ok())
                    .unwrap_or(0)
                    .clamp(0, 255) as u8
            };
            [v(0), v(1), v(2)]
        })
        .or(default_color_key);
    // ADP-SK-10: 場面の時間（後の行が勝つ）
    let mut timing = d::SceneTiming::default();
    for dir in &doc.directives {
        let v = dir
            .args
            .first()
            .and_then(|s| s.trim().parse::<i32>().ok())
            .unwrap_or(0);
        match dir.command.as_str() {
            "STARTINPUT" | "INPUT" => timing.start_input = v,
            "LOADSTART" => timing.load_start = v,
            "LOADEND" => timing.load_end = v,
            "PLAYSTART" => timing.play_start = v,
            "FADEOUT" => timing.fade_out = v,
            "CLOSE" | "SKIP" => timing.close = v,
            _ => {}
        }
    }
    d::Skin {
        timing,
        color_key,
        options: doc.options.into_iter().collect(),
        scene: scene_kind(doc.header.kind),
        resolution: doc.header.resolution,
        images: doc
            .images
            .into_iter()
            .map(|i| match i {
                sc::ImageEntry::File(p) => Some(p),
                sc::ImageEntry::Continue | sc::ImageEntry::Missing(_) => None,
            })
            .collect(),
        elements: doc
            .elements
            .iter()
            .map(|e| d::Element {
                kind: element_kind(e),
                source: e.source.as_ref().map(|s| d::SourceRect {
                    image: s.image.0,
                    rect: s.rect,
                    divide: s.divide,
                    cycle_ms: s.cycle_ms,
                    timer: timer(s.timer),
                    params: s.params,
                }),
                timer: timer(e.timer),
                conditions: e.conditions.iter().map(|c| d::OptionId(c.0)).collect(),
                looping: e.looping,
                dst_extra: e.dst_extra,
                order: e.order,
                keyframes: e
                    .keyframes
                    .iter()
                    .map(|k| d::Keyframe {
                        t_ms: k.time_ms,
                        dst: k.rect,
                        color: k.argb,
                        angle: k.angle,
                        center: k.center,
                        blend: blend(k.blend),
                        accel: k.accel,
                        filter: k.filter,
                    })
                    .collect(),
            })
            .collect(),
    }
}

// ---------------------------------------------------------------------------
// ポート実装
// ---------------------------------------------------------------------------

pub struct DecoderSkinLoader<D: SkinDecoder> {
    pub decoder: D,
    pub settings: SkinSettings,
    /// スキン中のパスの基準（LR2 なら LR2 本体のフォルダ）
    pub root: PathBuf,
    /// `#TRANSCOLOR` が無いときの抜き色
    pub default_color_key: Option<[u8; 3]>,
}

impl<D: SkinDecoder> SkinLoader for DecoderSkinLoader<D> {
    fn can_load(&self, p: &Path) -> bool {
        self.decoder.can_decode(p)
    }
    fn load(&self, p: &Path, rng: &mut LegacyMt) -> Result<d::Skin, PortError> {
        let mut files = FsResolver {
            root: self.root.clone(),
            rng,
        };
        let doc = self
            .decoder
            .decode(p, &self.settings, &mut files)
            .map_err(|e| PortError::Parse(format!("{e:?}")))?;
        Ok(map_skin(doc, self.default_color_key))
    }
}

#[cfg(test)]
mod tests;
