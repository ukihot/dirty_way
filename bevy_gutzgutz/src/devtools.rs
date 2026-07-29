//! FPS/Physics Debug/Spawn Entity/Time Scale/Skip Level/God Mode/Reload Scene/
//! Screenshotの8機能を提供する開発者向けオーバーレイ。表示はeguiで行い、
//! 本番プレイヤー向けUI（bevy_ui）とは実装方式ごと分離する。`devtools`
//! featureを外せばリリースビルドの依存グラフから完全に消える。
//!
//! "Level"や"Scene再構築"はゲームごとに構造が異なるため、gutzgutzは
//! `GutzDevtoolsEvent`の発行だけに留め、実際の処理はゲーム側が
//! `MessageReader<GutzDevtoolsEvent>`で購読して実装する（フック方式）。

mod fps;
mod god_mode;
mod physics_debug;
mod screenshot;
mod spawn_entity;
mod stats;
mod time_scale;

pub use god_mode::GutzGodMode;
pub use screenshot::take_screenshot;
pub use spawn_entity::GutzSpawnRegistry;
pub use stats::GutzDebugStats;

use avian3d::debug_render::PhysicsDebugPlugin;
use bevy::diagnostic::FrameTimeDiagnosticsPlugin;
use bevy::prelude::*;
use bevy::time::Virtual;
use bevy_egui::{egui, EguiContexts, EguiPlugin, EguiPrimaryContextPass};

/// devtoolsのキーバインド。ゲーム側で`insert_resource`して上書きできる。
#[derive(Resource, Clone, Copy)]
pub struct GutzDevtoolsSettings {
    /// オーバーレイ自体の表示/非表示。
    pub toggle_key: KeyCode,
    pub physics_debug_key: KeyCode,
    pub god_mode_key: KeyCode,
    pub screenshot_key: KeyCode,
    /// Time Scaleをキーで刻む1回分の増減幅。
    pub time_scale_step: f32,
}

impl Default for GutzDevtoolsSettings {
    fn default() -> Self {
        Self {
            toggle_key: KeyCode::F3,
            physics_debug_key: KeyCode::F2,
            god_mode_key: KeyCode::F4,
            screenshot_key: KeyCode::F12,
            time_scale_step: 0.25,
        }
    }
}

/// devtoolsオーバーレイの表示/非表示。
#[derive(Resource, Default)]
pub struct GutzDevtoolsVisible(pub bool);

/// ゲームごとに構造が異なる処理（レベル遷移・シーン再構築）をgutzgutzが
/// 知らずに済むよう、実処理はゲーム側に委ねるためのフックイベント。
#[derive(Message, Clone, Copy, Debug)]
pub enum GutzDevtoolsEvent {
    SkipLevel,
    ReloadScene,
}

pub struct GutzDevtoolsPlugin;

impl Plugin for GutzDevtoolsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<GutzDevtoolsSettings>()
            .init_resource::<GutzDevtoolsVisible>()
            .init_resource::<GutzDebugStats>()
            .init_resource::<GutzGodMode>()
            .init_resource::<GutzSpawnRegistry>()
            .add_message::<GutzDevtoolsEvent>()
            .add_plugins(FrameTimeDiagnosticsPlugin::default())
            .add_plugins(EguiPlugin::default())
            .add_plugins(PhysicsDebugPlugin)
            .add_systems(
                Update,
                (
                    toggle_visibility,
                    fps::update_fps_stats,
                    physics_debug::toggle_physics_debug,
                    time_scale::hotkeys,
                    god_mode::toggle,
                    screenshot::hotkey,
                ),
            )
            .add_systems(
                EguiPrimaryContextPass,
                draw_overlay.run_if(|visible: Res<GutzDevtoolsVisible>| visible.0),
            );
    }
}

fn toggle_visibility(
    keyboard: Res<ButtonInput<KeyCode>>,
    settings: Res<GutzDevtoolsSettings>,
    mut visible: ResMut<GutzDevtoolsVisible>,
) {
    if keyboard.just_pressed(settings.toggle_key) {
        visible.0 = !visible.0;
    }
}

#[allow(clippy::too_many_arguments)]
fn draw_overlay(
    mut contexts: EguiContexts,
    stats: Res<GutzDebugStats>,
    mut god_mode: ResMut<GutzGodMode>,
    mut time: ResMut<Time<Virtual>>,
    mut events: MessageWriter<GutzDevtoolsEvent>,
    registry: Res<GutzSpawnRegistry>,
    mut commands: Commands,
    mut selected_spawn: Local<usize>,
) {
    let Ok(ctx) = contexts.ctx_mut() else { return };

    egui::Window::new("gutzgutz devtools").show(ctx, |ui| {
        for (label, value) in stats.iter() {
            ui.label(format!("{label}: {value}"));
        }
        ui.separator();

        ui.horizontal(|ui| {
            ui.label("Time Scale");
            let mut speed = time.relative_speed();
            if ui.add(egui::Slider::new(&mut speed, 0.0..=4.0)).changed() {
                time.set_relative_speed(speed.max(0.0));
            }
        });

        ui.checkbox(&mut god_mode.0, "God Mode");

        ui.horizontal(|ui| {
            if ui.button("Skip Level").clicked() {
                events.write(GutzDevtoolsEvent::SkipLevel);
            }
            if ui.button("Reload Scene").clicked() {
                events.write(GutzDevtoolsEvent::ReloadScene);
            }
            if ui.button("Screenshot").clicked() {
                screenshot::take_screenshot(&mut commands);
            }
        });

        if !registry.is_empty() {
            ui.separator();
            ui.label("Spawn Entity");
            egui::ComboBox::from_label("prefab")
                .selected_text(registry.name_at(*selected_spawn).unwrap_or("-"))
                .show_ui(ui, |ui| {
                    for (i, name) in registry.names().enumerate() {
                        ui.selectable_value(&mut *selected_spawn, i, name);
                    }
                });
            if ui.button("Spawn at Origin").clicked() {
                registry.spawn_at(*selected_spawn, &mut commands, Vec3::ZERO);
            }
        }
    });
}
