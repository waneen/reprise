//! Composition Root + プレゼンテーション層（Bevy はここだけ）。
//! 記法を増やす = chart-formats/ か skin-formats/ に crate を足して、ここで 1 行登録するだけ。
//!
//! 使い方: `reprise <譜面ファイル> [--gauge groove|easy|hard|death|pattack|gattack] [--skin <スキン> [--skin-root <フォルダ>]]`
//! 1P の 7 鍵（5 鍵）のプレイ画面。`--skin` で LR2 のプレイスキンで描き、無ければ四角形だけで描く。
//! `--skin-root` はスキン中のパスの基準（LR2 なら LR2 本体のフォルダ。既定は今のフォルダ）。
mod skin_render;

use application::play::{KeyId, KeyInput, PlaySession, VisibleNote};
use application::play_skin::PlaySkin;
use application::ports::{AudioOutput, ChartLoader, ScoreImporter, SkinLoader};
use bevy::asset::RenderAssetUsages;
use bevy::audio::AudioSource;
use bevy::camera::visibility::NoFrustumCulling;
use bevy::camera::{OrthographicProjection, Projection, ScalingMode};
use bevy::image::ImageSampler;
use bevy::input::keyboard::KeyboardInput;
use bevy::input::ButtonState;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use bevy::sprite::{update_text2d_layout, Anchor};
use bevy::text::{FontSize, FontSource, FontWeight, TextLayoutInfo};
use bevy::transform::TransformSystems;
use bevy::window::{PresentMode, WindowResolution};
use chart_loader::DecoderLoader;
use domain::chart::{Chart, ImageId, Lane, SoundId};
use domain::play::{GaugeType, Judge, PlayEvent};
use domain::random::LegacyMt;
use domain::skin::lr2::{draw_play, pixels_per_beat, Draw, Quad, TextRun};
use domain::skin::{SceneKind, Skin};
use skin_loader::DecoderSkinLoader;
use skin_render::{SkinMaterial, SkinRenderPlugin};
use std::collections::{BTreeMap, HashMap};
use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

/// 再生開始から 0ms（譜面の先頭）までの待ち時間（スキン無しのとき。スキンがあればスキンの場面の時間）
const LEAD_IN_MS: i64 = 2000;
/// 判定ラインから上端までに見える拍数（スキン無しのとき）
const VISIBLE_BEATS: f64 = 4.0;
/// ハイスピード（×100。LR2 の既定）
const HI_SPEED: f64 = 200.0;

/// 既定のキー配置（1P）。皿は 2 つのキーで左右回転を表す
const KEYMAP: [(KeyCode, u8); 9] = [
    (KeyCode::ShiftLeft, 0),
    (KeyCode::ControlLeft, 0),
    (KeyCode::KeyZ, 1),
    (KeyCode::KeyS, 2),
    (KeyCode::KeyX, 3),
    (KeyCode::KeyD, 4),
    (KeyCode::KeyC, 5),
    (KeyCode::KeyF, 6),
    (KeyCode::KeyV, 7),
];

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

/// スキンの読み込み（描けるのは LR2 のプレイスキン）
fn skin_loaders(root: PathBuf) -> Vec<Box<dyn SkinLoader>> {
    vec![
        Box::new(DecoderSkinLoader {
            decoder: reprise_skin_lr2csv::Lr2CsvDecoder::default(),
            settings: Default::default(),
            root: root.clone(),
            // LR2 の既定の抜き色
            default_color_key: Some([0, 255, 0]),
        }),
        Box::new(DecoderSkinLoader {
            decoder: reprise_skin_beatoraja::BeatorajaJsonDecoder,
            settings: Default::default(),
            root,
            default_color_key: None,
        }),
    ]
}

/// 読み込んだスキン画像（幅, 高さ, RGBA）
type Pixels = (u32, u32, Vec<u8>);

