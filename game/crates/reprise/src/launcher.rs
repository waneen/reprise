//! ランチャー（設定画面）。LR2 は本体の前にランチャー（`LR2.exe`）で譜面のフォルダ・プレイオプション・画面・音などを選び、
//! 「開始」で本体を起動する。reprise も起動するとまずここを出し、「開始」（Enter）で選んだものを `config.json`
//! （ブラウザ版はページの保存領域）に保存してから曲一覧・記録・スキンを用意して選曲画面に入る（[`crate::start`]）。
//! 画面は egui（`bevy_egui`）で描く。ネイティブ版は譜面のフォルダ、ブラウザ版は曲サーバーの URL と合言葉を選ぶ（reprise の拡張）。
//! 決まり（一覧に足す・URL・範囲）は `application::launcher`。
use crate::platform::{self, ConnectError};
use crate::{save_settings, Launch, Scene, SettingsRes};
use application::launcher::{add_entry, clamped};
use application::settings::{ScreenMode, Settings};
use bevy::prelude::*;
use bevy_egui::{
    egui, EguiContexts, EguiGlobalSettings, EguiPlugin, EguiPrimaryContextPass, PrimaryEguiContext,
};
use domain::play::{GaugeType, HsFix};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

pub struct LauncherPlugin;

impl Plugin for LauncherPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(EguiPlugin::default())
            .insert_resource(EguiGlobalSettings {
                // ランチャーのカメラにだけ付ける（ほかの場面の入力に関わらない）
                auto_create_primary_context: false,
                ..default()
            })
            .init_resource::<LauncherUi>()
            .add_systems(OnEnter(Scene::Launcher), enter)
            .add_systems(EguiPrimaryContextPass, ui.run_if(in_state(Scene::Launcher)))
            .add_systems(Update, begin.run_if(in_state(Scene::Launcher)));
    }
}

/// ランチャーの状態（選んでいる設定と入力欄）
#[derive(Resource, Default)]
pub struct LauncherUi {
    form: Option<Settings>,
    new_song: String,
    new_table: String,
    rebuild: bool,
    status: String,
    /// 難易度表の取り直しの結果（取り直している間は None）
    tables_job: Option<Arc<Mutex<Option<bool>>>>,
    fonts: bool,
    /// 日本語の字があるか（無ければ英語で出す）
    ja: bool,
    start: bool,
}

fn enter(mut commands: Commands) {
    commands.spawn((Camera2d, PrimaryEguiContext, DespawnOnExit(Scene::Launcher)));
}

/// 日本語の字が無ければ英語
fn t<'a>(ja: bool, j: &'a str, e: &'a str) -> &'a str {
    if ja {
        j
    } else {
        e
    }
}

fn set_fonts(ctx: &egui::Context) -> bool {
    let Some((bytes, index)) = platform::japanese_font() else {
        return false;
    };
    let mut fonts = egui::FontDefinitions::default();
    fonts.font_data.insert(
        "ja".into(),
        Arc::new(egui::FontData {
            index,
            ..egui::FontData::from_owned(bytes)
        }),
    );
    for f in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
        fonts.families.entry(f).or_default().push("ja".into());
    }
    ctx.set_fonts(fonts);
    true
}

const GAUGES: [(GaugeType, &str); 6] = [
    (GaugeType::Groove, "GROOVE"),
    (GaugeType::Easy, "EASY"),
    (GaugeType::Hard, "HARD"),
    (GaugeType::Hazard, "DEATH"),
    (GaugeType::PAttack, "P-ATTACK"),
    (GaugeType::GAttack, "G-ATTACK"),
];

const HS_FIX: [(HsFix, &str); 6] = [
    (HsFix::Off, "OFF"),
    (HsFix::MaxBpm, "MAX BPM"),
    (HsFix::MinBpm, "MIN BPM"),
    (HsFix::Average, "AVERAGE"),
    (HsFix::Constant, "CONSTANT"),
    (HsFix::MainBpm, "MAIN BPM"),
];

/// ウィンドウの大きさの選択肢（LR2 のスキンの 640×480 と、よく使う大きさ）
const SIZES: [(u32, u32); 3] = [(640, 480), (1280, 720), (1920, 1080)];

fn combo<T: PartialEq + Copy>(ui: &mut egui::Ui, id: &str, v: &mut T, items: &[(T, &str)]) {
    let now = items.iter().find(|(x, _)| x == v).map_or("", |(_, n)| n);
    egui::ComboBox::from_id_salt(id)
        .selected_text(now)
        .show_ui(ui, |ui| {
            for (x, n) in items {
                ui.selectable_value(v, *x, *n);
            }
        });
}

/// パスの入力欄（空なら無し）
fn path_field(ui: &mut egui::Ui, label: &str, v: &mut Option<PathBuf>) {
    ui.horizontal(|ui| {
        ui.label(label);
        let mut text = v
            .as_ref()
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_default();
        if ui
            .add(egui::TextEdit::singleline(&mut text).desired_width(420.0))
            .changed()
        {
            let t = text.trim();
            *v = (!t.is_empty()).then(|| PathBuf::from(t));
        }
    });
}

