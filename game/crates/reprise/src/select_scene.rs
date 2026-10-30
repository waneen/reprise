//! 選曲画面（スキン無し。文字の一覧）。一覧の作り方とカーソルは `domain::select`。
//! 操作: 皿（上・下）か ↑↓ で選び、鍵盤 1/3/5/7・Enter・→ で決定、
//! 鍵盤 6 でオートプレイ、鍵盤 2・← で戻る。F1 でキー設定。最初の一覧で Esc なら終了
use crate::input::SlotInput;
use crate::{list_font, PlayRequest, Scene, ScoreDb};
use bevy::platform::time::Instant;
use bevy::prelude::*;
use domain::input::{key_slot, SCRATCH_DOWN, SCRATCH_UP};
use domain::play::RulesetId;
use domain::score::ClearLamp;
use domain::select::{Bar, Decided, KeyState, Library, ScrollRepeat, SelectState};

/// 一覧に出す行数（カーソルの上下にこの半分ずつ）
const ROWS: usize = 15;

#[derive(Resource)]
pub struct Selection {
    pub library: Library,
    pub state: SelectState,
    repeat: ScrollRepeat,
    start: Instant,
    /// 表示を作り直すか
    dirty: bool,
}

impl Selection {
    pub fn new(library: Library) -> Self {
        let state = SelectState::new(&library);
        Self {
            library,
            state,
            repeat: ScrollRepeat::default(),
            start: Instant::now(),
            dirty: true,
        }
    }
}

#[derive(Component)]
pub struct SelectText;

pub fn enter(mut commands: Commands, mut sel: ResMut<Selection>) {
    sel.dirty = true;
    commands.spawn((Camera2d, DespawnOnExit(Scene::Select)));
    commands.spawn((
        Text::new(""),
        TextFont {
            font: list_font(),
            font_size: bevy::text::FontSize::Px(20.0),
            ..default()
        },
        Node {
            position_type: PositionType::Absolute,
            top: px(24),
            left: px(40),
            ..default()
        },
        SelectText,
        DespawnOnExit(Scene::Select),
    ));
}

/// 操作かキーボードのキーの状態
fn key_state(
    slots: &SlotInput,
    slot: usize,
    keys: &ButtonInput<KeyCode>,
    code: KeyCode,
) -> KeyState {
    if slots.pressed[slot] || keys.just_pressed(code) {
        KeyState::Pressed
    } else if slots.held[slot] || keys.pressed(code) {
        KeyState::Held
    } else {
        KeyState::Off
    }
}

pub fn input(
    mut commands: Commands,
    keys: Res<ButtonInput<KeyCode>>,
    slots: Res<SlotInput>,
    mut sel: ResMut<Selection>,
    mut next: ResMut<NextState<Scene>>,
    mut exit: MessageWriter<AppExit>,
) {
    let sel = &mut *sel;
    let now = sel.start.elapsed().as_millis() as i64;
    let up = key_state(&slots, SCRATCH_UP, &keys, KeyCode::ArrowUp);
    let down = key_state(&slots, SCRATCH_DOWN, &keys, KeyCode::ArrowDown);
    let d = sel.repeat.update(now, up, down);
    if d != 0 {
        sel.state.scroll(d);
        sel.dirty = true;
    }
    let decide = [1, 3, 5, 7].iter().any(|&n| slots.pressed[key_slot(n)])
        || keys.any_just_pressed([KeyCode::Enter, KeyCode::ArrowRight]);
    let auto = slots.pressed[key_slot(6)];
    if decide || auto {
        match sel.state.decide(&sel.library) {
            Decided::Song(i) => {
                commands.insert_resource(PlayRequest {
                    path: sel.library.songs()[i].path.clone(),
                    auto,
                });
                next.set(Scene::Play);
            }
            Decided::Opened => sel.dirty = true,
            Decided::Nothing => {}
        }
    }
    if (slots.pressed[key_slot(2)]
        || keys.any_just_pressed([KeyCode::ArrowLeft, KeyCode::Backspace]))
        && sel.state.back()
    {
        sel.dirty = true;
    }
    if keys.just_pressed(KeyCode::F1) {
        next.set(Scene::KeyConfig);
    }
    if keys.just_pressed(KeyCode::Escape) && !sel.state.back() {
        exit.write(AppExit::Success);
    } else if keys.just_pressed(KeyCode::Escape) {
        sel.dirty = true;
    }
}

fn lamp_text(l: ClearLamp) -> &'static str {
    match l {
        ClearLamp::NoPlay => "     ",
        ClearLamp::Failed => "FAIL ",
        ClearLamp::AssistEasy | ClearLamp::LightAssistEasy => "ASST ",
        ClearLamp::Easy => "EASY ",
        ClearLamp::Normal => "CLEAR",
        ClearLamp::Hard => "HARD ",
        ClearLamp::ExHard => "EXH  ",
        ClearLamp::FullCombo | ClearLamp::Perfect | ClearLamp::Max => "FC   ",
    }
}

pub fn draw(
    mut sel: ResMut<Selection>,
    db: Res<ScoreDb>,
    mut text: Query<&mut Text, With<SelectText>>,
) {
    if !sel.dirty {
        return;
    }
    let Ok(mut text) = text.single_mut() else {
        return;
    };
    sel.dirty = false;
    let bars = sel.state.bars();
    let mut s = String::new();
    if bars.is_empty() {
        s += "曲がありません（--songs で譜面のフォルダを指定してください）\n";
    }
    let n = bars.len();
    let shown = n.min(ROWS);
    for k in 0..shown {
        // カーソルを真ん中に置き、端は回り込む
        let i = (sel.state.cursor() + n + k - shown / 2) % n;
        let mark = if i == sel.state.cursor() { ">" } else { " " };
        let line = match &bars[i] {
            Bar::Folder { title, .. } => format!("      [{title}]"),
            Bar::Song(j) => {
                let song = &sel.library.songs()[*j];
                let lamp = db
                    .find(&song.hash, RulesetId::Lr2)
                    .map_or(ClearLamp::NoPlay, |r| r.lamp);
                let title = if song.subtitle.is_empty() {
                    song.title.clone()
                } else {
                    format!("{} {}", song.title, song.subtitle)
                };
                format!("{} {:>2}  {title}", lamp_text(lamp), song.level)
            }
        };
        s += &format!("{mark} {line}\n");
    }
    if let Some(Bar::Song(j)) = sel.state.current() {
        let song = &sel.library.songs()[*j];
        s += &format!("\n{} / {}", song.artist, song.genre);
        if let Some(r) = db.find(&song.hash, RulesetId::Lr2) {
            s += &format!(
                "\nBEST {:?}  EX {}  MISS {}  PLAY {}",
                r.lamp,
                r.ex_score,
                r.min_bp.map_or("-".into(), |b| b.to_string()),
                r.play_count
            );
        }
    }
    s +=
        "\n\n↑↓/皿: 選ぶ  Enter/→/鍵盤 1・3・5・7: 決定  鍵盤 6: オート  ←/鍵盤 2: 戻る  F1: キー設定  Esc: 終了";
    text.0 = s;
}