/// スキン画像を RGBA にする。アルファの無い画像は抜き色の画素を透明にする
fn load_image(path: &Path, color_key: Option<[u8; 3]>) -> Result<Pixels, String> {
    let img = image::ImageReader::open(path)
        .and_then(|r| r.with_guessed_format())
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

/// スキンと画像を読み込む
fn load_skin(path: &Path, root: PathBuf) -> Result<(Skin, Vec<Option<Pixels>>), String> {
    let loaders = skin_loaders(root);
    let loader = loaders
        .iter()
        .find(|l| l.can_load(path))
        .ok_or_else(|| format!("スキンとして読めない: {}", path.display()))?;
    let mut rng = LegacyMt::new(seed());
    let skin = loader
        .load(path, &mut rng)
        .map_err(|e| format!("スキンを読めない: {e:?}"))?;
    if !matches!(skin.scene, SceneKind::Play7 | SceneKind::Play5) {
        return Err(format!(
            "7 鍵・5 鍵のプレイスキンではない: {:?}",
            skin.scene
        ));
    }
    let images = skin
        .images
        .iter()
        .map(|p| {
            let p = p.as_ref()?;
            load_image(p, skin.color_key)
                .map_err(|e| eprintln!("スキン画像を読めない: {e}"))
                .ok()
        })
        .collect();
    Ok((skin, images))
}

fn seed() -> u32 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u32)
        .unwrap_or(0)
}

