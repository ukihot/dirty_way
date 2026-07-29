use std::f32::consts::TAU;
use std::time::Duration;

use avian3d::prelude::*;
use bevy::prelude::*;
use bevy_gutzgutz::lifecycle::in_game;
use rand::RngExt;

use crate::consts::*;
use crate::state::{GameState, Health};

/// 汚れの種類。README の「油汚れ・ホコリ・泥人間」に対応。
#[derive(Clone, Copy, Debug)]
pub enum EnemyKind {
    Dust,
    Oil,
    Mud,
}

impl EnemyKind {
    fn random(rng: &mut impl RngExt) -> Self {
        match rng.random_range(0..100) {
            0..=44 => EnemyKind::Dust,
            45..=84 => EnemyKind::Oil,
            _ => EnemyKind::Mud,
        }
    }

    fn max_health(self) -> i32 {
        match self {
            EnemyKind::Dust => 1,
            EnemyKind::Oil => 2,
            EnemyKind::Mud => 4,
        }
    }

    fn speed(self) -> f32 {
        match self {
            EnemyKind::Dust => 3.4,
            EnemyKind::Oil => 2.0,
            EnemyKind::Mud => 1.15,
        }
    }

    fn radius(self) -> f32 {
        match self {
            EnemyKind::Dust => 0.35,
            EnemyKind::Oil => 0.5,
            EnemyKind::Mud => 0.75,
        }
    }

    fn color(self) -> Color {
        match self {
            EnemyKind::Dust => Color::srgb(0.78, 0.78, 0.72),
            EnemyKind::Oil => Color::srgb(0.25, 0.18, 0.05),
            EnemyKind::Mud => Color::srgb(0.42, 0.28, 0.15),
        }
    }

    pub fn score_value(self) -> u32 {
        match self {
            EnemyKind::Dust => 10,
            EnemyKind::Oil => 15,
            EnemyKind::Mud => 30,
        }
    }

    pub fn contact_damage(self) -> i32 {
        match self {
            EnemyKind::Mud => 2,
            _ => 1,
        }
    }
}

#[derive(Component)]
pub struct Enemy {
    pub kind: EnemyKind,
    pub health: i32,
}

/// 泡に埋もれている間だけ付与される鈍足状態。bubble.rs が接触中に毎フレーム
/// remaining を延長し、enemy.rs の tick_trapped が時間切れで剥がす。
#[derive(Component)]
pub struct Trapped {
    pub remaining: f32,
}

#[derive(Resource)]
pub struct EnemySpawnTimer {
    timer: Timer,
    elapsed: f32,
}

impl Default for EnemySpawnTimer {
    fn default() -> Self {
        Self {
            timer: Timer::from_seconds(ENEMY_SPAWN_INTERVAL_START, TimerMode::Repeating),
            elapsed: 0.0,
        }
    }
}

pub struct EnemyPlugin;

impl Plugin for EnemyPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<EnemySpawnTimer>().add_systems(
            Update,
            (spawn_enemies, tick_trapped, move_enemies, enemy_reach_center)
                .chain()
                .run_if(in_game::<GameState>()),
        );
    }
}

fn spawn_enemies(
    time: Res<Time>,
    mut spawn_timer: ResMut<EnemySpawnTimer>,
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    spawn_timer.elapsed += time.delta_secs();
    let interval = (ENEMY_SPAWN_INTERVAL_START - spawn_timer.elapsed * DIFFICULTY_RAMP_PER_SEC)
        .max(ENEMY_SPAWN_INTERVAL_MIN);
    spawn_timer.timer.set_duration(Duration::from_secs_f32(interval));
    spawn_timer.timer.tick(time.delta());

    if !spawn_timer.timer.just_finished() {
        return;
    }

    let mut rng = rand::rng();
    let angle = rng.random_range(0.0..TAU);
    let kind = EnemyKind::random(&mut rng);
    let pos = Vec3::new(angle.cos(), 0.0, angle.sin()) * ENEMY_SPAWN_RADIUS;

    commands.spawn((
        Enemy { kind, health: kind.max_health() },
        RigidBody::Kinematic,
        Collider::sphere(kind.radius()),
        Mesh3d(meshes.add(Sphere::new(kind.radius()))),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: kind.color(),
            perceptual_roughness: 0.85,
            ..default()
        })),
        Transform::from_translation(pos + Vec3::Y * kind.radius()),
    ));
}

/// 泡に埋もれてから TRAPPED_LINGER_TIME 秒が経つと鈍足状態を解除する。
/// （bubble.rs 側が接触中は毎フレーム remaining を延長し続ける）
fn tick_trapped(
    time: Res<Time>,
    mut commands: Commands,
    mut trapped: Query<(Entity, &mut Trapped)>,
) {
    for (entity, mut trapped) in &mut trapped {
        trapped.remaining -= time.delta_secs();
        if trapped.remaining <= 0.0 {
            commands.entity(entity).remove::<Trapped>();
        }
    }
}

fn move_enemies(mut enemies: Query<(&Enemy, &Transform, &mut LinearVelocity, Option<&Trapped>)>) {
    for (enemy, transform, mut velocity, trapped) in &mut enemies {
        let to_center = -Vec3::new(transform.translation.x, 0.0, transform.translation.z);
        let dir = to_center.normalize_or_zero();
        let speed_multiplier = if trapped.is_some() { TRAPPED_SPEED_MULTIPLIER } else { 1.0 };
        velocity.0 = dir * enemy.kind.speed() * speed_multiplier;
    }
}

fn enemy_reach_center(
    mut commands: Commands,
    mut health: ResMut<Health>,
    mut next_state: ResMut<NextState<GameState>>,
    enemies: Query<(Entity, &Enemy, &Transform)>,
) {
    for (entity, enemy, transform) in &enemies {
        let dist = Vec2::new(transform.translation.x, transform.translation.z).length();
        if dist <= CENTER_KILL_RADIUS {
            // bubble.rs 側の撃破処理と同一フレームで競合しうるため try_despawn にする。
            commands.entity(entity).try_despawn();
            health.0 -= enemy.kind.contact_damage();
            if health.0 <= 0 {
                health.0 = 0;
                next_state.set(GameState::GameOver);
            }
        }
    }
}
