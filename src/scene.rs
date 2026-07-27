use avian3d::prelude::*;
use bevy::prelude::*;

use crate::consts::ARENA_RADIUS;

pub struct ScenePlugin;

impl Plugin for ScenePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, setup_scene);
    }
}

fn setup_scene(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    // カメラ：中央のサークルを見下ろす固定アングル
    // Msaa::Off: soap.rs のカスタムメタボール描画パイプラインをMSAA非対応の
    // 単純な構成にするため、このゲームでは常時オフにする。
    commands.spawn((
        Camera3d::default(),
        Msaa::Off,
        Transform::from_xyz(0.0, 18.0, 14.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));

    // 太陽光
    commands.spawn((
        DirectionalLight { shadow_maps_enabled: true, illuminance: 6000.0, ..default() },
        Transform::default().looking_at(Vec3::new(-0.4, -1.0, -0.3), Vec3::Y),
    ));

    // 補助のポイントライト
    commands.spawn((
        PointLight { intensity: 4_000_000.0, range: 40.0, shadow_maps_enabled: false, ..default() },
        Transform::from_xyz(0.0, 12.0, 0.0),
    ));

    // 床（物理コライダー + キッチンのステンレス天板のような銀色マテリアル）
    commands.spawn((
        RigidBody::Static,
        Collider::half_space(Vec3::Y),
        Mesh3d(meshes.add(Plane3d::default().mesh().size(ARENA_RADIUS * 2.2, ARENA_RADIUS * 2.2))),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: Color::srgb(0.62, 0.64, 0.66),
            metallic: 0.9,
            perceptual_roughness: 0.35,
            reflectance: 0.6,
            ..default()
        })),
    ));
}
