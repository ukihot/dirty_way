//! `GutzUiPlugin`（骨組みのみ）。
//!
//! ゲージ・ダイアログ等の複数作品共通UI部品は、実際に2作目で同じ部品を
//! 書いたときに初めて抽出する（doc/gutzgutz-requirements.md 7節 Phase 4）。
//! 各ゲーム固有のHUD構成そのもの（スコア表示レイアウト等）はここには含めない。

use bevy::prelude::*;

pub struct GutzUiPlugin;

impl Plugin for GutzUiPlugin {
    fn build(&self, _app: &mut App) {}
}
