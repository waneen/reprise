//! 選曲画面。一覧の作り方とカーソルは `domain::select`。LR2 の選曲スキンがあればそれで描き（`draw_skin`）、
//! 無ければ文字の一覧で描く（`draw`）。
//! 操作: 皿（上・下）か ↑↓ で選び、鍵盤 1/3/5/7・Enter・→ で決定、
//! 鍵盤 6 でオートプレイ、鍵盤 2・← で戻る。最初の一覧で Esc なら終了。
//! スキンで描くときは LR2 と同じく、一覧が動くアニメーション・マウスのホイール・マウスでのバーの選択・BUTTON のクリックも受ける
//! （BUTTON 13 でキー設定画面へ）
use crate::input::SlotInput;
use crate::keyconfig_scene::KeyConfigScreen;
use crate::skin_render::{self, Pixels, SkinDraw, SkinGfx};
use crate::{list_font, PlayRequest, Scene, ScoreDb, SettingsRes};
use application::select_skin::{SelectBar, SelectSkin};
use bevy::input::mouse::MouseWheel;
use bevy::platform::time::Instant;
use bevy::prelude::*;
use domain::input::{key_slot, SCRATCH_DOWN, SCRATCH_UP};
use domain::play::RulesetId;
use domain::score::ClearLamp;
use domain::select::Bar;
use domain::select::{
    BarScroll, Decided, KeyState, Library, MouseButton as Lr2Mouse, ScrollRepeat, SelectState,
};
use domain::skin::lr2::{bar_slot_dst, button_clicks, draw_select, mouse_on, BarView, SelectView};
use domain::skin::Skin;

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
    /// 選曲スキン（無ければ文字の一覧）
    skin: Option<SkinSelect>,
}

/// 選曲スキンで描くための状態
struct SkinSelect {
    skin: Skin,
    /// 画像（最初に画面に入ったときに Bevy に渡す）
    pixels: Option<Vec<Option<Pixels>>>,
    gfx: SkinGfx,
    /// 画面に入るたびに作り直すもの
    scene: Option<SkinScene>,
}

/// 画面に入るたびに作り直すもの
struct SkinScene {
    state: SelectSkin,
    scroll: BarScroll,
    mouse: Lr2Mouse,
    start: Instant,
    /// 今の一覧（記録を引いたもの）と描画材料
    bars: Vec<SelectBar>,
    views: Vec<BarView>,
}

impl Selection {
    pub fn new(library: Library, skin: Option<(Skin, Vec<Option<Pixels>>)>) -> Self {
        let state = SelectState::new(&library);
        Self {
            library,
            state,
            repeat: ScrollRepeat::default(),
            start: Instant::now(),
            dirty: true,
            skin: skin.map(|(skin, pixels)| SkinSelect {
                skin,
                pixels: Some(pixels),
                gfx: SkinGfx::default(),
                scene: None,
            }),
        }
    }
    /// 選曲スキンで描くか
    pub fn has_skin(&self) -> bool {
        self.skin.is_some()
    }
}

#[derive(Component)]
pub struct SelectText;

pub fn enter(
    mut commands: Commands,
    mut sel: ResMut<Selection>,
    mut images: ResMut<Assets<Image>>,
    db: Res<ScoreDb>,
    settings: Res<SettingsRes>,
) {
    sel.dirty = true;
    let sel = &mut *sel;
    if let Some(sk) = sel.skin.as_mut() {
        commands.spawn((
            skin_render::camera(sk.skin.resolution),
            DespawnOnExit(Scene::Select),
        ));
        if let Some(pixels) = sk.pixels.take() {
            sk.gfx = SkinGfx::new(pixels, sk.skin.resolution, &mut images);
        }
        // 文字列の実体は前に画面を出たときに消えている
        sk.gfx.forget_texts();
        let bars = list_bars(&sel.library, &sel.state, &db);
        let views = bars.iter().map(SelectBar::view).collect::<Vec<_>>();
        let b = sk.skin.bars;
        let mut state = SelectSkin::new(sk.skin.timing, &sk.skin.options, settings.saved.options);
        state.select(bars.get(sel.state.cursor()));
        let scroll = BarScroll::new(b.center, b.available, sel.state.cursor(), bars.len(), 0);
        state.set_list(scroll.position(), bars.len());
        sk.scene = Some(SkinScene {
            state,
            scroll,
            mouse: Lr2Mouse::default(),
            start: Instant::now(),
            bars,
            views,
        });
        return;
    }
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
        "\n\n↑↓/皿: 選ぶ  Enter/→/鍵盤 1・3・5・7: 決定  鍵盤 6: オート  ←/鍵盤 2: 戻る  Esc: 終了";
    text.0 = s;
}

