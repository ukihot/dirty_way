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
    /// ポーズメニューの開閉トグル。`GutzPaused`を反転させるだけの
    /// システム（state.rs `toggle_pause`）がこれを見る。
    Pause,
    /// ポーズメニューから「タイトルに戻る」。InGame中は常に許可されるが
    /// （Pauseと同じ理由。下記コメント参照）、実際に効くのはポーズ中だけ
    /// （state.rs `quit_to_title`が`GutzPaused`を見て絞る）。
    QuitToTitle,
}

const INPUT_CONFIG_PATH: &str = "input.toml";

pub fn setup_input_bindings(mut map: ResMut<GutzInputMap<PlayerAction>>) {
    // RotateLeft/RotateRight/ChargeはInGame（Playing）専用、Restartは
    // OutGame（Title/GameOver）専用にする（doc：「Lifecycleによって
    // Actionの有効範囲を制御する」）。
    //
    // Pause/QuitToTitleは両方InGame専用にする——ポーズ中もGameStateは
    // Playingのまま変わらない（`GutzPaused`はGameStateと直交する別軸）ため、
    // 実行コンテキストだけでは「ポーズ中か否か」を区別できない。
    // QuitToTitleを本当にポーズ中だけに絞る判定は、Actionの許可範囲
    // ではなくシステム側の`run_if(paused)`で行う（state.rs参照）。
    map.restrict_to(PlayerAction::RotateLeft, GutzExecutionContext::InGame);
    map.restrict_to(PlayerAction::RotateRight, GutzExecutionContext::InGame);
    map.restrict_to(PlayerAction::Charge, GutzExecutionContext::InGame);
    map.restrict_to(PlayerAction::Restart, GutzExecutionContext::OutGame);
    map.restrict_to(PlayerAction::Pause, GutzExecutionContext::InGame);
    map.restrict_to(PlayerAction::QuitToTitle, GutzExecutionContext::InGame);

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
        "pause" => Some(PlayerAction::Pause),
        "quit_to_title" => Some(PlayerAction::QuitToTitle),
        _ => None,
    }
}

fn bind_hardcoded_defaults(map: &mut GutzInputMap<PlayerAction>) {
    map.bind(PlayerAction::RotateLeft, GutzInputSource::Key(KeyCode::KeyA));
    map.bind(PlayerAction::RotateRight, GutzInputSource::Key(KeyCode::KeyD));
    map.bind(PlayerAction::Charge, GutzInputSource::Key(KeyCode::Space));
    map.bind(PlayerAction::Restart, GutzInputSource::Key(KeyCode::KeyR));
    map.bind(PlayerAction::Pause, GutzInputSource::Key(KeyCode::Escape));
    map.bind(PlayerAction::QuitToTitle, GutzInputSource::Key(KeyCode::KeyQ));
}
