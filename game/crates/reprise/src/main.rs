//! Composition Root + プレゼンテーション層（Bevy はここだけ）。
//! 記法を増やす = chart-formats/ か skin-formats/ に crate を足して、ここで 1 行登録するだけ。
//!
//! 使い方: `reprise --songs <フォルダ> [オプション]` で選曲から、`reprise <譜面ファイル> [オプション]` でその譜面のプレイから（オプションは [`USAGE`]）。
//! 7 鍵（5 鍵）のシングルプレイの画面（1P と 2P の割り当てのどちらでも遊べる）。`--skin` で LR2 のプレイスキンで描き、無ければ四角形だけで描く。
//! 選曲画面は `--select-skin` で LR2 の選曲スキンで描き、無ければ文字の一覧で描く。選曲スキンの BUTTON 13 で
//! キー設定画面に入る（`config.json` の `keyconfig_skin` のキー設定スキンで描く。無ければ入らない）。
//! 遊び終えると `config.json` の `result_skin` のリザルトスキンでリザルト画面を出す（無ければ結果を文字で出す）。
//! `--skin-root` はスキン中のパスの基準（LR2 なら LR2 本体のフォルダ。既定は今のフォルダ）。
//! `config.json` に無いスキンは LR2 本体の `LR2files/Config/config.xml` で選んでいるもの。
//! 曲一覧は `--data` のフォルダの `song.db`（LR2 の song.db と同じテーブル）に置き、譜面のフォルダを見張って作り直す（ネイティブ版）。
//! 記録は `--data` のフォルダ（既定は今のフォルダの `reprise-data`）の `score.db` に、設定は `config.json` に保存する
//! （起動引数で渡した設定も保存するので、次からは引数無しで選曲から始められる）。
mod input;
mod keyconfig_scene;
mod platform;
mod preview;
mod result_scene;
mod select_scene;
mod skin_render;

use application::play::{KeyId, KeyInput, PlaySession, VisibleNote};
use application::play_options::{ControlInput, PlayOptions};
use application::play_skin::{PlaySkin, QuitInput};
use application::ports::{AudioOutput, ChartLoader, FileSystem, SkinLoader};
use application::result_skin::{after_play, AfterPlay};
use application::select_skin::OptionNames;
use application::settings::Settings;
use application::usecase::SavedScore;
use bevy::audio::AudioSource;
use bevy::ecs::system::RunSystemOnce;
use bevy::input::mouse::MouseWheel;
use bevy::platform::time::Instant;
use bevy::prelude::*;
use bevy::sprite::update_text2d_layout;
use bevy::text::FontSource;
use bevy::transform::TransformSystems;
use bevy::window::{PresentMode, WindowResolution};
use chart_loader::DecoderLoader;
use domain::chart::ChartHash;
use domain::chart::{Chart, ImageId, Lane, SoundId};
use domain::input::{SELECT, START};
use domain::play::RulesetId;
use domain::play::{GaugeType, HsFix, Judge, PlayEvent};
use domain::random::LegacyMt;
use domain::score::{SaveMode, ScoreRecord};
use domain::select::Library;
use domain::skin::lr2::draw_play;
use domain::skin::{SceneKind, Skin};
use platform::{fatal, seed, Scores};
use skin_loader::DecoderSkinLoader;
use skin_render::{Pixels, SkinDraw, SkinGfx, SkinRenderPlugin, SkinSprite};
use std::collections::{BTreeMap, HashMap};
use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

/// 再生開始から 0ms（譜面の先頭）までの待ち時間（スキン無しのとき。スキンがあればスキンの場面の時間）
const LEAD_IN_MS: i64 = 2000;
/// 判定ラインから上端までに見える拍数（スキン無しのとき）
const VISIBLE_BEATS: f64 = 4.0;

const USAGE: &str = "使い方: reprise [--songs <譜面のフォルダ>...] [<譜面ファイル>] [--gauge groove|easy|hard|death|pattack|gattack] [--skin <スキン>] [--select-skin <スキン>] [--skin-root <フォルダ>] \
[--hs <ハイスピード×100>] [--hsfix off|max|min|avg|const|main] [--cover <0..100>] [--offset <ms>] [--auto] [--data <フォルダ>]";

/// 記録などを置くフォルダの既定（今のフォルダから）
const DATA_DIR: &str = "reprise-data";

/// 操作（皿の 2 方向・鍵盤 1〜7）のレーン（1P）。皿は 2 つの操作で左右回転を表す
const SLOT_LANES: [u8; 9] = [0, 0, 1, 2, 3, 4, 5, 6, 7];

// 画面配置（ピクセル、原点は画面中央）
const LANE_LEFT: f32 = -560.0;
const SCRATCH_W: f32 = 60.0;
const KEY_W: f32 = 40.0;
const JUDGE_Y: f32 = -260.0;
const LANE_H: f32 = 560.0;
const NOTE_H: f32 = 10.0;

fn chart_loaders() -> Vec<Box<dyn ChartLoader>> {
    vec![
        Box::new(DecoderLoader(reprise_chart_bms::BmsDecoder::default())),
        Box::new(DecoderLoader(reprise_chart_bmson::BmsonDecoder)),
    ]
}

/// スキン設定の選択（項目名 → 選んだ名前）
pub type SkinCustom = BTreeMap<String, String>;

/// スキンの読み込み（描けるのは LR2 のプレイスキン・選曲スキン・キー設定スキン・リザルトスキン）。`custom` はスキン設定の選択
fn skin_loaders(
    fs: &Arc<dyn FileSystem>,
    root: PathBuf,
    custom: &SkinCustom,
) -> Vec<Box<dyn SkinLoader>> {
    vec![
        Box::new(DecoderSkinLoader {
            decoder: reprise_skin_lr2csv::Lr2CsvDecoder::default(),
            fs: fs.clone(),
            settings: Default::default(),
            custom: custom.clone(),
            root: root.clone(),
            // LR2 の既定の抜き色
            default_color_key: Some([0, 255, 0]),
        }),
        Box::new(DecoderSkinLoader {
            decoder: reprise_skin_beatoraja::BeatorajaJsonDecoder,
            fs: fs.clone(),
            settings: Default::default(),
            custom: custom.clone(),
            root,
            default_color_key: None,
        }),
    ]
}

