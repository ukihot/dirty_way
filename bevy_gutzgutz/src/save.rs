//! `GutzSavePlugin`（骨組みのみ）。
//!
//! セーブ形式（serde/ron等）は、実際にセーブが必要なゲームが出てから
//! 決める（doc/gutzgutz-requirements.md 7節 Phase 4）。

use bevy::prelude::*;

pub struct GutzSavePlugin;

impl Plugin for GutzSavePlugin {
    fn build(&self, _app: &mut App) {}
}
