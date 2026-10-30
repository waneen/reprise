//! キー設定画面（選曲で F1）。↑↓ で操作を選び、Enter のあとに押したキー・ボタン・軸を割り当てる。
//! Delete で外す。最後の行はアナログ皿の切り替え。Esc で保存して選曲に戻る
use crate::input::{InputMap, SlotInput};
use crate::{list_font, save_settings, Scene, SettingsRes};
use bevy::prelude::*;
use domain::input::{Source, SLOTS};

const NAMES: [&str; SLOTS] = [
    "皿（上）",
    "皿（下）",
    "鍵盤 1",
    "鍵盤 2",
    "鍵盤 3",
    "鍵盤 4",
    "鍵盤 5",
    "鍵盤 6",
    "鍵盤 7",
    "START",
    "SELECT",
];
/// アナログ皿を使うときの止まったとみなすまでの時間（beatoraja の既定の閾値 100 に合わせる）
const ANALOG_MS: u32 = 100;

#[derive(Resource, Default)]
pub struct KeyConfig {
    /// 選んでいる行（SLOTS 番はアナログ皿）
    cursor: usize,
    /// 次の入力を待っている
    waiting: bool,
}

#[derive(Component)]
pub struct KeyConfigText;

pub fn enter(mut commands: Commands) {
    commands.insert_resource(KeyConfig::default());
    commands.spawn((Camera2d, DespawnOnExit(Scene::KeyConfig)));
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
        KeyConfigText,
        DespawnOnExit(Scene::KeyConfig),
    ));
}

fn source_text(s: &Source) -> String {
    match s {
        Source::Key(k) => k.clone(),
        Source::Button(b) => format!("Pad {b}"),
        Source::Axis { axis, plus } => format!("Pad {axis}{}", if *plus { "+" } else { "-" }),
    }
}

pub fn input(
    keys: Res<ButtonInput<KeyCode>>,
    slots: Res<SlotInput>,
    mut kc: ResMut<KeyConfig>,
    mut map: ResMut<InputMap>,
    mut settings: ResMut<SettingsRes>,
    mut next: ResMut<NextState<Scene>>,
) {
    if kc.waiting {
        if keys.just_pressed(KeyCode::Escape) {
            kc.waiting = false;
        } else if let Some(src) = slots.last_pressed.clone() {
            map.bindings.bind(kc.cursor, src);
            kc.waiting = false;
        }
        return;
    }
    if keys.just_pressed(KeyCode::ArrowUp) {
        kc.cursor = (kc.cursor + SLOTS) % (SLOTS + 1);
    }
    if keys.just_pressed(KeyCode::ArrowDown) {
        kc.cursor = (kc.cursor + 1) % (SLOTS + 1);
    }
    if keys.just_pressed(KeyCode::Enter) {
        if kc.cursor == SLOTS {
            map.bindings.analog_scratch_ms = match map.bindings.analog_scratch_ms {
                Some(_) => None,
                None => Some(ANALOG_MS),
            };
        } else {
            kc.waiting = true;
        }
    }
    if keys.just_pressed(KeyCode::Delete) && kc.cursor < SLOTS {
        map.bindings.clear(kc.cursor);
    }
    if keys.just_pressed(KeyCode::Escape) {
        settings.saved.bindings = map.bindings.clone();
        save_settings(&settings);
        next.set(Scene::Select);
    }
}

pub fn draw(
    kc: Res<KeyConfig>,
    map: Res<InputMap>,
    slots: Res<SlotInput>,
    mut text: Query<&mut Text, With<KeyConfigText>>,
) {
    let Ok(mut text) = text.single_mut() else {
        return;
    };
    let mut s = String::from("キー設定\n\n");
    for (i, name) in NAMES.iter().enumerate() {
        let mark = if i == kc.cursor { ">" } else { " " };
        let held = if slots.held[i] { "*" } else { " " };
        let list: Vec<String> = map.bindings.slots[i].iter().map(source_text).collect();
        let waiting = if kc.waiting && i == kc.cursor {
            "  ← 割り当てる入力を押してください（Esc でやめる）"
        } else {
            ""
        };
        s += &format!("{mark}{held} {name:<8} {}{waiting}\n", list.join(", "));
    }
    let mark = if kc.cursor == SLOTS { ">" } else { " " };
    let analog = match map.bindings.analog_scratch_ms {
        Some(ms) => format!("ON（{ms}ms 止まったら離す）"),
        None => "OFF（軸は ±0.9 でボタンとして読む）".into(),
    };
    s += &format!("\n{mark}  アナログ皿: {analog}\n");
    s += "\n↑↓: 選ぶ  Enter: 割り当て・切り替え  Delete: 外す  Esc: 保存して戻る\n* は今押されている操作";
    if text.0 != s {
        text.0 = s;
    }
}