/// 記録の取り込み（まだ画面からは使わない。登録だけしておく）
fn score_importers() -> Vec<Box<dyn ScoreImporter>> {
    vec![
        Box::new(score_import_lr2::Lr2ScoreDb),
        Box::new(score_import_beatoraja::BeatorajaScoreDb::beatoraja()),
        Box::new(score_import_beatoraja::BeatorajaScoreDb::lr2oraja()),
    ]
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
type Sounds = Vec<(SoundId, Arc<[u8]>)>;

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
fn load_bga(chart: &Chart, chart_path: &Path) -> Vec<(ImageId, Pixels)> {
    let dir = chart_path.parent().unwrap_or(Path::new("."));
    chart
        .images
        .iter()
        .filter_map(|(id, name)| {
            let Some(p) = chart_loader::find_image_file(dir, name) else {
                eprintln!("画像が見つからない: {name}");
                return None;
            };
            load_image(&p, Some([0, 0, 0]))
                .map_err(|e| eprintln!("画像を読めない: {e}"))
                .ok()
                .map(|px| (*id, px))
        })
        .collect()
}

/// 譜面と音を読み込む
fn load(path: &Path) -> Result<(Chart, Sounds), String> {
    let bytes = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let (chart, _) = application::usecase::load_chart(&chart_loaders(), path, &bytes, seed())
        .map_err(|e| format!("譜面を読めない: {e:?}"))?;
    let dir = path.parent().unwrap_or(Path::new("."));
    let mut sounds = vec![];
    for (id, name) in &chart.sounds {
        let Some(p) = chart_loader::find_sound_file(dir, name) else {
            eprintln!("音が見つからない: {name}");
            continue;
        };
        match std::fs::read(&p) {
            Ok(b) => {
                let b: Arc<[u8]> = b.into();
                if decodable(&b) {
                    sounds.push((*id, b));
                } else {
                    eprintln!("音を読めない: {}", p.display());
                }
            }
            Err(e) => eprintln!("{}: {e}", p.display()),
        }
    }
    Ok((chart, sounds))
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut chart_path = None;
    let mut gauge = GaugeType::Groove;
    let mut skin_path = None;
    let mut skin_root = std::env::current_dir().unwrap_or_default();
    let mut it = args.iter();
    while let Some(a) = it.next() {
        if a == "--skin" || a == "--skin-root" {
            let Some(v) = it.next() else {
                eprintln!("{a} の後にパスが要る");
                std::process::exit(2);
            };
            if a == "--skin" {
                skin_path = Some(PathBuf::from(v));
            } else {
                skin_root = PathBuf::from(v);
            }
        } else if a == "--gauge" {
            match it.next().and_then(|g| parse_gauge(g)) {
                Some(g) => gauge = g,
                None => {
                    eprintln!("--gauge は groove|easy|hard|death|pattack|gattack");
                    std::process::exit(2);
                }
            }
        } else {
            chart_path = Some(PathBuf::from(a));
        }
    }
    let Some(chart_path) = chart_path else {
        eprintln!(
            "使い方: reprise <譜面ファイル> [--gauge groove|easy|hard|death|pattack|gattack] [--skin <スキン> [--skin-root <フォルダ>]]"
        );
        std::process::exit(2);
    };
    let _ = score_importers();
    let skin = match skin_path.map(|p| load_skin(&p, skin_root)).transpose() {
        Ok(v) => v,
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(1);
        }
    };
    let (chart, sounds) = match load(&chart_path) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(1);
        }
    };
    let input = KeyInput::new(
        KEYMAP
            .iter()
            .enumerate()
            .map(|(i, (_, lane))| (i as KeyId, Lane(*lane))),
    );
    let Some(session) = PlaySession::new(&chart, gauge, input) else {
        eprintln!("LR2 にこのゲージは無い");
        std::process::exit(2);
    };
    let title = format!("reprise - {} [{}]", chart.meta.title, chart.meta.artist);
    let (skin_play, pending_images) = match skin {
        Some((skin, mut images)) => {
            let lengths: BTreeMap<SoundId, i64> =
                sounds.iter().map(|(id, b)| (*id, sound_ms(b))).collect();
            // 描画側の画像 = スキンの画像、BGA の画像、単色用の白 1 画素 の順
            let mut bga_images = BTreeMap::new();
            for (id, px) in load_bga(&chart, &chart_path) {
                bga_images.insert(id, images.len() as u32);
                images.push(Some(px));
            }
            let white = images.len() as u32;
            images.push(Some((1, 1, vec![255; 4])));
            let play = PlaySkin::new(
                &chart,
                &session,
                gauge,
                skin.timing,
                &skin.options,
                &lengths,
                &bga_images,
            );
            let sizes = images
                .iter()
                .map(|i| i.as_ref().map(|(w, h, _)| (*w, *h)))
                .collect();
            (
                Some(SkinPlay {
                    skin,
                    play,
                    handles: vec![],
                    materials: HashMap::new(),
                    sizes,
                    rng: LegacyMt::new(seed()),
                    texts: vec![],
                    white,
                }),
                images,
            )
        }
        None => (None, vec![]),
    };
    let lead_in_ms = skin_play
        .as_ref()
        .map_or(LEAD_IN_MS, |sp| sp.play.lead_in_ms());

    App::new()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title,
                resolution: WindowResolution::new(1280, 720),
                // LR2 と同じく入力を読むループをできるだけ速く回す
                present_mode: PresentMode::AutoNoVsync,
                ..default()
            }),
            ..default()
        }))
        .add_plugins(SkinRenderPlugin)
        .insert_resource(Game {
            session,
            lead_in_ms,
            scratch: [false; 2],
            over: false,
            start: None,
            last_judge: None,
            sounds: HashMap::new(),
            skin: skin_play,
        })
        .insert_resource(PendingSounds(sounds))
        .insert_resource(PendingImages(pending_images))
        .add_systems(Startup, setup)
        .add_systems(
            Update,
            (
                play_frame,
                (draw_notes, draw_hud).run_if(|g: Res<Game>| g.skin.is_none()),
                draw_skin.run_if(|g: Res<Game>| g.skin.is_some() && !g.over),
                show_result.run_if(|g: Res<Game>| g.over),
            )
                .chain(),
        )
        .add_systems(
            PostUpdate,
            place_text
                .after(update_text2d_layout)
                .before(TransformSystems::Propagate),
        )
        .run();
}

