//! gutzgutz — 自社ゲーム開発共通基盤（README.md）。
//!
//! Bevy/Avian3D本体はラップしない。複数のゲームで繰り返し使うことになる
//! 「便利機能」「開発基盤」だけを、Cargo
//! feature単位のプラグインとして提供する。 ゲーム側はAvian3D・
//! Bevyを直接使ってよい。

#[cfg(feature = "atlas")]
pub mod atlas;
#[cfg(feature = "atlas-build")]
pub mod atlas_build;
#[cfg(feature = "atlas-sprite2d")]
pub mod atlas_sprite2d;
#[cfg(feature = "devtools")]
pub mod devtools;
#[cfg(feature = "interaction")]
pub mod interaction;
#[cfg(feature = "lifecycle")]
pub mod lifecycle;

#[cfg(feature = "audio")]
pub mod audio;
#[cfg(feature = "camera")]
pub mod camera;
#[cfg(feature = "input")]
pub mod input;
#[cfg(feature = "pacing")]
pub mod pacing;
#[cfg(feature = "save")]
pub mod save;
#[cfg(feature = "session")]
pub mod session;
#[cfg(feature = "steam")]
pub mod steam;
#[cfg(feature = "ui")]
pub mod ui;

/// `use bevy_gutzgutz::prelude::*;` で有効化済みfeature分のプラグイン・型が
/// まとめて見えるようにする窓口。
pub mod prelude {
    #[cfg(feature = "atlas")]
    pub use crate::atlas::*;
    #[cfg(feature = "atlas-sprite2d")]
    pub use crate::atlas_sprite2d::*;
    #[cfg(feature = "audio")]
    pub use crate::audio::*;
    #[cfg(feature = "camera")]
    pub use crate::camera::*;
    #[cfg(feature = "devtools")]
    pub use crate::devtools::*;
    #[cfg(feature = "input")]
    pub use crate::input::*;
    #[cfg(feature = "interaction")]
    pub use crate::interaction::*;
    #[cfg(feature = "lifecycle")]
    pub use crate::lifecycle::*;
    #[cfg(feature = "pacing")]
    pub use crate::pacing::*;
    #[cfg(feature = "save")]
    pub use crate::save::*;
    #[cfg(feature = "session")]
    pub use crate::session::*;
    #[cfg(feature = "steam")]
    pub use crate::steam::*;
    #[cfg(feature = "ui")]
    pub use crate::ui::*;
}
