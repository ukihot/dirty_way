use std::time::Duration;

use avian2d::prelude::*;
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
    mut materials: ResMut<Assets<ColorMaterial>>,
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
    // サイドビュー：床（Y=0）の左右どちらかの端から出現させる。
    let side = if rng.random_bool(0.5) { 1.0 } else { -1.0 };
    let kind = EnemyKind::random(&mut rng);
    let pos = Vec2::new(side * ENEMY_SPAWN_RADIUS, kind.radius());

    commands.spawn((
        Enemy { kind, health: kind.max_health() },
        RigidBody::Kinematic,
        Collider::circle(kind.radius()),
        Mesh2d(meshes.add(Circle::new(kind.radius()))),
        MeshMaterial2d(materials.add(ColorMaterial::from(kind.color()))),
        Transform::from_translation(pos.extend(0.0)),
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
        // 床（Y=0）に沿って中心(X=0)へ向かうだけの水平移動。KinematicBodyは
        // 重力の影響を受けないので、Yは常に着地時の高さのまま変わらない。
        let dir = -transform.translation.x.signum();
        let speed_multiplier = if trapped.is_some() { TRAPPED_SPEED_MULTIPLIER } else { 1.0 };
        velocity.0 = Vec2::new(dir * enemy.kind.speed() * speed_multiplier, 0.0);
    }
}

fn enemy_reach_center(
    mut commands: Commands,
    mut health: ResMut<Health>,
    mut next_state: ResMut<NextState<GameState>>,
    enemies: Query<(Entity, &Enemy, &Transform)>,
) {
    for (entity, enemy, transform) in &enemies {
        let dist = transform.translation.x.abs();
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
