use avian3d::prelude::*;
use bevy::prelude::*;
use rand::Rng;

use crate::consts::*;
use crate::enemy::{Enemy, Trapped};
use crate::state::{GameState, Score};

/// プレイヤーが発射する泡。power はダメージ量（チャージが深いほど増える）。
/// 敵に当たっても即座には消えず、
/// しばらく居座って「埋もれた」敵の足止めを続ける。
#[derive(Component)]
pub struct Bubble {
    pub power: i32,
    pub life: f32,
    /// このバブルが既にダメージを与えた敵（同じ相手に毎フレーム連続ダメージしないため）。
    hit_enemies: Vec<Entity>,
}

pub struct BubblePlugin;

impl Plugin for BubblePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (tick_bubble_lifetime, despawn_out_of_bounds, bubble_enemy_interaction)
                .run_if(in_state(GameState::Playing)),
        );
    }
}

pub fn spawn_bubble(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    position: Vec3,
    velocity: Vec3,
    radius: f32,
    power: i32,
) {
    let material = materials.add(StandardMaterial {
        base_color: Color::srgba(0.85, 0.95, 1.0, 0.6),
        alpha_mode: AlphaMode::Blend,
        perceptual_roughness: 0.15,
        reflectance: 0.6,
        ..default()
    });

    let mut rng = rand::thread_rng();
    let voxel_count = rng.gen_range(BUBBLE_VOXEL_MIN_COUNT..=BUBBLE_VOXEL_MAX_COUNT);

    commands
        .spawn((
            Bubble { power, life: 0.0, hit_enemies: Vec::new() },
            RigidBody::Dynamic,
            Collider::sphere(radius),
            Restitution::new(0.55),
            Friction::new(0.05),
            LinearDamping(0.15),
            AngularDamping(0.4),
            LinearVelocity(velocity),
            Transform::from_translation(position),
            Visibility::default(),
        ))
        .with_children(|parent| {
            // 毎回ランダムな個数・大きさ・配置のボクセルを寄せ集めて、
            // 同じ形にならない「雲塊」っぽい見た目を作る。
            for _ in 0..voxel_count {
                let voxel_size = radius * rng.gen_range(0.45..0.75);
                let offset_dir = Vec3::new(
                    rng.gen_range(-1.0f32..1.0),
                    rng.gen_range(-1.0f32..1.0),
                    rng.gen_range(-1.0f32..1.0),
                )
                .normalize_or_zero();
                let offset = offset_dir * rng.gen_range(0.0f32..radius * 0.55);

                parent.spawn((
                    Mesh3d(meshes.add(Cuboid::new(voxel_size, voxel_size, voxel_size))),
                    MeshMaterial3d(material.clone()),
                    Transform::from_translation(offset),
                ));
            }
        });
}

fn tick_bubble_lifetime(
    time: Res<Time>,
    mut commands: Commands,
    mut bubbles: Query<(Entity, &mut Bubble)>,
) {
    for (entity, mut bubble) in &mut bubbles {
        bubble.life += time.delta_secs();
        if bubble.life > BUBBLE_LIFETIME {
            commands.entity(entity).despawn();
        }
    }
}

fn despawn_out_of_bounds(
    mut commands: Commands,
    bubbles: Query<(Entity, &Transform), With<Bubble>>,
) {
    for (entity, transform) in &bubbles {
        if transform.translation.y < -5.0
            || transform.translation.length() > BUBBLE_DESPAWN_DISTANCE
        {
            commands.entity(entity).despawn();
        }
    }
}

/// 泡は敵に触れても即ポップしない。接触している間は毎フレーム鈍足状態を
/// 更新し続け（＝埋もれて動けなくなる）、
/// ダメージだけは初回接触時に一度だけ与える。
fn bubble_enemy_interaction(
    mut commands: Commands,
    collisions: Collisions,
    mut bubbles: Query<(Entity, &mut Bubble)>,
    mut enemies: Query<&mut Enemy>,
    mut score: ResMut<Score>,
) {
    for (bubble_entity, mut bubble) in &mut bubbles {
        for other in collisions.entities_colliding_with(bubble_entity) {
            let Ok(mut enemy) = enemies.get_mut(other) else {
                continue;
            };

            commands.entity(other).insert(Trapped { remaining: TRAPPED_LINGER_TIME });

            if bubble.hit_enemies.contains(&other) {
                continue;
            }
            bubble.hit_enemies.push(other);

            enemy.health -= bubble.power;
            if enemy.health <= 0 {
                score.0 += enemy.kind.score_value();
                commands.entity(other).despawn();
            }
        }
    }
}
