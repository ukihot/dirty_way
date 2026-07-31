use std::time::{SystemTime, UNIX_EPOCH};

use bevy::prelude::*;
use bevy::render::view::screenshot::{Screenshot, save_to_disk};

use super::GutzDevtoolsSettings;

/// プライマリウィンドウのスクリーンショットを、タイムスタンプ付きファイル名で
/// カレントディレクトリに保存する。devtoolsオーバーレイのボタンからも
/// キー1つからも呼べるように関数として公開する。
pub fn take_screenshot(commands: &mut Commands) {
    let path = format!("screenshot-{}.png", timestamp());
    commands.spawn(Screenshot::primary_window()).observe(save_to_disk(path));
}

fn timestamp() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

pub(crate) fn hotkey(
    keyboard: Res<ButtonInput<KeyCode>>,
    settings: Res<GutzDevtoolsSettings>,
    mut commands: Commands,
) {
    if keyboard.just_pressed(settings.screenshot_key) {
        take_screenshot(&mut commands);
    }
}