/// 一覧（消すボタン付き）と足す欄
fn list_field<T: PartialEq + From<String>>(
    ui: &mut egui::Ui,
    ja: bool,
    list: &mut Vec<T>,
    show: impl Fn(&T) -> String,
    new: &mut String,
) {
    let mut remove = None;
    for (i, item) in list.iter().enumerate() {
        ui.horizontal(|ui| {
            if ui.small_button(t(ja, "削除", "Remove")).clicked() {
                remove = Some(i);
            }
            ui.label(show(item));
        });
    }
    if let Some(i) = remove {
        list.remove(i);
    }
    ui.horizontal(|ui| {
        let r = ui.add(egui::TextEdit::singleline(new).desired_width(420.0));
        let enter = r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
        if ui.button(t(ja, "追加", "Add")).clicked() || enter {
            add_entry(list, new);
            new.clear();
        }
    });
}

fn ui(
    mut contexts: EguiContexts,
    mut st: ResMut<LauncherUi>,
    settings: Res<SettingsRes>,
    launch: Res<Launch>,
) -> Result {
    let ctx = contexts.ctx_mut()?;
    let st = &mut *st;
    if !st.fonts {
        st.fonts = true;
        st.ja = set_fonts(ctx);
    }
    let ja = st.ja;
    let typing = ctx.text_edit_focused();
    let form = st.form.get_or_insert_with(|| {
        let mut s = settings.saved.clone();
        if s.server.url.is_empty() {
            if let Some(h) = platform::server_hint() {
                s.server = h;
            }
        }
        s
    });
    let done = st
        .tables_job
        .as_ref()
        .and_then(|j| *j.lock().unwrap_or_else(|e| e.into_inner()));
    if let Some(ok) = done {
        st.status = if ok {
            t(ja, "難易度表を取り直しました", "Tables updated")
        } else {
            t(ja, "難易度表を取れませんでした", "Could not fetch tables")
        }
        .into();
        st.tables_job = None;
    }
    let mut start = false;
    let mut root = egui::Ui::new(
        ctx.clone(),
        "launcher".into(),
        egui::UiBuilder::new()
            .layer_id(egui::LayerId::background())
            .max_rect(ctx.viewport_rect()),
    );
    egui::Panel::bottom("start").show(&mut root, |ui| {
        ui.horizontal(|ui| {
            let b = ui.add_sized([120.0, 32.0], egui::Button::new(t(ja, "開始", "Start")));
            start = b.clicked();
            ui.label(&st.status);
        });
    });
    egui::CentralPanel::default().show(&mut root, |ui| {
        egui::ScrollArea::vertical().show(ui, |ui| {
            if platform::BROWSER {
                ui.heading(t(ja, "曲サーバー", "Song server"));
                ui.horizontal(|ui| {
                    ui.label("URL");
                    ui.add(egui::TextEdit::singleline(&mut form.server.url).desired_width(360.0));
                });
                ui.horizontal(|ui| {
                    ui.label(t(ja, "合言葉", "Token"));
                    ui.add(
                        egui::TextEdit::singleline(&mut form.server.token)
                            .password(true)
                            .desired_width(240.0),
                    );
                });
                ui.label(t(
                    ja,
                    "空にするとページで選んだフォルダを使います",
                    "Leave empty to use the folders chosen on the page",
                ));
            } else {
                ui.heading(t(ja, "譜面のフォルダ", "BMS folders"));
                list_field(
                    ui,
                    ja,
                    &mut form.songs,
                    |p| p.display().to_string(),
                    &mut st.new_song,
                );
                ui.checkbox(
                    &mut st.rebuild,
                    t(ja, "曲の DB を作り直す", "Rebuild the song database"),
                );
            }
            ui.separator();
            ui.heading(t(ja, "難易度表", "Difficulty tables"));
            list_field(ui, ja, &mut form.tables, String::clone, &mut st.new_table);
            let busy = st.tables_job.is_some();
            if ui
                .add_enabled(
                    !busy,
                    egui::Button::new(t(ja, "難易度表を取り直す", "Update tables")),
                )
                .clicked()
            {
                st.status = t(ja, "難易度表を取っています…", "Fetching tables…").into();
                st.tables_job = Some(update_tables(&launch, form));
            }
            ui.separator();
            ui.heading(t(ja, "スキン", "Skins"));
            path_field(
                ui,
                t(ja, "LR2 本体のフォルダ", "LR2 folder"),
                &mut form.skin_root,
            );
            path_field(ui, t(ja, "プレイ", "Play"), &mut form.skin);
            path_field(ui, t(ja, "選曲", "Select"), &mut form.select_skin);
            path_field(ui, t(ja, "リザルト", "Result"), &mut form.result_skin);
            path_field(
                ui,
                t(ja, "キー設定", "Key config"),
                &mut form.keyconfig_skin,
            );
            ui.label(t(
                ja,
                "空のスキンは LR2 本体の設定（config.xml）で選んでいるもの",
                "Empty skins use the ones chosen in LR2's config.xml",
            ));
            ui.separator();
            ui.heading(t(ja, "プレイオプション", "Play options"));
            let o = &mut form.options;
            egui::Grid::new("options").num_columns(2).show(ui, |ui| {
                ui.label(t(ja, "ゲージ", "Gauge"));
                combo(ui, "gauge", &mut form.gauge, &GAUGES);
                ui.end_row();
                ui.label(t(ja, "ハイスピード", "Hi-speed"));
                ui.add(
                    egui::DragValue::new(&mut o.hi_speed)
                        .range(10..=900)
                        .speed(10)
                        .custom_formatter(|v, _| format!("{:.2}", v / 100.0)),
                );
                ui.end_row();
                ui.label("HS-FIX");
                combo(ui, "hsfix", &mut o.hs_fix, &HS_FIX);
                ui.end_row();
                ui.label(t(ja, "レーンカバー", "Lane cover"));
                ui.horizontal(|ui| {
                    ui.checkbox(&mut o.lane_cover, "");
                    ui.add_enabled(
                        o.lane_cover,
                        egui::DragValue::new(&mut o.cover).range(0..=100),
                    );
                });
                ui.end_row();
                ui.label(t(ja, "判定タイミング（ms）", "Judge timing (ms)"));
                ui.add(egui::DragValue::new(&mut o.judge_offset).range(-199..=199));
                ui.end_row();
                ui.label(t(ja, "プレビュー音", "Song preview"));
                ui.checkbox(&mut form.preview, "");
                ui.end_row();
            });
            ui.separator();
            ui.heading(t(ja, "画面", "Display"));
            let sc = &mut form.screen;
            ui.horizontal(|ui| {
                ui.radio_value(
                    &mut sc.mode,
                    ScreenMode::Window,
                    t(ja, "ウィンドウ", "Window"),
                );
                ui.radio_value(
                    &mut sc.mode,
                    ScreenMode::Fullscreen,
                    t(ja, "フルスクリーン", "Fullscreen"),
                );
            });
            ui.horizontal(|ui| {
                ui.label(t(ja, "大きさ", "Size"));
                ui.add(egui::DragValue::new(&mut sc.width).range(320..=7680));
                ui.label("×");
                ui.add(egui::DragValue::new(&mut sc.height).range(240..=4320));
                for (w, h) in SIZES {
                    if ui.small_button(format!("{w}×{h}")).clicked() {
                        (sc.width, sc.height) = (w, h);
                    }
                }
            });
            ui.checkbox(&mut sc.vsync, t(ja, "垂直同期", "V-Sync"));
            ui.separator();
            ui.heading(t(ja, "音", "Sound"));
            ui.add(egui::Slider::new(&mut form.volume, 0..=100).text(t(ja, "音量", "Volume")));
        });
    });
    // Enter は「開始」（LR2 のランチャーの既定のボタン）。入力欄で押したとき（フレームの初めに入力欄にいた）は除く
    if start || (!typing && ctx.input(|i| i.key_pressed(egui::Key::Enter))) {
        match platform::connect(&form.server) {
            Ok(()) => st.start = true,
            Err(e) => st.status = connect_error(ja, &e),
        }
    }
    Ok(())
}