/// LR2 スキンで描くための状態
struct SkinPlay {
    skin: Skin,
    play: PlaySkin,
    /// スキン画像（読めなかったものは None）。フィルタ無し・有りの 2 つ
    handles: Vec<Option<[Handle<Image>; 2]>>,
    /// （画像, 合成モード, フィルタ）ごとのマテリアル
    materials: HashMap<(u32, u8, bool), Handle<SkinMaterial>>,
    sizes: Vec<Option<(u32, u32)>>,
    /// ゲージのちらつき用（LR2 の `GetRand`）
    rng: LegacyMt,
    /// システムフォントの文字列の実体（使い回す）
    texts: Vec<Entity>,
    /// 単色の四角形に使う白 1 画素の画像の番号
    white: u32,
}

#[derive(Resource)]
struct Game {
    session: PlaySession,
    /// 画面の開始から譜面の先頭までの時間
    lead_in_ms: i64,
    /// 皿の 2 つのキー（KEYMAP の 0, 1 番）を押しているか
    scratch: [bool; 2],
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
struct SkinSprite;

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
        // スキンの解像度を窓いっぱいに（縦横比は保つ）
        let (w, h) = sp.skin.resolution;
        commands.spawn((
            Camera2d,
            Projection::Orthographic(OrthographicProjection {
                scaling_mode: ScalingMode::AutoMin {
                    min_width: w as f32,
                    min_height: h as f32,
                },
                ..OrthographicProjection::default_2d()
            }),
        ));
        sp.handles = pending_images
            .0
            .drain(..)
            .map(|p| {
                let (w, h, data) = p?;
                let mut img = |sampler| {
                    let mut image = Image::new(
                        Extent3d {
                            width: w,
                            height: h,
                            depth_or_array_layers: 1,
                        },
                        TextureDimension::D2,
                        data.clone(),
                        TextureFormat::Rgba8UnormSrgb,
                        RenderAssetUsages::default(),
                    );
                    image.sampler = sampler;
                    images.add(image)
                };
                Some([img(ImageSampler::nearest()), img(ImageSampler::linear())])
            })
            .collect();
        return;
    }
    commands.spawn(Camera2d);
    // レーンの背景と判定ライン
    for lane in 0..=7u8 {
        let (x, w) = lane_x(lane);
        let c = if lane % 2 == 0 { 0.08 } else { 0.12 };
        commands.spawn((
            Sprite::from_color(Color::srgb(c, c, c), Vec2::new(w - 2.0, LANE_H)),
            Transform::from_xyz(x, JUDGE_Y + LANE_H / 2.0, 0.0),
        ));
    }
    let width = SCRATCH_W + KEY_W * 7.0;
    commands.spawn((
        Sprite::from_color(Color::srgb(0.9, 0.2, 0.2), Vec2::new(width, 4.0)),
        Transform::from_xyz(LANE_LEFT + width / 2.0, JUDGE_Y, 1.0),
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
            self.commands
                .spawn((AudioPlayer::new(h.clone()), PlaybackSettings::DESPAWN));
        }
    }
}

/// 1 ループ = 1 フレーム。入力を読んで `PlaySession` を進める
fn play_frame(
    mut commands: Commands,
    mut keys: MessageReader<KeyboardInput>,
    mut game: ResMut<Game>,
    mut exit: MessageWriter<AppExit>,
) {
    let mut events: Vec<(KeyId, bool)> = vec![];
    for k in keys.read() {
        if k.repeat {
            continue;
        }
        if k.key_code == KeyCode::Escape && k.state == ButtonState::Pressed {
            exit.write(AppExit::Success);
        }
        if let Some(i) = KEYMAP.iter().position(|(code, _)| *code == k.key_code) {
            let down = k.state == ButtonState::Pressed;
            events.push((i as KeyId, down));
            if i < 2 {
                game.scratch[i] = down;
            }
        }
    }
    if game.start.is_none() {
        game.start = Some(Instant::now());
    }
    let t = game.now_ms();
    let game = &mut *game;
    let mut audio = BevyAudio {
        commands: &mut commands,
        sounds: &game.sounds,
    };
    let out = game.session.frame(t, &events, &mut audio);
    if let Some(sp) = game.skin.as_mut() {
        // 皿の向き: 1 つ目のキーは上、2 つ目は下（両方なら止める）
        sp.play
            .set_scratch(game.scratch[1] as i32 - game.scratch[0] as i32);
        sp.play.update(t, &game.session, &out);
        if sp.play.scene_over() {
            game.over = true;
        }
    }
    for e in out {
        if let PlayEvent::Judge { judge, .. } = e {
            game.last_judge = Some((judge, t));
        }
    }
}