/// スキン画像を RGBA にする。アルファの無い画像は抜き色の画素を透明にする
fn load_image(
    fs: &dyn FileSystem,
    path: &Path,
    color_key: Option<[u8; 3]>,
) -> Result<Pixels, String> {
    let bytes = fs.read(path).map_err(|e| format!("{e:?}"))?;
    let img = image::ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|e| format!("{}: {e}", path.display()))?
        .decode()
        .map_err(|e| format!("{}: {e}", path.display()))?;
    let has_alpha = img.color().has_alpha();
    let mut rgba = img.into_rgba8();
    if let (false, Some(key)) = (has_alpha, color_key) {
        for p in rgba.pixels_mut() {
            if p.0[..3] == key {
                p.0[3] = 0;
            }
        }
    }
    Ok((rgba.width(), rgba.height(), rgba.into_raw()))
}

/// スキンと画像を読み込む。`scenes` は描ける場面の種類、`custom` は `config.json` の `skin_custom` のこのスキンの選択
pub fn load_skin(
    fs: &Arc<dyn FileSystem>,
    path: &Path,
    root: PathBuf,
    scenes: &[SceneKind],
    custom: &SkinCustom,
) -> Result<(Skin, Vec<Option<Pixels>>), String> {
    let loaders = skin_loaders(fs, root, custom);
    let loader = loaders
        .iter()
        .find(|l| l.can_load(path))
        .ok_or_else(|| format!("スキンとして読めない: {}", path.display()))?;
    let mut rng = LegacyMt::new(seed());
    let skin = loader
        .load(path, &mut rng)
        .map_err(|e| format!("スキンを読めない: {e:?}"))?;
    if !scenes.contains(&skin.scene) {
        return Err(format!(
            "{} の種類が違う（{scenes:?} ではない）: {:?}",
            path.display(),
            skin.scene
        ));
    }
    let images = skin
        .images
        .iter()
        .map(|p| {
            let p = p.as_ref()?;
            load_image(fs.as_ref(), p, skin.color_key)
                .map_err(|e| eprintln!("スキン画像を読めない: {e}"))
                .ok()
        })
        .collect();
    Ok((skin, images))
}

fn parse_hs_fix(s: &str) -> Option<HsFix> {
    Some(match s.to_ascii_lowercase().as_str() {
        "off" => HsFix::Off,
        "max" => HsFix::MaxBpm,
        "min" => HsFix::MinBpm,
        "avg" => HsFix::Average,
        "const" => HsFix::Constant,
        "main" => HsFix::MainBpm,
        _ => return None,
    })
}

fn parse_gauge(s: &str) -> Option<GaugeType> {
    Some(match s.to_ascii_lowercase().as_str() {
        "groove" | "normal" => GaugeType::Groove,
        "easy" => GaugeType::Easy,
        "hard" => GaugeType::Hard,
        "death" => GaugeType::Hazard,
        "pattack" => GaugeType::PAttack,
        "gattack" => GaugeType::GAttack,
        _ => return None,
    })
}

/// rodio で読めない音は登録しない（Bevy は読めない音を鳴らそうとすると止まるため）
fn decodable(bytes: &Arc<[u8]>) -> bool {
    rodio::Decoder::builder()
        .with_byte_len(bytes.len() as u64)
        .with_data(Cursor::new(bytes.clone()))
        .build()
        .is_ok()
}

/// 読み込んだ音（ID とファイルの中身）
pub(crate) type Sounds = Vec<(SoundId, Arc<[u8]>)>;

/// 音の長さ（ms）。分からなければ 0
fn sound_ms(bytes: &Arc<[u8]>) -> i64 {
    use rodio::Source;
    let Ok(d) = rodio::Decoder::builder()
        .with_byte_len(bytes.len() as u64)
        .with_data(Cursor::new(bytes.clone()))
        .build()
    else {
        return 0;
    };
    if let Some(t) = d.total_duration() {
        return t.as_millis() as i64;
    }
    let (ch, rate) = (d.channels().get() as u64, d.sample_rate().get() as u64);
    let n = d.count() as u64;
    (n * 1000 / (ch * rate).max(1)) as i64
}

/// 譜面の BGA 画像を読み込む（LR2 と同じく、アルファの無い画像は黒を透明にする）
fn load_bga(fs: &dyn FileSystem, chart: &Chart, chart_path: &Path) -> Vec<(ImageId, Pixels)> {
    let dir = chart_path.parent().unwrap_or(Path::new("."));
    chart
        .images
        .iter()
        .filter_map(|(id, name)| {
            let Some(p) = chart_loader::find_image_file(fs, dir, name) else {
                eprintln!("画像が見つからない: {name}");
                return None;
            };
            load_image(fs, &p, Some([0, 0, 0]))
                .map_err(|e| eprintln!("画像を読めない: {e}"))
                .ok()
                .map(|px| (*id, px))
        })
        .collect()
}

/// 譜面と音を読み込む（`seed` は `#RANDOM` の乱数の種）
pub(crate) fn load(fs: &dyn FileSystem, path: &Path, seed: u32) -> Result<(Chart, Sounds), String> {
    let bytes = fs.read(path).map_err(|e| format!("{e:?}"))?;
    let (chart, _) = application::usecase::load_chart(&chart_loaders(), path, &bytes, seed)
        .map_err(|e| format!("譜面を読めない: {e:?}"))?;
    let dir = path.parent().unwrap_or(Path::new("."));
    let mut sounds = vec![];
    for (id, name) in &chart.sounds {
        let Some(p) = chart_loader::find_sound_file(fs, dir, name) else {
            eprintln!("音が見つからない: {name}");
            continue;
        };
        match fs.read(&p) {
            Ok(b) => {
                let b: Arc<[u8]> = b.into();
                if decodable(&b) {
                    sounds.push((*id, b));
                } else {
                    eprintln!("音を読めない: {}", p.display());
                }
            }
            Err(e) => eprintln!("{e:?}"),
        }
    }
    Ok((chart, sounds))
}

