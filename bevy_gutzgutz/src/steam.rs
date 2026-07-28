//! `GutzSteamPlugin`（骨組みのみ）。
//!
//! Steamworks SDKへの実依存は、実際にSteamリリースが具体化してから
//! 追加する（doc/gutzgutz-requirements.md 7節 Phase 5）。それまでは
//! `steamworks`クレートへの依存自体を持たない。

use bevy::prelude::*;

pub struct GutzSteamPlugin;

impl Plugin for GutzSteamPlugin {
    fn build(&self, _app: &mut App) {}
}