/// システムフォントの文字列（描く位置と大きさは文字列の幅が決まってから [`place_text`] で決める）。
/// Bevy は見えている文字列しか組まないので、実体は使い回して見えたままにする
#[derive(Component)]
struct SkinText {
    run: Option<TextRun>,
    z: f32,
    res: (u32, u32),
}

/// LR2 スキンで描く（毎フレーム作り直す）。同じ画像・合成モード・フィルタが続く四角形を 1 つのメッシュにする
fn draw_skin(
    mut commands: Commands,
    mut game: ResMut<Game>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<SkinMaterial>>,
    old: Query<(Entity, &Mesh2d), With<SkinSprite>>,
    mut texts: Query<(&mut Text2d, &mut TextFont, &mut TextColor, &mut SkinText)>,
) {
    for (e, m) in &old {
        meshes.remove(&m.0);
        commands.entity(e).despawn();
    }
    let game = &mut *game;
    let Some(sp) = game.skin.as_mut() else {
        return;
    };
    let ppb = pixels_per_beat(HI_SPEED);
    let visible = sp.skin.resolution.1 as f64 * 2.0 / ppb;
    let tremble = sp.rng.get_rand(2) as i32;
    let mut view = sp.play.view(&game.session, HI_SPEED, visible, tremble);
    // 途中終了したらノーツは描かない
    if game.session.play().is_dead() {
        view.notes.clear();
    }
    let sizes = &sp.sizes;
    let size = |i: u32| sizes.get(i as usize).copied().flatten();
    let draws = draw_play(&sp.skin, &sp.play, &view, &size);
    let loaded = |i: u32| {
        sp.handles
            .get(i as usize)
            .is_some_and(|h| h.is_some())
            .then(|| size(i))
            .flatten()
    };
    // 奥から順に z を増やす。続く四角形はまとめ、文字列はその間に挟む
    let mut z = 0.0;
    let mut quads: Vec<Quad> = vec![];
    let mut runs: Vec<(TextRun, f32)> = vec![];
    let mut flush = |quads: &mut Vec<Quad>, z: &mut f32, commands: &mut Commands| {
        for b in skin_render::batches(quads, sp.skin.resolution, &loaded) {
            let Some(Some(h)) = sp.handles.get(b.image as usize) else {
                continue;
            };
            let texture = h[b.filter as usize].clone();
            let material = sp
                .materials
                .entry((b.image, b.blend as u8, b.filter))
                .or_insert_with(|| {
                    materials.add(SkinMaterial {
                        texture,
                        blend: b.blend,
                    })
                })
                .clone();
            commands.spawn((
                Mesh2d(meshes.add(b.mesh)),
                MeshMaterial2d(material),
                Transform::from_xyz(0.0, 0.0, *z),
                SkinSprite,
            ));
            *z += 0.001;
        }
        quads.clear();
    };
    for d in draws {
        match d {
            Draw::Quad(q) => quads.push(q),
            Draw::Fill(f) => quads.push(Quad {
                image: sp.white,
                src: [0.0, 0.0, 1.0, 1.0],
                dst: f.dst,
                argb: f.argb,
                angle: 0.0,
                center: 0,
                blend: f.blend,
                filter: false,
            }),
            Draw::Text(run) => {
                flush(&mut quads, &mut z, &mut commands);
                runs.push((run, z));
                z += 0.001;
            }
        }
    }
    flush(&mut quads, &mut z, &mut commands);
    // 文字列の実体を使い回す（足りなければ作り、余りは空にする）
    let res = sp.skin.resolution;
    for i in 0..runs.len().max(sp.texts.len()) {
        let run = runs.get(i).cloned();
        let style = |r: &TextRun| {
            let [a, red, g, b] = r.argb.map(|c| c as f32 / 255.0);
            (
                TextFont {
                    font: FontSource::SansSerif,
                    font_size: FontSize::Px(r.font.size.max(1) as f32),
                    weight: FontWeight((r.font.thickness.clamp(1, 9) * 100) as u16),
                    ..default()
                },
                TextColor(Color::srgba(red, g, b, a)),
            )
        };
        match sp.texts.get(i).and_then(|e| texts.get_mut(*e).ok()) {
            Some((mut text, mut tf, mut color, mut st)) => {
                let s = run.as_ref().map_or(String::new(), |(r, _)| r.text.clone());
                if text.0 != s {
                    text.0 = s;
                }
                if let Some((r, z)) = &run {
                    let (f, c) = style(r);
                    if tf.font_size != f.font_size || tf.weight != f.weight {
                        *tf = f;
                    }
                    color.set_if_neq(c);
                    st.z = *z;
                }
                st.run = run.map(|(r, _)| r);
            }
            None => {
                let Some((r, z)) = run else { continue };
                let (f, c) = style(&r);
                let e = commands
                    .spawn((
                        Text2d::new(r.text.clone()),
                        f,
                        c,
                        Anchor::TOP_LEFT,
                        // 幅が分かるまでは大きさ 0
                        Transform::from_scale(Vec3::ZERO),
                        NoFrustumCulling,
                        SkinText {
                            run: Some(r),
                            z,
                            res,
                        },
                    ))
                    .id();
                sp.texts.push(e);
            }
        }
    }
}