/// 場面
#[derive(States, Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Scene {
    #[default]
    Select,
    Play,
    /// キー設定（選曲スキンの BUTTON 13 から）
    KeyConfig,
    /// リザルト（リザルトスキンがあるとき、プレイの後に）
    Result,
}

/// 次に遊ぶ譜面
#[derive(Resource)]
pub struct PlayRequest {
    pub path: PathBuf,
    /// 選曲でオートプレイを選んだ
    pub auto: bool,
    /// `#RANDOM` の乱数の種（リザルトからもう一度遊ぶときは前と同じ。None なら新しく決める）
    pub seed: Option<u32>,
}

/// 起動時に決めたこと（どのプレイにも使う）
#[derive(Resource)]
pub struct Boot {
    pub(crate) fs: Arc<dyn FileSystem>,
    gauge: GaugeType,
    options: PlayOptions,
    skin_path: Option<PathBuf>,
    /// プレイスキンのスキン設定の選択（プレイのたびに読み直すので持っておく）
    skin_custom: SkinCustom,
    skin_root: PathBuf,
    /// 譜面を直接渡して起動した（終わったら選曲に戻らずに終了する）
    pub direct: bool,
}

/// 記録の保存先（開けなければ・ブラウザ版は None。sqlite の接続は Sync でないので Mutex に入れる）
#[derive(Resource)]
pub struct ScoreDb(Option<Mutex<Scores>>);

impl ScoreDb {
    pub fn find(&self, chart: &ChartHash, ruleset: RulesetId) -> Option<ScoreRecord> {
        let db = self.0.as_ref()?.lock().ok()?;
        db.find(chart, ruleset).ok().flatten()
    }
}

/// 設定と保存先。`saved` は保存する内容（起動引数で渡したものも含む。譜面の直接指定とオートは除く）
#[derive(Resource)]
pub struct SettingsRes {
    pub saved: Settings,
    store: Option<platform::Store>,
}

/// 設定を保存する（保存先が無ければ何もしない）
pub fn save_settings(s: &SettingsRes) {
    if let Some(store) = &s.store {
        if let Err(e) = store.save(&s.saved) {
            eprintln!("設定を保存できない: {e:?}");
        }
    }
}

/// 一覧などの文字のフォント（ブラウザ版は OS のフォントを探せないので Bevy の既定のフォント。日本語の字は無い）
pub fn list_font() -> FontSource {
    if cfg!(target_arch = "wasm32") {
        TextFont::default().font
    } else {
        FontSource::SansSerif
    }
}

/// LR2 本体の `LR2files/Config/optionstr.csv`（選曲スキンの文字列 60..62 の名前。無ければ空）
fn option_names(fs: &dyn FileSystem, root: &Path) -> OptionNames {
    let path = root.join("LR2files").join("Config").join("optionstr.csv");
    let Ok(bytes) = fs.read(&path) else {
        return OptionNames::default();
    };
    let m = reprise_skin_lr2csv::optionstr::parse_option_strings(&bytes);
    let get = |k: &str| m.get(k).cloned().unwrap_or_default();
    OptionNames {
        mode: get("#MODE"),
        sort: get("#SORT"),
        difficulty: get("#DIFFICULTY"),
    }
}

/// LR2 本体の `LR2files/Config/config.xml` で選んでいるスキン（名前 → LR2 本体のフォルダからのパス。無ければ空）
fn lr2_skins(fs: &dyn FileSystem, root: &Path) -> BTreeMap<String, String> {
    let path = root.join("LR2files").join("Config").join("config.xml");
    fs.read(&path)
        .map(|b| reprise_skin_lr2csv::lr2config::skin_paths(&b))
        .unwrap_or_default()
}

/// 使うスキンとそのスキン設定の選択（`config.json` の `skin_custom`）。`own` は `config.json` のスキン、
/// 無ければ LR2 本体の `config.xml` の `key` のスキン（LR2 本体のフォルダからのパス。スキンのパスはスキン中のパスと同じく
/// `--skin-root` から探す）
fn pick_skin(
    s: &Settings,
    own: &Option<PathBuf>,
    lr2: &BTreeMap<String, String>,
    key: &str,
) -> Option<(PathBuf, SkinCustom)> {
    let path = match own {
        Some(p) => p.clone(),
        None => PathBuf::from(lr2.get(key)?),
    };
    let custom = s.skin_custom.get(&path).cloned().unwrap_or_default();
    Some((path, custom))
}

