//! Composition Root + プレゼンテーション層（Bevy はここだけ）。
//! 記法を増やす = chart-formats/ か skin-formats/ に crate を足して、ここで 1 行登録するだけ。
//!
//! 使い方: `reprise <譜面ファイル> [--gauge groove|easy|hard|death|pattack|gattack]`
//! いまは 1P の 7 鍵（5 鍵）だけを四角形で描く最小のプレイ画面。スキンは未対応。
use application::play::{KeyId, KeyInput, PlaySession, VisibleNote};
use application::ports::{AudioOutput, ChartLoader, ScoreImporter, SkinLoader};
use bevy::audio::AudioSource;
use bevy::input::keyboard::KeyboardInput;
use bevy::input::ButtonState;
use bevy::prelude::*;
use bevy::window::{PresentMode, WindowResolution};
use chart_loader::DecoderLoader;
use domain::chart::{Chart, Lane, SoundId};
use domain::play::{GaugeType, Judge, PlayEvent};
use skin_loader::DecoderSkinLoader;
use std::collections::HashMap;
use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

/// 再生開始から 0ms（譜面の先頭）までの待ち時間
const LEAD_IN_MS: i64 = 2000;
/// 判定ラインから上端までに見える拍数
const VISIBLE_BEATS: f64 = 4.0;

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

/// スキンの読み込み（まだ画面からは使わない。登録だけしておく）
fn skin_loaders(root: PathBuf) -> Vec<Box<dyn SkinLoader>> {
    vec![
        Box::new(DecoderSkinLoader {
            decoder: reprise_skin_lr2csv::Lr2CsvDecoder::default(),
            settings: Default::default(),
            root: root.clone(),
        }),
        Box::new(DecoderSkinLoader {
            decoder: reprise_skin_beatoraja::BeatorajaJsonDecoder,
            settings: Default::default(),
            root,
        }),
    ]
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

/// 譜面と音を読み込む
fn load(path: &Path) -> Result<(Chart, Sounds), String> {
    let bytes = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let seed = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u32)
        .unwrap_or(0);
    let (chart, _) = application::usecase::load_chart(&chart_loaders(), path, &bytes, seed)
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
    let mut it = args.iter();
    while let Some(a) = it.next() {
        if a == "--gauge" {
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
            "使い方: reprise <譜面ファイル> [--gauge groove|easy|hard|death|pattack|gattack]"
        );
        std::process::exit(2);
    };
    let _ = (
        skin_loaders(std::env::current_dir().unwrap_or_default()),
        score_importers(),
    );
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
        .insert_resource(Game {
            session,
            start: None,
            last_judge: None,
            sounds: HashMap::new(),
        })
        .insert_resource(PendingSounds(sounds))
        .add_systems(Startup, setup)
        .add_systems(Update, (play_frame, draw_notes, draw_hud).chain())
        .run();
}

#[derive(Resource)]
struct Game {
    session: PlaySession,
    /// 最初のフレームで決める（起動にかかった時間で先頭を取りこぼさないように）
    start: Option<Instant>,
    /// 最後の判定と、そのフレーム時刻
    last_judge: Option<(Judge, i64)>,
    sounds: HashMap<SoundId, Handle<AudioSource>>,
}

impl Game {
    fn now_ms(&self) -> i64 {
        self.start.map_or(0, |s| s.elapsed().as_millis() as i64) - LEAD_IN_MS
    }
}

#[derive(Resource)]
struct PendingSounds(Sounds);

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
    mut audio: ResMut<Assets<AudioSource>>,
    mut game: ResMut<Game>,
) {
    commands.spawn(Camera2d);
    for (id, bytes) in pending.0.drain(..) {
        let h = audio.add(AudioSource { bytes });
        game.sounds.insert(id, h);
    }
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
            events.push((i as KeyId, k.state == ButtonState::Pressed));
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
    for e in out {
        if let PlayEvent::Judge { judge, .. } = e {
            game.last_judge = Some((judge, t));
        }
    }
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
