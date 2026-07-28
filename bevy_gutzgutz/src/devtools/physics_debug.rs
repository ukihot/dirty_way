use avian3d::debug_render::PhysicsGizmos;
use bevy::prelude::*;

use super::GutzDevtoolsSettings;

/// Avian3D自身の`PhysicsDebugPlugin`（`GutzDevtoolsPlugin`が追加する）が持つ
/// gizmo描画を、キー1つでオン/オフする薄い配線だけを行う。デバッグ描画自体は
/// 再実装しない（doc/gutzgutz-requirements.md 2節・5節）。
pub(crate) fn toggle_physics_debug(
    keyboard: Res<ButtonInput<KeyCode>>,
    settings: Res<GutzDevtoolsSettings>,
    mut store: ResMut<GizmoConfigStore>,
) {
    if keyboard.just_pressed(settings.physics_debug_key) {
        let config = store.config_mut::<PhysicsGizmos>().0;
        config.enabled = !config.enabled;
    }
}