fn main() {
    let args = platform::args();
    let fs = platform::file_system();
    // 先に --data だけ見て設定を読み、ほかの引数で上書きする
    let data_dir = args
        .iter()
        .position(|a| a == "--data")
        .and_then(|i| args.get(i + 1))
        .map_or_else(|| PathBuf::from(DATA_DIR), PathBuf::from);
    let store = platform::settings_store(&data_dir);
    let mut settings = match store.as_ref().map(|s| s.load()) {
        Some(Ok(Some(s))) => s,
        Some(Err(e)) => {
            eprintln!("設定を読めないので既定値を使う: {e:?}");
            Settings::default()
        }
        _ => Settings::default(),
    };
    let mut chart_path = None;
    let mut cli_songs = vec![];
    let mut auto = false;
    let usage = || -> ! { fatal(USAGE, 2) };
    let mut it = args.iter();
    while let Some(a) = it.next() {
        let mut value = || it.next().unwrap_or_else(|| usage());
        let mut number = || value().parse::<i32>().unwrap_or_else(|_| usage());
        let o = &mut settings.options;
        match a.as_str() {
            "--songs" => cli_songs.push(PathBuf::from(value())),
            "--skin" => settings.skin = Some(PathBuf::from(value())),
            "--select-skin" => settings.select_skin = Some(PathBuf::from(value())),
            "--skin-root" => settings.skin_root = Some(PathBuf::from(value())),
            "--gauge" => settings.gauge = parse_gauge(value()).unwrap_or_else(|| usage()),
            "--hs" => o.hi_speed = number(),
            "--hsfix" => o.hs_fix = parse_hs_fix(value()).unwrap_or_else(|| usage()),
            "--cover" => {
                o.lane_cover = true;
                o.cover = number();
            }
            "--offset" => o.judge_offset = number(),
            "--auto" => auto = true,
            "--data" => {
                value();
            }
            _ if a.starts_with("--") => usage(),
            _ => chart_path = Some(PathBuf::from(a)),
        }
    }
    if !cli_songs.is_empty() {
        settings.songs = cli_songs;
    }
    settings.options = settings.options.clamped();
    if chart_path.is_none() && settings.songs.is_empty() {
        usage();
    }
    let settings = SettingsRes {
        saved: settings,
        store,
    };
    // 起動引数で変えた設定は次の起動にも使う
    if matches!(settings.store.as_ref().map(|s| s.load()), Some(Ok(ref old)) if old.as_ref() != Some(&settings.saved))
    {
        save_settings(&settings);
    }
    let s = &settings.saved;
    let options = PlayOptions { auto, ..s.options };
    #[cfg(not(target_arch = "wasm32"))]
    let _ = platform::score_importers();
    let scores = ScoreDb(platform::open_scores(&data_dir).map(Mutex::new));
    let direct = chart_path.is_some();
    let table_feed = select_scene::TableFeed::default();
    let (library, feed, query) = if direct {
        (Library::default(), None, None)
    } else {
        let (lib, feed) = platform::library(&data_dir, &s.songs, fs.clone(), chart_loaders);
        eprintln!("譜面 {} 件", lib.songs().len());
        // 難易度表: 手元のものですぐ出し、手元に無いものは別のスレッドで取ってくる（beatoraja の起動時と同じ）
        let lib = match platform::table_source(&data_dir) {
            Some(src) => {
                let cached = application::tables::cached(src.as_ref(), &s.tables);
                let urls = s.tables.clone();
                let out = table_feed.0.clone();
                std::thread::spawn(move || {
                    if application::tables::fetch_missing(src.as_ref(), &urls) {
                        let t = application::tables::cached(src.as_ref(), &urls);
                        *out.lock().unwrap_or_else(|e| e.into_inner()) = Some(t);
                    }
                });
                lib.with_tables(cached)
            }
            None => lib,
        };
        (lib, feed, platform::song_query(&data_dir))
    };
    let skin_root = s
        .skin_root
        .clone()
        .unwrap_or_else(platform::default_skin_root);
    // config.json に無いスキンは LR2 本体の設定で選んでいるもの（LR2 と同じ）
    let lr2 = lr2_skins(fs.as_ref(), &skin_root);
    let play_skin = pick_skin(s, &s.skin, &lr2, "play_7");
    let boot = Boot {
        fs,
        gauge: s.gauge,
        options,
        skin_path: play_skin.as_ref().map(|(p, _)| p.clone()),
        skin_custom: play_skin.map(|(_, c)| c).unwrap_or_default(),
        skin_root,
        direct,
    };
    let input_map = input::InputMap::new(s.bindings.clone());
    let s_select = s.select;
    let option_names = option_names(boot.fs.as_ref(), &boot.skin_root);
    // 選曲スキン（読めなければ文字の一覧）
    let select_skin = match (pick_skin(s, &s.select_skin, &lr2, "select"), direct) {
        (Some((p, custom)), false) => load_skin(
            &boot.fs,
            &p,
            boot.skin_root.clone(),
            &[SceneKind::Select],
            &custom,
        )
        .map_err(|e| eprintln!("選曲スキンを読めないので文字の一覧で出す: {e}"))
        .ok(),
        _ => None,
    };
    // キー設定スキン（選曲スキンの BUTTON 13 から入る。無い・読めなければ BUTTON 13 は何もしない）
    let keyconfig_skin = match (
        pick_skin(s, &s.keyconfig_skin, &lr2, "keyconfig"),
        &select_skin,
    ) {
        (Some((p, custom)), Some(_)) => load_skin(
            &boot.fs,
            &p,
            boot.skin_root.clone(),
            &[SceneKind::KeyConfig],
            &custom,
        )
        .map_err(|e| eprintln!("キー設定スキンを読めない: {e}"))
        .ok(),
        _ => None,
    };
    // リザルトスキン（無い・読めなければ結果を文字で出す）
    let result_skin = pick_skin(s, &s.result_skin, &lr2, "result").and_then(|(p, custom)| {
        load_skin(
            &boot.fs,
            &p,
            boot.skin_root.clone(),
            &[SceneKind::Result],
            &custom,
        )
        .map_err(|e| eprintln!("リザルトスキンを読めないので結果を文字で出す: {e}"))
        .ok()
    });

    let mut app = App::new();
    app.add_plugins(DefaultPlugins.set(WindowPlugin {
        primary_window: Some(Window {
            title: "reprise".into(),
            resolution: WindowResolution::new(1280, 720),
            // LR2 と同じく入力を読むループをできるだけ速く回す（ブラウザでは画面の更新に合わせて回る）
            present_mode: PresentMode::AutoNoVsync,
            // ブラウザ版はページの canvas に描き、大きさをページに合わせる
            canvas: cfg!(target_arch = "wasm32").then(|| "#bevy".into()),
            fit_canvas_to_parent: true,
            ..default()
        }),
        ..default()
    }))
    .add_plugins(SkinRenderPlugin)
    .insert_state(if direct { Scene::Play } else { Scene::Select })
    .insert_resource(boot)
    .insert_resource(settings)
    .insert_resource(input_map)
    .init_resource::<input::SlotInput>()
    .insert_resource(scores)
    .insert_resource(select_scene::Selection::new(
        library,
        select_skin,
        s_select,
        option_names,
        query,
    ))
    .insert_resource(select_scene::LibraryFeed(feed))
    .insert_resource(table_feed)
    .init_resource::<preview::PreviewPlayer>()
    .add_systems(
        PreUpdate,
        input::read_input.after(bevy::input::InputSystems),
    )
    .add_systems(OnEnter(Scene::Select), select_scene::enter)
    .add_systems(OnExit(Scene::Select), preview::exit)
    .add_systems(
        Update,
        (
            select_scene::refresh,
            (select_scene::input, select_scene::draw)
                .chain()
                .run_if(|s: Res<select_scene::Selection>| !s.has_skin()),
            (select_scene::skin_input, select_scene::draw_skin)
                .chain()
                .run_if(|s: Res<select_scene::Selection>| s.has_skin()),
            select_scene::preview,
        )
            .chain()
            .run_if(in_state(Scene::Select)),
    )
    .add_systems(OnEnter(Scene::KeyConfig), keyconfig_scene::enter)
    .add_systems(OnExit(Scene::KeyConfig), keyconfig_scene::exit)
    .add_systems(
        Update,
        (keyconfig_scene::input, keyconfig_scene::draw)
            .chain()
            .run_if(in_state(Scene::KeyConfig)),
    )
    .add_systems(OnEnter(Scene::Result), result_scene::enter)
    .add_systems(
        Update,
        (result_scene::input, result_scene::draw).chain().run_if(
            in_state(Scene::Result).and_then(resource_exists::<result_scene::ResultScreen>),
        ),
    )
    .add_systems(OnEnter(Scene::Play), enter_play)
    .add_systems(OnExit(Scene::Play), exit_play)
    .add_systems(
        Update,
        (
            play_frame,
            leave_play,
            (draw_notes, draw_hud)
                .run_if(|g: Option<Res<Game>>| g.is_some_and(|g| g.skin.is_none())),
            draw_skin.run_if(|g: Option<Res<Game>>| g.is_some_and(|g| g.skin.is_some() && !g.over)),
            close_play.run_if(
                |g: Option<Res<Game>>, r: Option<Res<result_scene::ResultScreen>>| {
                    r.is_some() && g.is_some_and(|g| g.over)
                },
            ),
            show_result.run_if(
                |g: Option<Res<Game>>, r: Option<Res<result_scene::ResultScreen>>| {
                    r.is_none() && g.is_some_and(|g| g.over)
                },
            ),
        )
            .chain()
            .run_if(in_state(Scene::Play).and_then(resource_exists::<Game>)),
    )
    .add_systems(
        PostUpdate,
        skin_render::place_text
            .after(update_text2d_layout)
            .before(TransformSystems::Propagate),
    );
    if let Some((skin, pixels)) = keyconfig_skin {
        app.insert_resource(keyconfig_scene::KeyConfigScreen::new(skin, pixels));
    }
    if let Some((skin, pixels)) = result_skin {
        app.insert_resource(result_scene::ResultScreen::new(skin, pixels));
    }
    if let Some(path) = chart_path {
        app.insert_resource(PlayRequest {
            path,
            auto: false,
            seed: None,
        });
    }
    app.run();
}

