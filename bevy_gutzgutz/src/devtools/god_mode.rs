use bevy::prelude::*;

use super::GutzDevtoolsSettings;

/// 無敵化フラグ。gutzgutz自身はダメージ処理を持たないため、実際の無敵化は
/// ダメージ処理側がこのResourceを参照して判定を無視することで実現する。
#[derive(Resource, Default)]
pub struct GutzGodMode(pub bool);

pub(crate) fn toggle(
    keyboard: Res<ButtonInput<KeyCode>>,
    settings: Res<GutzDevtoolsSettings>,
    mut god_mode: ResMut<GutzGodMode>,
) {
    if keyboard.just_pressed(settings.god_mode_key) {
        god_mode.0 = !god_mode.0;
    }
}
