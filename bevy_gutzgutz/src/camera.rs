//! `GutzCameraPlugin` — ワールド空間の矩形領域を少なくとも画面に収める
//! 2Dカメラのセットアップ。
//!
//! `Camera2d`/`OrthographicProjection`自体はラップしない——「最低でもこの
//! ワールド幅×高さを画面に収める（`ScalingMode::AutoMin`）」という、2Dの
//! アリーナ・トラック・盤面を持つゲーム（タワーディフェンス・レース・
//! パズル等）で繰り返し必要になる頻出パターンのボイラープレートだけを
//! 引き受ける。
//!
//! 2026-07-30追記：以前は「dirty_wayと異なるカメラ制御を要求する2作目が
//! 出てから中身を詰める」という空の骨組みだった。今回、dirty_wayの
//! `scene.rs`が持っていたカメラセットアップ（`ScalingMode::AutoMin`＋
//! ワールド座標オフセット）が複数ジャンルのゲームで共通して必要になる
//! ことが見えている（doc/gutzgutz-requirements.md参照）ため、
//! 「2作目を待つ」原則を意図的に上書きしてここへ引き上げた。

use bevy::camera::ScalingMode;
use bevy::prelude::*;

pub struct GutzCameraPlugin;

impl Plugin for GutzCameraPlugin {
    fn build(&self, _app: &mut App) {}
}

/// [`spawn_fit_camera_2d`]が使うフレーミング設定。
#[derive(Clone, Copy, Debug)]
pub struct GutzCameraFit2d {
    /// 画面に収める、ワールド空間での最小幅。
    pub min_width: f32,
    /// 画面に収める、ワールド空間での最小高さ。
    pub min_height: f32,
    /// カメラのワールド座標オフセット（`Transform`のXY。原点中心から
    /// ずらしたい場合に使う）。
    pub offset: Vec2,
}

/// `Camera2d`を、`fit`のワールド矩形が常に画面に収まる`ScalingMode::AutoMin`
/// でspawnする。返り値の`Entity`へ、ゲーム固有の追加設定（例：カスタム
/// 描画パイプラインの都合による`Msaa::Off`）を呼び出し側が`.insert`できる。
pub fn spawn_fit_camera_2d(commands: &mut Commands, fit: GutzCameraFit2d) -> Entity {
    commands
        .spawn((
            Camera2d,
            Projection::Orthographic(OrthographicProjection {
                scaling_mode: ScalingMode::AutoMin {
                    min_width: fit.min_width,
                    min_height: fit.min_height,
                },
                ..OrthographicProjection::default_2d()
            }),
            Transform::from_translation(fit.offset.extend(0.0)),
        ))
        .id()
}