/// 譜面とスキンを読み込んでプレイを始める。読めなければ選曲に戻る（直接起動なら終了）
fn enter_play(world: &mut World) {
    let Some(req) = world.remove_resource::<PlayRequest>() else {
        return;
    };
    let boot = world.resource::<Boot>();
    match prepare_play(boot, &req) {
        Ok((game, sounds, images, title)) => {
            world.insert_resource(game);
            world.insert_resource(PendingSounds(sounds));
            world.insert_resource(PendingImages(images));
            if let Ok(mut w) = world.query::<&mut Window>().single_mut(world) {
                w.title = title;
            }
            if let Err(e) = world.run_system_once(setup) {
                eprintln!("{e}");
            }
        }
        Err(e) if boot.direct => fatal(&e, 1),
        Err(e) => {
            eprintln!("{e}");
            world.resource_mut::<NextState<Scene>>().set(Scene::Select);
        }
    }
}

/// プレイの片付け（実体は `DespawnOnExit` で消える）
fn exit_play(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    old: Query<&Mesh2d, With<SkinSprite>>,
) {
    for m in &old {
        meshes.remove(&m.0);
    }
    commands.remove_resource::<Game>();
}

type Prepared = (Game, Sounds, Vec<Option<Pixels>>, String);

/// 譜面・音・スキン・BGA を読み、プレイの状態を作る
fn prepare_play(boot: &Boot, req: &PlayRequest) -> Result<Prepared, String> {
    let fs = &boot.fs;
    let options = PlayOptions {
        auto: boot.options.auto || req.auto,
        ..boot.options
    };
    let skin = boot
        .skin_path
        .as_ref()
        .map(|p| {
            load_skin(
                fs,
                p,
                boot.skin_root.clone(),
                &[SceneKind::Play7, SceneKind::Play5],
                &boot.skin_custom,
            )
        })
        .transpose()?;
    let chart_seed = req.seed.unwrap_or_else(seed);
    let (chart, sounds) = load(fs.as_ref(), &req.path, chart_seed)?;
    let input = KeyInput::new(
        SLOT_LANES
            .iter()
            .enumerate()
            .map(|(i, lane)| (i as KeyId, Lane(*lane))),
    );
    let gauge = boot.gauge;
    let mut session =
        PlaySession::new(&chart, gauge, input).ok_or("LR2 にこのゲージは無い".to_string())?;
    if options.auto {
        session.set_autoplay(options.judge_offset as i64);
    }
    let title = format!("reprise - {} [{}]", chart.meta.title, chart.meta.artist);
    let (skin_play, pending_images) = match skin {
        Some((skin, mut images)) => {
            let lengths: BTreeMap<SoundId, i64> =
                sounds.iter().map(|(id, b)| (*id, sound_ms(b))).collect();
            // 描画側の画像 = スキンの画像、BGA の画像（単色用の白 1 画素は `SkinGfx` が足す）
            let mut bga_images = BTreeMap::new();
            for (id, px) in load_bga(fs.as_ref(), &chart, &req.path) {
                bga_images.insert(id, images.len() as u32);
                images.push(Some(px));
            }
            let play = PlaySkin::new(
                &chart,
                &session,
                gauge,
                skin.timing,
                &skin.options,
                &lengths,
                &bga_images,
                options,
            );
            (
                Some(SkinPlay {
                    skin,
                    play,
                    gfx: SkinGfx::default(),
                    rng: LegacyMt::new(seed()),
                }),
                images,
            )
        }
        None => (None, vec![]),
    };
    let lead_in_ms = skin_play
        .as_ref()
        .map_or(LEAD_IN_MS, |sp| sp.play.lead_in_ms());
    let game = Game {
        path: req.path.clone(),
        seed: chart_seed,
        session,
        lead_in_ms,
        scratch: [false; 2],
        control: [false; 2],
        mouse_right: 0,
        save_mode: options.save_mode(),
        saved: None,
        over: false,
        start: None,
        last_judge: None,
        sounds: HashMap::new(),
        skin: skin_play,
    };
    Ok((game, sounds, pending_images, title))
}

