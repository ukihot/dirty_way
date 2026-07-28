//! `GutzInputPlugin`（骨組みのみ）。
//!
//! キーバインド抽象化は、dirty_wayと異なる操作方式を要求する2作目が
//! 出てから中身を詰める（doc/gutzgutz-requirements.md 7節 Phase 3）。
//! 現時点では「何もしないが app.add_plugins((...GutzInputPlugin...)) で
//! 差し込める」という形だけを確保する。

use bevy::prelude::*;

pub struct GutzInputPlugin;

impl Plugin for GutzInputPlugin {
    fn build(&self, _app: &mut App) {}
}
