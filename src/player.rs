use std::f32::consts::TAU;

use bevy::prelude::*;
use rand::Rng;

use crate::bubble::spawn_bubble;
use crate::consts::*;
use crate::state::GameState;

#[derive(Component)]
pub struct SoapDispenser;

/// ノズルの向き（Y軸回りの角度）。角度 0 は +Z 方向。
#[derive(Resource, Default)]
pub struct Aim {
    pub angle: f32,
}

impl Aim {
    pub fn direction(&self) -> Vec3 {
        Vec3::new(self.angle.sin(), 0.0, self.angle.cos())
    }
}

/// ノズルの押し込み（チャージ）状態。
#[derive(Resource, Default)]
pub struct Charge {
    pub charging: bool,
    pub time: f32,
}

pub struct PlayerPlugin;

impl Plugin for PlayerPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Aim>()
            .init_resource::<Charge>()
            .add_systems(Startup, spawn_player)
            .add_systems(
                Update,
                (update_aim, rotate_player_visual, handle_charge_input)
                    .chain()
                    .run_if(in_state(GameState::Playing)),
            );
    }
}

fn spawn_player(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    commands.spawn((
        SoapDispenser,
        Mesh3d(meshes.add(Capsule3d::new(0.45, 1.1))),
        MeshMaterial3d(
            materials
                .add(StandardMaterial { base_color: Color::srgb(0.95, 0.55, 0.75), ..default() }),
        ),
        Transform::from_xyz(0.0, 0.75, 0.0),
    ));
}

/// A = 反時計回り、D = 時計回りでノズルの向きを回転させる。
fn update_aim(time: Res<Time>, keyboard: Res<ButtonInput<KeyCode>>, mut aim: ResMut<Aim>) {
    let mut delta = 0.0;
    if keyboard.pressed(KeyCode::KeyA) {
        delta += AIM_ROTATE_SPEED;
    }
    if keyboard.pressed(KeyCode::KeyD) {
        delta -= AIM_ROTATE_SPEED;
    }
    aim.angle = (aim.angle + delta * time.delta_secs()).rem_euclid(TAU);
}

fn rotate_player_visual(aim: Res<Aim>, mut query: Query<&mut Transform, With<SoapDispenser>>) {
    if let Ok(mut transform) = query.single_mut() {
        transform.rotation = Quat::from_rotation_y(aim.angle);
    }
}

/// SPACE 押下でチャージ開始、押しっぱなしでチャージ継続、離した瞬間に発射。
fn handle_charge_input(
    time: Res<Time>,
    keyboard: Res<ButtonInput<KeyCode>>,
    mut charge: ResMut<Charge>,
    aim: Res<Aim>,
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    if keyboard.just_pressed(KeyCode::Space) {
        charge.charging = true;
        charge.time = 0.0;
    }

    if charge.charging && keyboard.pressed(KeyCode::Space) {
        charge.time = (charge.time + time.delta_secs()).min(CHARGE_MAX_TIME);
    }

    if keyboard.just_released(KeyCode::Space) && charge.charging {
        let fraction = (charge.time / CHARGE_MAX_TIME).clamp(0.0, 1.0);
        fire_bubble(&mut commands, &mut meshes, &mut materials, aim.direction(), fraction);
        charge.charging = false;
        charge.time = 0.0;
    }
}

fn fire_bubble(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    aim_dir: Vec3,
    fraction: f32,
) {
    let base_dir = if aim_dir.length_squared() > 0.0001 { aim_dir.normalize() } else { Vec3::Z };

    // 不器用な操作感：溜めが浅いほど狙いがブレる。
    let jitter = (1.0 - fraction) * CHARGE_MAX_JITTER;
    let jitter_angle = rand::thread_rng().gen_range(-jitter..=jitter);
    let dir = Quat::from_rotation_y(jitter_angle) * base_dir;

    let horizontal_speed = CHARGE_MIN_SPEED + (CHARGE_MAX_SPEED - CHARGE_MIN_SPEED) * fraction;
    let lob_speed = CHARGE_MIN_LOB + (CHARGE_MAX_LOB - CHARGE_MIN_LOB) * fraction;
    let velocity = dir * horizontal_speed + Vec3::Y * lob_speed;

    let spawn_pos = dir * NOZZLE_RADIUS + Vec3::Y * NOZZLE_HEIGHT;
    let radius = BUBBLE_MIN_RADIUS + (BUBBLE_MAX_RADIUS - BUBBLE_MIN_RADIUS) * fraction;
    let power = 1 + (fraction * 2.0).floor() as i32;

    spawn_bubble(commands, meshes, materials, spawn_pos, velocity, radius, power);
}