/// LR2 スキンで描くための状態
struct SkinPlay {
    skin: Skin,
    play: PlaySkin,
    /// 描く側の持ち物（画像は `setup` で渡す）
    gfx: SkinGfx,
    /// ゲージのちらつき用（LR2 の `GetRand`）
    rng: LegacyMt,
}

#[derive(Resource)]
struct Game {
    /// 遊んでいる譜面と `#RANDOM` の乱数の種（リザルトからもう一度遊ぶため）
    path: PathBuf,
    seed: u32,
    session: PlaySession,
    /// 画面の開始から譜面の先頭までの時間
    lead_in_ms: i64,
    /// 皿の 2 つの操作（上・下）を押しているか
    scratch: [bool; 2],
    /// START・SELECT を押しているか
    control: [bool; 2],
    /// LR2 のマウスの右ボタンの状態（途中終了に使う）
    mouse_right: u8,
    /// 何を保存するか（None は保存しない）
    save_mode: Option<SaveMode>,
    /// 保存の結果（保存したら Some。保存しなかったら Some(None)）
    saved: Option<Option<SavedScore>>,
    /// 場面が終わった（結果を出す）
    over: bool,
    /// 最初のフレームで決める（起動にかかった時間で先頭を取りこぼさないように）
    start: Option<Instant>,
    /// 最後の判定と、そのフレーム時刻
    last_judge: Option<(Judge, i64)>,
    sounds: HashMap<SoundId, Handle<AudioSource>>,
    skin: Option<SkinPlay>,
}

impl Game {
    fn now_ms(&self) -> i64 {
        self.start.map_or(0, |s| s.elapsed().as_millis() as i64) - self.lead_in_ms
    }
}

#[derive(Resource)]
struct PendingSounds(Sounds);

#[derive(Resource)]
struct PendingImages(Vec<Option<Pixels>>);

#[derive(Component)]
struct NoteSprite;

#[derive(Component)]
struct Hud;

fn lane_x(lane: u8) -> (f32, f32) {
    if lane == 0 {
        (LANE_LEFT + SCRATCH_W / 2.0, SCRATCH_W)
    } else {
        (LANE_LEFT + SCRATCH_W + KEY_W * (lane as f32 - 0.5), KEY_W)
    }
}

fn setup(
    mut commands: Commands,
    mut pending: ResMut<PendingSounds>,
    mut pending_images: ResMut<PendingImages>,
    mut audio: ResMut<Assets<AudioSource>>,
    mut images: ResMut<Assets<Image>>,
    mut game: ResMut<Game>,
) {
    for (id, bytes) in pending.0.drain(..) {
        let h = audio.add(AudioSource { bytes });
        game.sounds.insert(id, h);
    }
    if let Some(sp) = game.skin.as_mut() {
        commands.spawn((
            skin_render::camera(sp.skin.resolution),
            DespawnOnExit(Scene::Play),
        ));
        let pixels = std::mem::take(&mut pending_images.0);
        sp.gfx = SkinGfx::new(pixels, sp.skin.resolution, &mut images);
        return;
    }
    commands.spawn((Camera2d, DespawnOnExit(Scene::Play)));
    // レーンの背景と判定ライン
    for lane in 0..=7u8 {
        let (x, w) = lane_x(lane);
        let c = if lane % 2 == 0 { 0.08 } else { 0.12 };
        commands.spawn((
            Sprite::from_color(Color::srgb(c, c, c), Vec2::new(w - 2.0, LANE_H)),
            Transform::from_xyz(x, JUDGE_Y + LANE_H / 2.0, 0.0),
            DespawnOnExit(Scene::Play),
        ));
    }
    let width = SCRATCH_W + KEY_W * 7.0;
    commands.spawn((
        Sprite::from_color(Color::srgb(0.9, 0.2, 0.2), Vec2::new(width, 4.0)),
        Transform::from_xyz(LANE_LEFT + width / 2.0, JUDGE_Y, 1.0),
        DespawnOnExit(Scene::Play),
    ));
    commands.spawn((
        Text::new(""),
        TextFont::from_font_size(22.0),
        Node {
            position_type: PositionType::Absolute,
            top: px(20),
            left: px(500),
            ..default()
        },
        Hud,
        DespawnOnExit(Scene::Play),
    ));
}

/// キー音と BGM を Bevy の音として鳴らす
struct BevyAudio<'a, 'w, 's> {
    commands: &'a mut Commands<'w, 's>,
    sounds: &'a HashMap<SoundId, Handle<AudioSource>>,
}

impl AudioOutput for BevyAudio<'_, '_, '_> {
    fn play(&mut self, sound: SoundId, _at_us: i64) {
        if let Some(h) = self.sounds.get(&sound) {
            self.commands.spawn((
                AudioPlayer::new(h.clone()),
                PlaybackSettings::DESPAWN,
                DespawnOnExit(Scene::Play),
            ));
        }
    }
}

