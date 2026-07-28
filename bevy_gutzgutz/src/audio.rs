//! `GutzAudioPlugin`（骨組みのみ）。
//!
//! サウンドが必要なゲームが出てから中身を詰める
//! （doc/gutzgutz-requirements.md 7節 Phase 4）。

use bevy::prelude::*;

pub struct GutzAudioPlugin;

impl Plugin for GutzAudioPlugin {
    fn build(&self, _app: &mut App) {}
}
