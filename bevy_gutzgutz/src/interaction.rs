//! Avian3Dの上に乗る、複数ゲームで繰り返し使う物理インタラクションの便利機能群。
//!
//! Avian3D自体のAPI（`RigidBody`/`Collider`等）はラップしない。ここにあるのは
//! raycast/grab-drag/explosion/impulseのような、Avian3Dの型をそのまま
//! 受け渡しする薄いユーティリティだけ。common collision layersはスコープ外
//! （レイヤー体系はゲームごとに違いすぎるため）。

mod explosion;
mod grab_drag;
mod impulse;
mod raycast;

pub use explosion::explode;
pub use grab_drag::{GutzGrabSettings, GutzGrabState};
pub use impulse::{apply_force, apply_impulse, apply_impulse_at_point};
pub use raycast::{cast_ray_from_cursor, cursor_ray};

use bevy::prelude::*;

/// grab/dragの入力処理・追従だけをシステムとして登録する。raycast/explosion/
/// impulseはステートを持たない関数群なので、ゲーム側が自分のシステムから
/// 直接呼び出す（プラグインが勝手に動かすものではない）。
pub struct GutzInteractionPlugin;

impl Plugin for GutzInteractionPlugin {
    fn build(&self, app: &mut App) {
        grab_drag::plugin(app);
    }
}
