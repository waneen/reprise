//! キー設定画面。LR2 と同じく選曲スキンの BUTTON 13 から入り、キー設定スキン（`#INFORMATION` 種別 8）で描く。
//! 割り当てはスキンの BUTTON（キー番号 100..139・7 鍵の並び 140・位置 150..157）で選び、物理入力を押して置く
//! （Delete で外す、F1 で既定、F2 ですべて外す）。Esc か右クリックで `#FADEOUT` の後に選曲に戻る。
//! 規則は `domain::keyconfig`・`application::keyconfig_skin`。キー設定スキンが無ければこの画面には入らない（LR2 にも代わりの画面は無い）
use crate::input::{InputMap, SlotInput};
use crate::select_scene::mouse_pos;
use crate::skin_render::{self, Pixels, SkinDraw, SkinGfx};
use crate::{save_settings, Scene, SettingsRes};
use application::keyconfig_skin::{KeyConfigInput, KeyConfigSkin};
use bevy::platform::time::Instant;
use bevy::prelude::*;
use domain::select::MouseButton as Lr2Mouse;
use domain::skin::lr2::{button_clicks, draw_keyconfig};
use domain::skin::Skin;

/// キー設定スキンとその画面の状態（スキンが読めたときだけある）
#[derive(Resource)]
pub struct KeyConfigScreen {
    skin: Skin,
    /// 画像（最初に画面に入ったときに Bevy に渡す）
    pixels: Option<Vec<Option<Pixels>>>,
    gfx: SkinGfx,
    /// 画面に入るたびに作り直すもの
    scene: Option<KeyConfigScene>,
}

struct KeyConfigScene {
    state: KeyConfigSkin,
    mouse: Lr2Mouse,
    start: Instant,
}

impl KeyConfigScreen {
    pub fn new(skin: Skin, pixels: Vec<Option<Pixels>>) -> Self {
        Self {
            skin,
            pixels: Some(pixels),
            gfx: SkinGfx::default(),
            scene: None,
        }
    }
}

pub fn enter(
    mut commands: Commands,
    mut kc: ResMut<KeyConfigScreen>,
    mut images: ResMut<Assets<Image>>,
    map: Res<InputMap>,
    buttons: Res<ButtonInput<MouseButton>>,
) {
    let kc = &mut *kc;
    commands.spawn((
        skin_render::camera(kc.skin.resolution),
        DespawnOnExit(Scene::KeyConfig),
    ));
    if let Some(pixels) = kc.pixels.take() {
        kc.gfx = SkinGfx::new(pixels, kc.skin.resolution, &mut images);
    }
    // 文字列の実体は前に画面を出たときに消えている
    kc.gfx.forget_texts();
    // LR2 のマウスの状態は場面をまたいで続くので、BUTTON 13 を押したままならこの画面では押し続けになる
    let mut mouse = Lr2Mouse::default();
    mouse.update(0, buttons.pressed(MouseButton::Left));
    kc.scene = Some(KeyConfigScene {
        state: KeyConfigSkin::new(kc.skin.timing, &kc.skin.options, &map.bindings),
        mouse,
        start: Instant::now(),
    });
}

/// 入力（LR2 `ReactInput` の BUTTON と `ProcI_Keyconfig`）。割り当ては入力の割り当て（`InputMap`）をそのまま変え、
/// 保存するときは設定にも写して `config.json` に書く
#[allow(clippy::too_many_arguments)]
pub fn input(
    buttons: Res<ButtonInput<MouseButton>>,
    slots: Res<SlotInput>,
    windows: Query<&Window>,
    cams: Query<(&Camera, &GlobalTransform)>,
    mut kc: ResMut<KeyConfigScreen>,
    mut map: ResMut<InputMap>,
    mut settings: ResMut<SettingsRes>,
    mut next: ResMut<NextState<Scene>>,
) {
    let kc = &mut *kc;
    let Some(sc) = kc.scene.as_mut() else {
        return;
    };
    let now = sc.start.elapsed().as_millis() as i64;
    let mouse = sc.mouse.update(now, buttons.pressed(MouseButton::Left));
    let clicks = match mouse_pos(&windows, &cams, kc.skin.resolution) {
        // パネルは無い（−1）
        Some((x, y)) if mouse == 1 => button_clicks(&kc.skin, &sc.state, x, y, -1),
        _ => vec![],
    };
    let out = sc.state.frame(
        now,
        &mut map.bindings,
        KeyConfigInput {
            pressed: &slots.sources,
            clicks: &clicks,
            right_click: buttons.just_pressed(MouseButton::Right),
            slots: &slots.events,
        },
    );
    if out.save {
        settings.saved.bindings = map.bindings.clone();
        save_settings(&settings);
    }
    if out.close {
        next.set(Scene::Select);
    }
}

/// 画面を出る: LR2 は選曲に入るときに保存した割り当てを読み直すので、保存していない変更（F2）は捨てる
pub fn exit(mut map: ResMut<InputMap>, settings: Res<SettingsRes>) {
    map.bindings = settings.saved.bindings.clone();
}

/// キー設定スキンで描く（毎フレーム作り直す）
pub fn draw(mut p: SkinDraw, mut kc: ResMut<KeyConfigScreen>) {
    let kc = &mut *kc;
    let Some(sc) = kc.scene.as_ref() else {
        return;
    };
    let gfx = &kc.gfx;
    let draws = draw_keyconfig(&kc.skin, &sc.state, &|i| gfx.size(i));
    skin_render::render(&mut p, &mut kc.gfx, draws, Scene::KeyConfig);
}