/// 一覧のバー（LR2 のルールの記録を引く）
fn list_bars(library: &Library, state: &SelectState, db: &ScoreDb) -> Vec<SelectBar> {
    let record = |h: &domain::chart::ChartHash| db.find(h, RulesetId::Lr2);
    state
        .bars()
        .iter()
        .map(|b| SelectBar::new(library, b, &record))
        .collect()
}

/// マウスの位置（スキンの座標）
pub(crate) fn mouse_pos(
    windows: &Query<&Window>,
    cams: &Query<(&Camera, &GlobalTransform)>,
    res: (u32, u32),
) -> Option<(i32, i32)> {
    let cursor = windows.iter().next()?.cursor_position()?;
    let (cam, gt) = cams.iter().next()?;
    let w = cam.viewport_to_world_2d(gt, cursor).ok()?;
    Some((
        (w.x + res.0 as f32 / 2.0).floor() as i32,
        (res.1 as f32 / 2.0 - w.y).floor() as i32,
    ))
}

/// 選曲スキンのときの入力（LR2 `ProcI_Select` の順: ホイール → キー → 決定・戻る・マウス）と一覧の動き
#[allow(clippy::too_many_arguments)]
pub fn skin_input(
    mut commands: Commands,
    keys: Res<ButtonInput<KeyCode>>,
    buttons: Res<ButtonInput<MouseButton>>,
    mut wheel: MessageReader<MouseWheel>,
    slots: Res<SlotInput>,
    windows: Query<&Window>,
    cams: Query<(&Camera, &GlobalTransform)>,
    db: Res<ScoreDb>,
    mut sel: ResMut<Selection>,
    keyconfig: Option<Res<KeyConfigScreen>>,
    mut next: ResMut<NextState<Scene>>,
    mut exit: MessageWriter<AppExit>,
) {
    let wheel: i32 = wheel.read().map(|w| w.y.signum() as i32).sum();
    let sel = &mut *sel;
    let Some(sk) = sel.skin.as_mut() else {
        return;
    };
    let Some(sc) = sk.scene.as_mut() else {
        return;
    };
    let now = sc.start.elapsed().as_millis() as i64;
    let res = sk.skin.resolution;
    let mouse = sc.mouse.update(now, buttons.pressed(MouseButton::Left));
    let pos = mouse_pos(&windows, &cams, res);
    let (cursor0, slot0) = (sel.state.cursor(), sc.scroll.slot());
    // `#STARTINPUT` までは入力を受けない
    if now > sk.skin.timing.start_input as i64 {
        sc.scroll.wheel(now, wheel);
        let up = key_state(&slots, SCRATCH_UP, &keys, KeyCode::ArrowUp);
        let down = key_state(&slots, SCRATCH_DOWN, &keys, KeyCode::ArrowDown);
        sc.scroll.keys(now, up, down);
        // マウス: 選んでいるバーを押したら決定、それ以外のバーは押し続けると選ぶ（DOM-SEL-10）
        let on_selected = pos.is_some_and(|(x, y)| {
            bar_slot_dst(&sk.skin, &sc.state, sc.scroll.slot(), true)
                .is_some_and(|d| mouse_on(d, res, x, y) != 0)
        });
        let mut decide = [1, 3, 5, 7].iter().any(|&n| slots.pressed[key_slot(n)])
            || keys.any_just_pressed([KeyCode::Enter, KeyCode::ArrowRight])
            || (mouse == 1 && on_selected);
        let mut auto = slots.pressed[key_slot(6)];
        if mouse == 2 {
            if let Some((x, y)) = pos {
                for i in 0..30 {
                    let hit = bar_slot_dst(&sk.skin, &sc.state, i, false)
                        .is_some_and(|d| mouse_on(d, res, x, y) != 0);
                    if hit {
                        sc.scroll.mouse_hold(now, i);
                    }
                }
            }
        }
        // BUTTON（パネルは開いていない = −1）
        if mouse == 1 {
            if let Some((x, y)) = pos {
                for c in button_clicks(&sk.skin, &sc.state, x, y, -1) {
                    match c.action {
                        // 13 キー設定（LR2 `is_clicked_keyconfig`）。キー設定スキンが無ければ何もしない（LR2 にも代わりの画面は無い）
                        13 if keyconfig.is_some() => {
                            next.set(Scene::KeyConfig);
                            return;
                        }
                        // 14 スキン選択: 画面がまだ無いのでクリックを受けても何もしない
                        13 | 14 => {}
                        // 15 プレイ・16 オートプレイ（曲を選んでいるときだけ）
                        15 | 16 if matches!(sel.state.current(), Some(Bar::Song(_))) => {
                            decide = true;
                            auto |= c.action == 16;
                        }
                        // ほか（パネル・絞り込み・並べ替え・FX など）は対応する機能が無いので何もしない
                        _ => {}
                    }
                }
            }
        }
        let mut reload = false;
        if decide || auto {
            match sel.state.decide(&sel.library) {
                Decided::Song(i) => {
                    commands.insert_resource(PlayRequest {
                        path: sel.library.songs()[i].path.clone(),
                        auto,
                    });
                    next.set(Scene::Play);
                }
                Decided::Opened => reload = true,
                Decided::Nothing => {}
            }
        }
        if (slots.pressed[key_slot(2)]
            || keys.any_just_pressed([KeyCode::ArrowLeft, KeyCode::Backspace]))
            && sel.state.back()
        {
            reload = true;
        }
        if keys.just_pressed(KeyCode::Escape) {
            if sel.state.back() {
                reload = true;
            } else {
                exit.write(AppExit::Success);
            }
        }
        if reload {
            // フォルダを開いた・戻った: 一覧を作り直し、カーソルの位置で止める（DOM-SEL-09）
            sc.bars = list_bars(&sel.library, &sel.state, &db);
            sc.views = sc.bars.iter().map(SelectBar::view).collect();
            sc.scroll.jump(sel.state.cursor(), sc.bars.len(), now);
            sc.state.reset_bar_timer();
            sc.state.select(sc.bars.get(sel.state.cursor()));
        }
    }
    // 一覧の動き（DOM-SEL-07, 08）
    let n = sc.bars.len();
    let moved = sc.scroll.update(now, n);
    sel.state.set_cursor(sc.scroll.cursor(n));
    let changed = sel.state.cursor() != cursor0 || sc.scroll.slot() != slot0;
    sc.state.update(now, moved, changed);
    if changed {
        sc.state.select(sc.bars.get(sel.state.cursor()));
    }
    sc.state.set_list(sc.scroll.position(), n);
}

/// 選曲スキンで描く（毎フレーム作り直す）
pub fn draw_skin(mut p: SkinDraw, mut sel: ResMut<Selection>) {
    let Some(sk) = sel.skin.as_mut() else {
        return;
    };
    let Some(sc) = sk.scene.as_ref() else {
        return;
    };
    let view = SelectView {
        position: sc.scroll.position(),
        slot: sc.scroll.slot(),
        bars: &sc.views,
    };
    let gfx = &sk.gfx;
    let draws = draw_select(&sk.skin, &sc.state, &view, &|i| gfx.size(i));
    skin_render::render(&mut p, &mut sk.gfx, draws, Scene::Select);
}
