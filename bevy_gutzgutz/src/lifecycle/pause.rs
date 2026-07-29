use bevy::prelude::*;
use bevy::time::Virtual;

/// ゲーム全体の一時停止フラグ。gutzgutz自身は「何のキー・UIでこれを
/// トグルするか」を決めない（それはゲーム側、あるいはdevtoolsのTime Scale
/// とは別軸の話）。トグルすると[`apply_pause_to_time`]経由で`Time<Virtual>`が
/// 連動して止まる／再開する（doc：「PauseだからTimeを止める」）。
#[derive(Resource, Default, Clone, Copy, PartialEq, Eq, Debug)]
pub struct GutzPaused(pub bool);

/// ポーズ中かどうかを見る実行条件。
///
/// ```ignore
/// app.add_systems(Update, move_enemies.run_if(in_game::<GameState>()).run_if(not_paused));
/// ```
pub fn paused(paused: Res<GutzPaused>) -> bool {
    paused.0
}

/// ポーズ中でないかどうかを見る実行条件。
pub fn not_paused(paused: Res<GutzPaused>) -> bool {
    !paused.0
}

pub(crate) fn apply_pause_to_time(paused: Res<GutzPaused>, mut time: ResMut<Time<Virtual>>) {
    if !paused.is_changed() {
        return;
    }
    if paused.0 {
        time.pause();
    } else {
        time.unpause();
    }
}
