//! `GutzCameraPlugin`（骨組みのみ）。
//!
//! dirty_wayと異なるカメラ制御を要求する2作目が出てから中身を詰める
//! （doc/gutzgutz-requirements.md 7節 Phase 3）。

use bevy::prelude::*;

pub struct GutzCameraPlugin;

impl Plugin for GutzCameraPlugin {
    fn build(&self, _app: &mut App) {}
}