fn connect_error(ja: bool, e: &ConnectError) -> String {
    match e {
        ConnectError::BadUrl => t(
            ja,
            "URL は http:// か https:// で始めてください",
            "The URL must start with http:// or https://",
        )
        .into(),
        ConnectError::WrongToken => t(ja, "合言葉が違います", "Wrong token").into(),
        ConnectError::Unreachable => {
            t(ja, "曲サーバーにつながりません", "Cannot reach the server").into()
        }
        ConnectError::Status(s) => format!("{} ({s})", t(ja, "曲サーバーの誤り", "Server error")),
    }
}

/// 難易度表を取り直す（ネイティブ版は別のスレッド、ブラウザ版は曲サーバーに頼む）
fn update_tables(launch: &Launch, form: &Settings) -> Arc<Mutex<Option<bool>>> {
    let out = Arc::new(Mutex::new(None));
    let (dir, urls, server) = (
        launch.data_dir.clone(),
        form.tables.clone(),
        form.server.clone(),
    );
    let job = out.clone();
    let run = move || {
        let ok = platform::connect(&server).is_ok() && platform::update_tables(&dir, &urls);
        *job.lock().unwrap_or_else(|e| e.into_inner()) = Some(ok);
    };
    if cfg!(target_arch = "wasm32") {
        run();
    } else {
        std::thread::spawn(run);
    }
    out
}

/// 「開始」を押したら、選んだものを保存して始める
fn begin(world: &mut World) {
    let (form, rebuild) = {
        let mut st = world.resource_mut::<LauncherUi>();
        if !st.start {
            return;
        }
        st.start = false;
        (st.form.take(), st.rebuild)
    };
    if let Some(form) = form {
        let mut s = world.resource_mut::<SettingsRes>();
        s.saved = clamped(form);
        s.saved.options = s.saved.options.clamped();
        save_settings(&s);
    }
    world.resource_mut::<Launch>().rebuild = rebuild;
    crate::start(world);
    world.resource_mut::<NextState<Scene>>().set(Scene::Select);
}