/// Esc で選曲に戻る（譜面を直接渡して起動したなら終了）。プレイスキンとリザルトスキンで遊んでいるときは
/// LR2 と同じ途中終了（`play_frame`。`#FADEOUT` の後に場面を閉じる）にする
fn leave_play(
    keys: Res<ButtonInput<KeyCode>>,
    boot: Res<Boot>,
    game: Res<Game>,
    result: Option<Res<result_scene::ResultScreen>>,
    mut next: ResMut<NextState<Scene>>,
    mut exit: MessageWriter<AppExit>,
) {
    if result.is_some() && game.skin.is_some() {
        return;
    }
    if keys.just_pressed(KeyCode::Escape) {
        if boot.direct {
            exit.write(AppExit::Success);
        } else {
            next.set(Scene::Select);
        }
    }
}

/// 1 ループ = 1 フレーム。入力を読んで `PlaySession` を進める
#[allow(clippy::too_many_arguments)]
fn play_frame(
    mut commands: Commands,
    input: Res<input::SlotInput>,
    keys: Res<ButtonInput<KeyCode>>,
    buttons: Res<ButtonInput<MouseButton>>,
    result: Option<Res<result_scene::ResultScreen>>,
    mut wheel: MessageReader<MouseWheel>,
    mut game: ResMut<Game>,
    mut scores: ResMut<ScoreDb>,
) {
    let mut events: Vec<(KeyId, bool)> = vec![];
    // START・SELECT は前のフレームから押し続けているかを見る
    let mut ctrl = ControlInput {
        start_held: game.control[0],
        select_held: game.control[1],
        ..Default::default()
    };
    // SP のプレイは 1P と 2P の割り当てをまとめて 1P の操作として読む（LR2 `InputToButton` の player 1。DOM-INP-05）
    for &(slot, down) in &input.single.events {
        if slot == START || slot == SELECT {
            let i = slot - START;
            game.control[i] = down;
            if down && i == 0 {
                ctrl.start_pressed = true;
            } else if down {
                ctrl.select_pressed = true;
            }
            continue;
        }
        events.push((slot as KeyId, down));
        if slot < 2 {
            game.scratch[slot] = down;
        } else if down {
            ctrl.keys_pressed[slot - 2] = true;
        }
    }
    ctrl.start_held &= game.control[0];
    ctrl.select_held &= game.control[1];
    ctrl.wheel = wheel.read().map(|w| w.y.signum() as i32).sum();
    if game.start.is_none() {
        game.start = Some(Instant::now());
    }
    let t = game.now_ms();
    let game = &mut *game;
    game.mouse_right =
        result_scene::right_state(game.mouse_right, buttons.pressed(MouseButton::Right));
    // 途中終了した後は判定しない（APP-SKN-14）
    let quitting = game.skin.as_ref().is_some_and(|sp| sp.play.quitting());
    let out = if quitting {
        vec![]
    } else {
        let mut audio = BevyAudio {
            commands: &mut commands,
            sounds: &game.sounds,
        };
        game.session.frame(t, &events, &mut audio)
    };
    if let Some(sp) = game.skin.as_mut() {
        // 皿の向き: 1 つ目のキーは上、2 つ目は下（両方なら止める）
        sp.play
            .set_scratch(game.scratch[1] as i32 - game.scratch[0] as i32);
        sp.play.update(t, &game.session, &out);
        sp.play.control(&ctrl);
        // 途中終了（APP-SKN-14）はリザルトスキンがあるとき（無ければ Esc ですぐ選曲に戻る）
        if result.is_some() {
            let quit = QuitInput {
                escape: keys.pressed(KeyCode::Escape),
                mouse_right: game.mouse_right,
                start_held: game.control[0],
                select_held: game.control[1],
            };
            sp.play.quit(&quit, game.session.play().total_notes());
        }
        if sp.play.scene_over() {
            game.over = true;
        }
    }
    // 終わったら 1 回だけ記録を保存する
    if game.session.play().is_finished() {
        save_once(game, &mut scores);
    }
    for e in out {
        if let PlayEvent::Judge { judge, .. } = e {
            game.last_judge = Some((judge, t));
        }
    }
}

/// 記録を 1 回だけ保存する
fn save_once(game: &mut Game, scores: &mut ScoreDb) {
    if game.saved.is_some() {
        return;
    }
    let record = game.session.record();
    let saved = match scores.0.as_mut().and_then(|m| m.get_mut().ok()) {
        Some(db) => application::usecase::save_play(db.as_mut(), &record, game.save_mode)
            .map_err(|e| eprintln!("記録を保存できない: {e:?}"))
            .ok()
            .flatten(),
        None => None,
    };
    game.saved = Some(saved);
}

/// プレイの場面が閉じたら（リザルトスキンがあるとき）、LR2 と同じくリザルトか選曲（譜面を直接渡して起動したなら終了）へ
fn close_play(
    mut commands: Commands,
    mut game: ResMut<Game>,
    mut scores: ResMut<ScoreDb>,
    boot: Res<Boot>,
    mut next: ResMut<NextState<Scene>>,
    mut exit: MessageWriter<AppExit>,
) {
    let game = &mut *game;
    // LR2 は場面を閉じるときに保存する（まだなら）
    save_once(game, &mut scores);
    // 場面が閉じるのはプレイスキンで描いているときだけ
    let result = game
        .skin
        .as_ref()
        .map(|sp| sp.play.result(&game.session, game.saved.clone().flatten()));
    let auto = game.session.is_autoplay();
    let to = result
        .as_ref()
        .map(|r| after_play(auto, r.processed, &r.counts));
    match (to, result) {
        (Some(AfterPlay::Result), Some(result)) => {
            commands.insert_resource(result_scene::PendingResult {
                result,
                path: game.path.clone(),
                seed: game.seed,
            });
            next.set(Scene::Result);
        }
        _ if boot.direct => {
            exit.write(AppExit::Success);
        }
        _ => next.set(Scene::Select),
    }
}

