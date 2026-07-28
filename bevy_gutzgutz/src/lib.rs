//! gutzgutz — 自社ゲーム開発共通基盤（doc/gutzgutz-requirements.md）。
//!
//! Bevy/Avian3D本体はラップしない。複数のゲームで繰り返し使うことになる
//! 「便利機能」「開発基盤」だけを、Cargo feature単位のプラグインとして提供する。
//! ゲーム側はAvian3D・Bevyを直接使ってよい。

#[cfg(feature = "devtools")]
pub mod devtools;
#[cfg(feature = "interaction")]
pub mod interaction;

#[cfg(feature = "audio")]
pub mod audio;
#[cfg(feature = "camera")]
pub mod camera;
#[cfg(feature = "input")]
pub mod input;
#[cfg(feature = "save")]
pub mod save;
#[cfg(feature = "steam")]
pub mod steam;
#[cfg(feature = "ui")]
pub mod ui;

/// `use bevy_gutzgutz::prelude::*;` で有効化済みfeature分のプラグイン・型が
/// まとめて見えるようにする窓口。
pub mod prelude {
    #[cfg(feature = "devtools")]
    pub use crate::devtools::*;
    #[cfg(feature = "interaction")]
    pub use crate::interaction::*;

    #[cfg(feature = "audio")]
    pub use crate::audio::*;
    #[cfg(feature = "camera")]
    pub use crate::camera::*;
    #[cfg(feature = "input")]
    pub use crate::input::*;
    #[cfg(feature = "save")]
    pub use crate::save::*;
    #[cfg(feature = "steam")]
    pub use crate::steam::*;
    #[cfg(feature = "ui")]
    pub use crate::ui::*;
}