/// システムフォントの文字列を LR2 と同じく置く: 縦 = h / 大きさ、横 = 縦 × min(1, w / 幅)、揃え 1 は幅の半分・2 は幅だけ左へ
fn place_text(mut q: Query<(&SkinText, &TextLayoutInfo, &mut Transform)>) {
    for (t, info, mut tf) in &mut q {
        let width = info.size.x;
        let Some(run) = t.run.as_ref().filter(|_| width > 0.0) else {
            tf.scale = Vec3::ZERO;
            continue;
        };
        let [x, y, w, h] = run.dst;
        let hl = h / run.font.size.max(1) as f32;
        let wl = if width > w { w / width } else { 1.0 } * hl;
        let x = match run.align {
            1 => x - (width * wl * 0.5).trunc(),
            2 => x - (width * wl).trunc(),
            _ => x,
        };
        *tf = Transform::from_xyz(x - t.res.0 as f32 / 2.0, t.res.1 as f32 / 2.0 - y, t.z)
            .with_scale(Vec3::new(wl, hl, 1.0));
    }
}

#[derive(Component)]
struct ResultText;

/// 場面が終わったら結果を出す（結果画面ができるまでの仮）
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
        for e in sp.texts.drain(..) {
            commands.entity(e).despawn();
        }
    }
    let play = game.session.play();
    let c = play.counts();
    commands.spawn((
        Text::new(format!(
            "RESULT: {:?}\nEX SCORE {}\nPG {}  GR {}  GD {}  BD {}  PR {}\nMAX COMBO {} / {}\n\nPress Esc to quit",
            game.session.record().lamp,
            c.ex_score(),
            c.pgreat,
            c.great,
            c.good,
            c.bad,
            c.poor,
            play.max_combo(),
            play.total_notes(),
        )),
        TextFont::from_font_size(28.0),
        Node {
            position_type: PositionType::Absolute,
            top: px(40),
            left: px(40),
            ..default()
        },
        ResultText,
    ));
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
            ));
        }
        commands.spawn((
            Sprite::from_color(color, Vec2::new(w - 4.0, NOTE_H)),
            Transform::from_xyz(x, head, 3.0),
            NoteSprite,
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
            "\n\nRESULT: {:?}\nPress Esc to quit",
            game.session.record().lamp
        );
    }
    text.0 = s;
}
