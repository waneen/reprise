//! リザルト画面。LR2 と同じく、プレイの場面を閉じたあと（オートプレイ・PG/GR/GD の無いプレイは除く）にリザルトスキン
//! （`#INFORMATION` 種別 7）で描く。鍵盤 1〜7・Esc・Enter・マウスの左右の押し続けで、ランクの表示 → 記録更新の表示 →
//! 閉じ始め（`#FADEOUT`）と進み、閉じるときに白鍵と黒鍵を押し続けていれば同じ譜面をもう一度、それ以外は選曲に戻る。
//! 規則は `application::result_skin`・`domain::skin::lr2::draw_result`。リザルトスキンが無ければこの画面は使わず、結果を文字で出す
use crate::input::SlotInput;
use crate::skin_render::{self, Pixels, SkinDraw, SkinGfx};
use crate::{Boot, PlayRequest, Scene};
use application::result_skin::{PlayResult, ResultExit, ResultInput, ResultSkin};
use bevy::platform::time::Instant;
use bevy::prelude::*;
use domain::select::MouseButton as Lr2Mouse;
use domain::skin::lr2::{draw_result, gauge_chart_times, ResultView};
use domain::skin::Skin;
use std::path::PathBuf;

/// リザルトスキンとその画面の状態（スキンが読めたときだけある）
#[derive(Resource)]
pub struct ResultScreen {
    skin: Skin,
    /// 画像（最初に画面に入ったときに Bevy に渡す）
    pixels: Option<Vec<Option<Pixels>>>,
    gfx: SkinGfx,
    /// 画面に入るたびに作り直すもの
    scene: Option<ResultScene>,
}

struct ResultScene {
    state: ResultSkin,
    mouse: Lr2Mouse,
    /// LR2 のマウスの右ボタンの状態
    right: u8,
    start: Instant,
    /// もう一度遊ぶときの譜面と乱数の種
    retry: (PathBuf, u32),
}

impl ResultScreen {
    pub fn new(skin: Skin, pixels: Vec<Option<Pixels>>) -> Self {
        Self {
            skin,
            pixels: Some(pixels),
            gfx: SkinGfx::default(),
            scene: None,
        }
    }
}

/// プレイの場面から渡す結果（遊んだ譜面と乱数の種も）
#[derive(Resource)]
pub struct PendingResult {
    pub result: PlayResult,
    pub path: PathBuf,
    pub seed: u32,
}

pub fn enter(
    mut commands: Commands,
    mut rs: ResMut<ResultScreen>,
    mut images: ResMut<Assets<Image>>,
    pending: Option<Res<PendingResult>>,
    buttons: Res<ButtonInput<MouseButton>>,
    mut next: ResMut<NextState<Scene>>,
) {
    let Some(pending) = pending else {
        next.set(Scene::Select);
        return;
    };
    commands.remove_resource::<PendingResult>();
    let rs = &mut *rs;
    commands.spawn((
        skin_render::camera(rs.skin.resolution),
        DespawnOnExit(Scene::Result),
    ));
    if let Some(pixels) = rs.pixels.take() {
        rs.gfx = SkinGfx::new(pixels, rs.skin.resolution, &mut images);
    }
    // 文字列の実体は前に画面を出たときに消えている
    rs.gfx.forget_texts();
    // LR2 のマウスの状態は場面をまたいで続く
    let mut mouse = Lr2Mouse::default();
    mouse.update(0, buttons.pressed(MouseButton::Left));
    let state = ResultSkin::new(
        rs.skin.timing,
        &rs.skin.options,
        gauge_chart_times(&rs.skin),
        pending.result.clone(),
    );
    rs.scene = Some(ResultScene {
        state,
        mouse,
        right: if buttons.pressed(MouseButton::Right) {
            1
        } else {
            0
        },
        start: Instant::now(),
        retry: (pending.path.clone(), pending.seed),
    });
}

/// LR2 のマウスの右ボタンの状態（`En_input.cpp`: 0 なし・1 押した・2 押し続け・3 離した）
pub(crate) fn right_state(prev: u8, down: bool) -> u8 {
    match (down, prev) {
        (false, 0 | 3) => 0,
        (false, _) => 3,
        (true, 1 | 2) => 2,
        (true, _) => 1,
    }
}

/// 入力（LR2 `ProcI_Result`）。閉じたら選曲（譜面を直接渡して起動したなら終了）か、同じ譜面をもう一度
#[allow(clippy::too_many_arguments)]
pub fn input(
    mut commands: Commands,
    keys: Res<ButtonInput<KeyCode>>,
    buttons: Res<ButtonInput<MouseButton>>,
    slots: Res<SlotInput>,
    boot: Res<Boot>,
    mut rs: ResMut<ResultScreen>,
    mut next: ResMut<NextState<Scene>>,
    mut exit: MessageWriter<AppExit>,
) {
    let Some(sc) = rs.scene.as_mut() else {
        return;
    };
    let now = sc.start.elapsed().as_millis() as i64;
    let mouse_left = sc.mouse.update(now, buttons.pressed(MouseButton::Left));
    sc.right = right_state(sc.right, buttons.pressed(MouseButton::Right));
    let out = sc.state.frame(
        now,
        ResultInput {
            slots: &slots.events,
            held: slots.held,
            escape: keys.just_pressed(KeyCode::Escape),
            enter: keys.just_pressed(KeyCode::Enter),
            mouse_left,
            mouse_right: sc.right,
        },
    );
    match out {
        Some(ResultExit::Retry) => {
            let (path, seed) = sc.retry.clone();
            commands.insert_resource(PlayRequest {
                path,
                auto: false,
                seed: Some(seed),
            });
            next.set(Scene::Play);
        }
        Some(ResultExit::Select) if boot.direct => {
            exit.write(AppExit::Success);
        }
        Some(ResultExit::Select) => next.set(Scene::Select),
        None => {}
    }
}

/// リザルトスキンで描く（毎フレーム作り直す）
pub fn draw(mut p: SkinDraw, mut rs: ResMut<ResultScreen>) {
    let rs = &mut *rs;
    let Some(sc) = rs.scene.as_ref() else {
        return;
    };
    let gfx = &rs.gfx;
    let r = sc.state.result();
    let view = ResultView {
        graph: &r.graph,
        groove: r.groove,
        chart_end: sc.state.chart_end(),
    };
    let draws = draw_result(&rs.skin, &sc.state, &view, &|i| gfx.size(i));
    skin_render::render(&mut p, &mut rs.gfx, draws, Scene::Result);
}
