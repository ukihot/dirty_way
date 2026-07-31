use avian2d::prelude::*;
use bevy::prelude::*;
use bevy_gutzgutz::camera::{GutzCameraFit2d, spawn_fit_camera_2d};

use crate::consts::{ARENA_RADIUS, GameLayer};

/// 床エンティティの目印。`bubble.rs`が「泡が床に触れたか」を判定するのに使う。
#[derive(Component)]
pub struct Floor;

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
    // AutoMinで床の全幅＋弾道の山なりが常に画面に収まるようにする
    // （フレーミング自体はbevy_gutzgutz::cameraへ委譲。ARENA_RADIUSから
    // 決まる大きさ・オフセットはこのゲーム固有の値なのでここで渡す）。
    // Msaa::Off: soap.rs のカスタムメタボール描画パイプラインをMSAA非対応の
    // 単純な構成にするため、このゲームでは常時オフにする（カメラの
    // フレーミングとは無関係な、このゲーム固有の描画都合なのでここで挿入する）。
    let camera = spawn_fit_camera_2d(
        &mut commands,
        GutzCameraFit2d {
            min_width: ARENA_RADIUS * 2.3,
            min_height: ARENA_RADIUS * 1.5,
            offset: Vec2::new(0.0, ARENA_RADIUS * 0.35),
        },
    );
    commands.entity(camera).insert(Msaa::Off);

    // 床（物理コライダー + 見た目のSprite）。Y=0を通る無限平面コライダーの
    // 上面がちょうどY=0に来るよう、見た目はその下に厚みを足して配置する。
    commands.spawn((
        Floor,
        RigidBody::Static,
        Collider::half_space(Vec2::Y),
        // 課題S-29（2026-07-29）：着地済みの泡はCollisionLayersのmembershipsが
        // Bubble→LandedBubbleへ切り替わる（bubble.rs::settle_landed_bubbles、
        // 課題S-24）。ここにLandedBubbleを含め忘れていたため、床に直接
        // 着地して静止した瞬間、床との相互衝突条件が成立しなくなり、
        // Dynamicのまま重力に従って床をすり抜けて落ち続けてしまっていた
        // （実機確認）。
        CollisionLayers::new(
            GameLayer::Floor,
            [GameLayer::Bubble, GameLayer::LandedBubble, GameLayer::Enemy],
        ),
        Transform::default(),
    ));
    commands.spawn((
        Sprite {
            color: Color::srgb(0.62, 0.64, 0.66),
            custom_size: Some(Vec2::new(ARENA_RADIUS * 2.2, FLOOR_VISUAL_THICKNESS)),
            ..default()
        },
        Transform::from_xyz(0.0, -FLOOR_VISUAL_THICKNESS * 0.5, 0.0),
    ));
}
