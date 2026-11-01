//! 選曲画面。一覧の作り方とカーソルは `domain::select`。LR2 の選曲スキンがあればそれで描き（`draw_skin`）、
//! 無ければ文字の一覧で描く（`draw`）。
//! 操作: 皿（上・下）か ↑↓ で選び、鍵盤 1/3/5/7・Enter で決定（→ はフォルダを開く）、
//! 鍵盤 6 でオートプレイ、鍵盤 2・← で戻る。最初の一覧で Esc なら終了。曲を選んでいるときの SELECT・→ は難易度の絞り込みを替え、
//! START を押している間はパネル 1 が開いて鍵盤 1 で鍵盤数の絞り込みを替える（LR2 と同じ）。皿・鍵盤は LR2 と同じく 1P・2P のどちらでもよい。
//! スキンで描くときは LR2 と同じく、一覧が動くアニメーション・マウスのホイール・マウスでのバーの選択・BUTTON のクリックも受ける
//! （BUTTON 10..12 で絞り込み・並べ替え、13 でキー設定画面へ）。選んでいる曲のプレビュー音は `preview`
use crate::input::SlotInput;
use crate::keyconfig_scene::KeyConfigScreen;
use crate::preview::{self, PreviewPlayer, PreviewSound};
use crate::skin_render::{self, Pixels, SkinDraw, SkinGfx};
use crate::{list_font, save_settings, Boot, PlayRequest, Scene, ScoreDb, SettingsRes};
use application::select_skin::{lr2_clear, OptionNames, SelectBar, SelectSkin};
use bevy::input::mouse::MouseWheel;
use bevy::platform::time::Instant;
use bevy::prelude::*;
use domain::chart::ChartHash;
use domain::input::{key_slot, SCRATCH_DOWN, SCRATCH_UP, SELECT, START};
use domain::play::RulesetId;
use domain::score::ClearLamp;
use domain::select::Bar;
use domain::select::{
    BarScroll, Decided, FilterKind, KeyState, Library, ListFilter, MouseButton as Lr2Mouse, Panel,
    PanelChange, ScrollRepeat, SelectState, SortRecord,
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
    /// 並べ替え・絞り込み（`config.json` に保存する）
    pub filter: ListFilter,
    /// オプションの名前（LR2 本体の `LR2files/Config/optionstr.csv`）
    names: OptionNames,
    panel: Panel,
    /// 文字の一覧のとき、選んでいるバーを替えた時刻（スキンのタイマー 11 の代わり。プレビュー音に使う）
    text_bar_ms: i64,
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
    pub fn new(
        library: Library,
        skin: Option<(Skin, Vec<Option<Pixels>>)>,
        filter: ListFilter,
        names: OptionNames,
    ) -> Self {
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
            filter,
            names,
            panel: Panel::default(),
            text_bar_ms: 0,
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
    sel.panel = Panel::default();
    sel.start = Instant::now();
    sel.text_bar_ms = 0;
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
        state.set_filter(sel.filter, &sel.names);
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

/// 1P か 2P の操作かキーボードのキーの状態（LR2 の選曲は 1P と 2P の操作のどちらも受ける）
fn key_state(
    slots: &SlotInput,
    slot: usize,
    keys: &ButtonInput<KeyCode>,
    code: KeyCode,
) -> KeyState {
    if slots.pressed_any(slot) || keys.just_pressed(code) {
        KeyState::Pressed
    } else if slots.held_any(slot) || keys.pressed(code) {
        KeyState::Held
    } else {
        KeyState::Off
    }
}

/// 並べ替えに使う記録（LR2 のルールの記録のランプと EX スコア）
fn sort_record(db: &ScoreDb) -> impl Fn(&ChartHash) -> SortRecord + '_ {
    |h| {
        db.find(h, RulesetId::Lr2)
            .map(|r| SortRecord {
                clear: lr2_clear(r.lamp),
                ex_score: r.ex_score,
            })
            .unwrap_or_default()
    }
}

/// 鍵盤と START・SELECT の操作（文字の一覧と選曲スキンで共通。LR2 `ProcI_Select`）
#[derive(Default)]
struct Keys {
    /// 決定（鍵盤 1/3/5/7・Enter、フォルダなら →）
    decide: bool,
    /// オートプレイ（鍵盤 6）
    auto: bool,
    /// 一覧を作り直した（絞り込みを替えた）
    reload: bool,
    /// パネル 1 が開いて START を押し続けているので、ほかの入力は受けない
    blocked: bool,
    /// パネル 1 が開いた・閉じた
    panel: Option<PanelChange>,
}

fn keys_of(
    sel: &mut Selection,
    slots: &SlotInput,
    keys: &ButtonInput<KeyCode>,
    db: &ScoreDb,
) -> Keys {
    let mut k = Keys::default();
    let rec = sort_record(db);
    // パネル 1（START か Space で開き、離すと閉じる）
    let start_pressed = slots.pressed_any(START) || keys.just_pressed(KeyCode::Space);
    let start_released = slots.released_any(START) || keys.just_released(KeyCode::Space);
    k.panel = sel.panel.update(start_pressed, start_released);
    if sel.panel.is_open() {
        // パネル 1 の鍵盤 1: 鍵盤数の絞り込み（LR2 `SetPlayOption`）
        if slots.pressed_any(key_slot(1)) {
            sel.filter.next(FilterKind::Key);
            sel.state
                .refilter(&sel.library, &mut sel.filter, &rec, true);
            k.reload = true;
        }
        if !start_pressed && (slots.held_any(START) || keys.pressed(KeyCode::Space)) {
            k.blocked = true;
            return k;
        }
    }
    let on_song = matches!(sel.state.current(), Some(Bar::Song(_)));
    let right = keys.just_pressed(KeyCode::ArrowRight);
    k.auto = slots.pressed_any(key_slot(6));
    if !k.auto && !sel.panel.is_open() && on_song && (right || slots.pressed_any(SELECT)) {
        // 曲の → と SELECT: 難易度の絞り込みを替える（DOM-SEL-16）
        sel.state
            .change_difficulty(&sel.library, &mut sel.filter, &rec);
        k.reload = true;
        return k;
    }
    k.decide = [1, 3, 5, 7].iter().any(|&n| slots.pressed_any(key_slot(n)))
        || keys.just_pressed(KeyCode::Enter)
        || (right && !on_song);
    k
}

/// 決定した（曲ならプレイへ、フォルダなら開く）。一覧を作り直したら true
fn decide(
    commands: &mut Commands,
    sel: &mut Selection,
    db: &ScoreDb,
    auto: bool,
    next: &mut NextState<Scene>,
) -> bool {
    let rec = sort_record(db);
    match sel.state.decide(&sel.library, &mut sel.filter, &rec) {
        Decided::Song(i) => {
            commands.insert_resource(PlayRequest {
                path: sel.library.songs()[i].path.clone(),
                auto,
                seed: None,
            });
            next.set(Scene::Play);
            false
        }
        Decided::Opened => true,
        Decided::Nothing => false,
    }
}

/// 絞り込みが替わったら `config.json` に保存する（LR2 も `config.select` に持って保存する）
fn save_filter(sel: &Selection, settings: &mut SettingsRes) {
    if settings.saved.select != sel.filter {
        settings.saved.select = sel.filter;
        save_settings(settings);
    }
}

/// 選んでいる曲（添字・MD5・パス）
fn current_song(sel: &Selection) -> Option<(usize, ChartHash, std::path::PathBuf)> {
    match sel.state.current() {
        Some(Bar::Song(i)) => {
            let s = &sel.library.songs()[*i];
            Some((*i, s.hash, s.path.clone()))
        }
        _ => None,
    }
}

#[allow(clippy::too_many_arguments)]
pub fn input(
    mut commands: Commands,
    keys: Res<ButtonInput<KeyCode>>,
    slots: Res<SlotInput>,
    db: Res<ScoreDb>,
    mut settings: ResMut<SettingsRes>,
    mut sel: ResMut<Selection>,
    mut next: ResMut<NextState<Scene>>,
    mut exit: MessageWriter<AppExit>,
) {
    let sel = &mut *sel;
    let now = sel.start.elapsed().as_millis() as i64;
    let before = (sel.state.cursor(), sel.state.bars().to_vec());
    let k = keys_of(sel, &slots, &keys, &db);
    if !k.blocked {
        let up = key_state(&slots, SCRATCH_UP, &keys, KeyCode::ArrowUp);
        let down = key_state(&slots, SCRATCH_DOWN, &keys, KeyCode::ArrowDown);
        let d = sel.repeat.update(now, up, down);
        if d != 0 {
            sel.state.scroll(d);
        }
        if k.decide || k.auto {
            decide(&mut commands, sel, &db, k.auto, &mut next);
        }
        if slots.pressed_any(key_slot(2))
            || keys.any_just_pressed([KeyCode::ArrowLeft, KeyCode::Backspace])
        {
            sel.state.back();
        }
        if keys.just_pressed(KeyCode::Escape) && !sel.state.back() {
            exit.write(AppExit::Success);
        }
    }
    if k.reload || (sel.state.cursor(), sel.state.bars()) != (before.0, &before.1[..]) {
        sel.text_bar_ms = now;
        sel.dirty = true;
    }
    if settings.saved.select != sel.filter {
        sel.dirty = true;
    }
    save_filter(sel, &mut settings);
}

/// 選曲のプレビュー音（文字の一覧・選曲スキンのどちらでも。タイマー 11 から 500ms）
#[allow(clippy::too_many_arguments)]
pub fn preview(
    mut commands: Commands,
    sel: Res<Selection>,
    boot: Res<Boot>,
    settings: Res<SettingsRes>,
    mut player: ResMut<PreviewPlayer>,
    mut audio: ResMut<Assets<AudioSource>>,
    sounds: Query<Entity, With<PreviewSound>>,
) {
    use domain::skin::lr2::SkinState;
    let bar_timer = match &sel.skin {
        Some(sk) => sk.scene.as_ref().and_then(|sc| sc.state.timer(11)),
        None => Some(sel.start.elapsed().as_millis() as i64 - sel.text_bar_ms),
    };
    preview::update(
        &mut commands,
        &mut player,
        &boot,
        &settings,
        &mut audio,
        &sounds,
        bar_timer,
        current_song(&sel),
    );
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
    let f = sel.filter;
    s += &format!(
        "\n\n並べ替え {}  鍵盤数 {}  難易度 {}",
        f.sort, f.key, f.difficulty
    );
    s += "\n↑↓/皿: 選ぶ  Enter/鍵盤 1・3・5・7: 決定（→ はフォルダを開く）  鍵盤 6: オート  ←/鍵盤 2: 戻る  Esc: 終了\n\
          SELECT/→: 難易度の絞り込み  START を押しながら鍵盤 1: 鍵盤数の絞り込み";
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

/// 選曲スキンのときの入力（LR2 `ProcI_Select` の順: パネル → ホイール → キー → 決定・戻る・マウス）と一覧の動き
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
    mut settings: ResMut<SettingsRes>,
    mut sel: ResMut<Selection>,
    keyconfig: Option<Res<KeyConfigScreen>>,
    mut next: ResMut<NextState<Scene>>,
    mut exit: MessageWriter<AppExit>,
) {
    let wheel: i32 = wheel.read().map(|w| w.y.signum() as i32).sum();
    let sel = &mut *sel;
    let Some(sk) = sel.skin.as_ref() else {
        return;
    };
    let Some(sc) = sk.scene.as_ref() else {
        return;
    };
    let now = sc.start.elapsed().as_millis() as i64;
    let ready = now > sk.skin.timing.start_input as i64;
    let (cursor0, slot0) = (sel.state.cursor(), sc.scroll.slot());
    let mut reload = false;
    // `#STARTINPUT` までは入力を受けない
    if ready {
        let k = keys_of(sel, &slots, &keys, &db);
        let sk = sel.skin.as_mut().expect("スキン");
        let sc = sk.scene.as_mut().expect("場面");
        if let Some(c) = k.panel {
            sc.state.set_panel(c == PanelChange::Opened);
        }
        reload |= k.reload;
        let res = sk.skin.resolution;
        let mouse = sc.mouse.update(now, buttons.pressed(MouseButton::Left));
        let pos = mouse_pos(&windows, &cams, res);
        let panel = if sel.panel.is_open() { 1 } else { -1 };
        let mut decide_now = k.decide;
        let mut auto = k.auto;
        // BUTTON（LR2 `SetObjectValue_Button`。パネルは開いていれば 1、閉じていれば −1）
        let mut clicked: Option<FilterKind> = None;
        if mouse == 1 {
            if let Some((x, y)) = pos {
                for c in button_clicks(&sk.skin, &sc.state, x, y, panel) {
                    match c.action {
                        // 10 難易度・11 鍵盤数・12 並べ替え（1 つ先へ。DOM-SEL-13）
                        10 => clicked = Some(FilterKind::Difficulty),
                        11 => clicked = Some(FilterKind::Key),
                        12 => clicked = Some(FilterKind::Sort),
                        // 13 キー設定（LR2 `is_clicked_keyconfig`）。キー設定スキンが無ければ何もしない（LR2 にも代わりの画面は無い）
                        13 if keyconfig.is_some() => {
                            next.set(Scene::KeyConfig);
                            return;
                        }
                        // 14 スキン選択: 画面がまだ無いのでクリックを受けても何もしない
                        13 | 14 => {}
                        // 15 プレイ・16 オートプレイ（曲を選んでいるときだけ）
                        15 | 16 if matches!(sel.state.current(), Some(Bar::Song(_))) => {
                            decide_now = true;
                            auto |= c.action == 16;
                        }
                        // ほか（パネルの中身・FX など）は対応する機能が無いので何もしない
                        _ => {}
                    }
                }
            }
        }
        if !k.blocked {
            sc.scroll.wheel(now, wheel);
            let up = key_state(&slots, SCRATCH_UP, &keys, KeyCode::ArrowUp);
            let down = key_state(&slots, SCRATCH_DOWN, &keys, KeyCode::ArrowDown);
            sc.scroll.keys(now, up, down);
            // マウス: 選んでいるバーを押したら決定、それ以外のバーは押し続けると選ぶ（DOM-SEL-10）
            let on_selected = pos.is_some_and(|(x, y)| {
                bar_slot_dst(&sk.skin, &sc.state, sc.scroll.slot(), true)
                    .is_some_and(|d| mouse_on(d, res, x, y) != 0)
            });
            decide_now |= mouse == 1 && on_selected;
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
        }
        if let Some(kind) = clicked {
            sel.filter.next(kind);
            let rec = sort_record(&db);
            sel.state
                .refilter(&sel.library, &mut sel.filter, &rec, kind == FilterKind::Key);
            reload = true;
        }
        if !k.blocked {
            if (decide_now || auto) && decide(&mut commands, sel, &db, auto, &mut next) {
                reload = true;
            }
            if (slots.pressed_any(key_slot(2))
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
        }
    }
    let sk = sel.skin.as_mut().expect("スキン");
    let sc = sk.scene.as_mut().expect("場面");
    sc.state.set_filter(sel.filter, &sel.names);
    if reload {
        // フォルダを開いた・戻った・絞り込みを替えた: 一覧を作り直し、カーソルの位置で止める（DOM-SEL-09, 20）
        sc.bars = list_bars(&sel.library, &sel.state, &db);
        sc.views = sc.bars.iter().map(SelectBar::view).collect();
        sc.scroll.jump(sel.state.cursor(), sc.bars.len(), now);
        sc.state.reset_bar_timer();
        sc.state.select(sc.bars.get(sel.state.cursor()));
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
    save_filter(sel, &mut settings);
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
