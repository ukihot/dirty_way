use avian2d::prelude::*;
use bevy::camera::ScalingMode;
use bevy::prelude::*;

use crate::consts::ARENA_RADIUS;

/// 床（Y=0）の見た目の厚み。コライダー自体は`Collider::half_space`による
/// Y=0を通る無限平面なので、この厚みはあくまで見た目（Sprite）だけの値。
const FLOOR_VISUAL_THICKNESS: f32 = 0.4;

pub struct ScenePlugin;

impl Plugin for ScenePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, setup_scene);
    }
}

fn setup_scene(mut commands: Commands) {
    // カメラ：ハンドソープ筐体を真横から見た固定アングル（doc/soap-model.md
    // 2026-07-29追記：3Dトップダウンから2Dサイドビューへ方針転換）。
    // AutoMinで床の全幅＋弾道の山なりが常に画面に収まるようにする。
    // Msaa::Off: soap.rs のカスタムメタボール描画パイプラインをMSAA非対応の
    // 単純な構成にするため、このゲームでは常時オフにする。
    commands.spawn((
        Camera2d,
        Msaa::Off,
        Projection::Orthographic(OrthographicProjection {
            scaling_mode: ScalingMode::AutoMin {
                min_width: ARENA_RADIUS * 2.3,
                min_height: ARENA_RADIUS * 1.5,
            },
            ..OrthographicProjection::default_2d()
        }),
        Transform::from_xyz(0.0, ARENA_RADIUS * 0.35, 0.0),
    ));

    // 床（物理コライダー + 見た目のSprite）。Y=0を通る無限平面コライダーの
    // 上面がちょうどY=0に来るよう、見た目はその下に厚みを足して配置する。
    commands.spawn((RigidBody::Static, Collider::half_space(Vec2::Y), Transform::default()));
    commands.spawn((
        Sprite {
            color: Color::srgb(0.62, 0.64, 0.66),
            custom_size: Some(Vec2::new(ARENA_RADIUS * 2.2, FLOOR_VISUAL_THICKNESS)),
            ..default()
        },
        Transform::from_xyz(0.0, -FLOOR_VISUAL_THICKNESS * 0.5, 0.0),
    ));
}