/// LR2 スキンで描く（毎フレーム作り直す）
fn draw_skin(mut p: SkinDraw, mut game: ResMut<Game>) {
    let game = &mut *game;
    let Some(sp) = game.skin.as_mut() else {
        return;
    };
    let tremble = sp.rng.get_rand(2) as i32;
    // 画面の高さの 2 倍先まで
    let visible = sp.skin.resolution.1 as f64 * 2.0;
    let mut view = sp.play.view(&game.session, visible, tremble);
    // 途中終了したらノーツは描かない
    if game.session.play().is_dead() {
        view.notes.clear();
    }
    let gfx = &sp.gfx;
    let draws = draw_play(&sp.skin, &sp.play, &view, &|i| gfx.size(i));
    skin_render::render(&mut p, &mut sp.gfx, draws, Scene::Play);
}

#[derive(Component)]
struct ResultText;

/// 場面が終わったら結果を文字で出す（リザルトスキンが無いとき）
fn show_result(
    mut commands: Commands,
    mut game: ResMut<Game>,
    mut meshes: ResMut<Assets<Mesh>>,
    old: Query<(Entity, &Mesh2d), With<SkinSprite>>,
    shown: Query<(), With<ResultText>>,
) {
    if !shown.is_empty() {
        return;
    }
    for (e, m) in &old {
        meshes.remove(&m.0);
        commands.entity(e).despawn();
    }
    if let Some(sp) = game.skin.as_mut() {
        sp.gfx.despawn_texts(&mut commands);
    }
    let play = game.session.play();
    let c = play.counts();
    commands.spawn((
        Text::new(format!(
            "RESULT: {:?}\nEX SCORE {}\nPG {}  GR {}  GD {}  BD {}  PR {}\nMAX COMBO {} / {}\n{}\n\nPress Esc to finish",
            game.session.record().lamp,
            c.ex_score(),
            c.pgreat,
            c.great,
            c.good,
            c.bad,
            c.poor,
            play.max_combo(),
            play.total_notes(),
            best_text(game.saved.as_ref()),
        )),
        TextFont::from_font_size(28.0),
        Node {
            position_type: PositionType::Absolute,
            top: px(40),
            left: px(40),
            ..default()
        },
        ResultText,
        DespawnOnExit(Scene::Play),
    ));
}

/// 保存した記録の前後
fn best_text(saved: Option<&Option<SavedScore>>) -> String {
    let Some(Some(s)) = saved else {
        // ブラウザ版の既定のフォントには日本語の字が無い
        return if cfg!(target_arch = "wasm32") {
            "(score is not saved)"
        } else {
            "（記録は保存していません）"
        }
        .into();
    };
    let line = |r: &ScoreRecord| {
        format!(
            "{:?}  EX {}  MISS {}  PLAY {}",
            r.lamp,
            r.ex_score,
            r.min_bp.map_or("-".into(), |b| b.to_string()),
            r.play_count
        )
    };
    let before = s.before.as_ref().map_or("-".into(), line);
    format!("\nBEST {before}\n  -> {}", line(&s.after))
}

fn draw_notes(mut commands: Commands, game: Res<Game>, old: Query<Entity, With<NoteSprite>>) {
    for e in &old {
        commands.entity(e).despawn();
    }
    // 途中終了したらノーツは描かない
    if game.session.play().is_dead() {
        return;
    }
    let t = game.now_ms();
    for VisibleNote {
        lane,
        y,
        y_end,
        mine,
        ..
    } in game.session.visible_notes(t, VISIBLE_BEATS)
    {
        if lane.0 > 7 {
            continue;
        }
        let (x, w) = lane_x(lane.0);
        let color = if mine {
            Color::srgb(0.9, 0.1, 0.1)
        } else if lane.0 == 0 {
            Color::srgb(0.9, 0.3, 0.3)
        } else if lane.0 % 2 == 1 {
            Color::srgb(0.9, 0.9, 0.9)
        } else {
            Color::srgb(0.3, 0.5, 1.0)
        };
        let head = JUDGE_Y + y as f32 * LANE_H;
        if let Some(end) = y_end {
            let tail = JUDGE_Y + (end.min(1.0) as f32) * LANE_H;
            commands.spawn((
                Sprite::from_color(
                    color.with_alpha(0.5),
                    Vec2::new(w - 10.0, (tail - head).max(0.0)),
                ),
                Transform::from_xyz(x, (head + tail) / 2.0, 2.0),
                NoteSprite,
                DespawnOnExit(Scene::Play),
            ));
        }
        commands.spawn((
            Sprite::from_color(color, Vec2::new(w - 4.0, NOTE_H)),
            Transform::from_xyz(x, head, 3.0),
            NoteSprite,
            DespawnOnExit(Scene::Play),
        ));
    }
}

fn draw_hud(game: Res<Game>, mut hud: Query<&mut Text, With<Hud>>) {
    let Ok(mut text) = hud.single_mut() else {
        return;
    };
    let t = game.now_ms();
    let play = game.session.play();
    let c = play.counts();
    let judge = match game.last_judge {
        Some((j, at)) if t - at < 500 => match j {
            Judge::PGreat => "PGREAT",
            Judge::Great => "GREAT",
            Judge::Good => "GOOD",
            Judge::Bad => "BAD",
            Judge::Poor | Judge::EmptyPoor => "POOR",
        },
        _ => "",
    };
    let mut s = format!(
        "{judge} {combo}\n\nGAUGE {gauge:.0}%\nEX SCORE {ex}\nPG {pg}  GR {gr}  GD {gd}  BD {bd}  PR {pr}\nMAX COMBO {max} / {total}",
        combo = if play.combo() > 0 { play.combo().to_string() } else { String::new() },
        gauge = play.gauge(),
        ex = c.ex_score(),
        pg = c.pgreat,
        gr = c.great,
        gd = c.good,
        bd = c.bad,
        pr = c.poor,
        max = play.max_combo(),
        total = play.total_notes(),
    );
    if play.is_finished() {
        s += &format!(
            "\n\nRESULT: {:?}\n{}\nPress Esc to finish",
            game.session.record().lamp,
            best_text(game.saved.as_ref())
        );
    }
    text.0 = s;
}
