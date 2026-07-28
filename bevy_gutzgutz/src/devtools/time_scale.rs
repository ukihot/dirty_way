use bevy::prelude::*;
use bevy::time::Virtual;

use super::GutzDevtoolsSettings;

/// `Time<Virtual>::set_relative_speed`をキーで刻む。スライダーでの操作は
/// devtoolsオーバーレイ（mod.rsの`draw_overlay`）側で行う。
pub(crate) fn hotkeys(
    keyboard: Res<ButtonInput<KeyCode>>,
    settings: Res<GutzDevtoolsSettings>,
    mut time: ResMut<Time<Virtual>>,
) {
    if keyboard.just_pressed(KeyCode::Equal) {
        let speed = (time.relative_speed() + settings.time_scale_step).clamp(0.0, 8.0);
        time.set_relative_speed(speed);
    }
    if keyboard.just_pressed(KeyCode::Minus) {
        let speed = (time.relative_speed() - settings.time_scale_step).max(0.0);
        time.set_relative_speed(speed);
    }
}
