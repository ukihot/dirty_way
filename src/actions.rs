//! プレイヤーの入力Action定義（`GutzInputPlugin`用）。
//!
//! `keyboard.pressed(KeyCode::KeyA)`のような生入力を直接見るのではなく、
//! 「プレイヤーが何をしたいか」だけをここに列挙する。実際にどのキー/ボタンが
//! 割り当たっているかは`input.toml`（読み込めない場合はここの
//! ハードコードされた既定値にフォールバックする）が決める。

use bevy::prelude::*;
use bevy_gutzgutz::input::{GutzInputMap, GutzInputSource, load_into_from_file};
use bevy_gutzgutz::lifecycle::GutzExecutionContext;

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum PlayerAction {
    RotateLeft,
    RotateRight,
    Charge,
    Restart,
}

const INPUT_CONFIG_PATH: &str = "input.toml";

pub fn setup_input_bindings(mut map: ResMut<GutzInputMap<PlayerAction>>) {
    // RotateLeft/RotateRight/ChargeはInGame（Playing）専用、Restartは
    // OutGame（GameOver）専用にする（doc：「Lifecycleによって
    // Actionの有効範囲を制御する」）。
    map.restrict_to(PlayerAction::RotateLeft, GutzExecutionContext::InGame);
    map.restrict_to(PlayerAction::RotateRight, GutzExecutionContext::InGame);
    map.restrict_to(PlayerAction::Charge, GutzExecutionContext::InGame);
    map.restrict_to(PlayerAction::Restart, GutzExecutionContext::OutGame);

    if let Err(error) = load_into_from_file(&mut map, INPUT_CONFIG_PATH, resolve_action) {
        warn!("{INPUT_CONFIG_PATH} を読み込めなかったため既定のキー割り当てを使う: {error}");
        bind_hardcoded_defaults(&mut map);
    }
}

fn resolve_action(name: &str) -> Option<PlayerAction> {
    match name {
        "rotate_left" => Some(PlayerAction::RotateLeft),
        "rotate_right" => Some(PlayerAction::RotateRight),
        "charge" => Some(PlayerAction::Charge),
        "restart" => Some(PlayerAction::Restart),
        _ => None,
    }
}

fn bind_hardcoded_defaults(map: &mut GutzInputMap<PlayerAction>) {
    map.bind(PlayerAction::RotateLeft, GutzInputSource::Key(KeyCode::KeyA));
    map.bind(PlayerAction::RotateRight, GutzInputSource::Key(KeyCode::KeyD));
    map.bind(PlayerAction::Charge, GutzInputSource::Key(KeyCode::Space));
    map.bind(PlayerAction::Restart, GutzInputSource::Key(KeyCode::KeyR));
}
