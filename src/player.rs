use std::f32::consts::TAU;

use bevy::prelude::*;
use bevy_gutzgutz::input::GutzActionState;
use bevy_gutzgutz::lifecycle::{in_game, not_paused};
use rand::RngExt;

use crate::actions::PlayerAction;
use crate::bubble::{FoamSlotAllocator, spawn_bubble};
use crate::consts::*;
use crate::quality::{FoamQualityProfile, FoamQualitySetting};
use crate::state::GameState;

#[derive(Component)]
pub struct SoapDispenser;

/// ノズルの向き（Z軸回りの角度、画面内でのXY回転）。角度 0 は +X 方向。
/// サイドビューでは狙い方向そのものが上下左右を含む2D方向になるので、
/// 以前のような「水平角度＋別軸の山なり」という分離は不要（consts.rs参照）。
#[derive(Resource, Default)]
pub struct Aim {
    pub angle: f32,
}

impl Aim {
    pub fn direction(&self) -> Vec2 {
        Vec2::new(self.angle.cos(), self.angle.sin())
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
                    .run_if(in_game::<GameState>())
                    // update_aimの回転自体はTime<Virtual>停止（pause連動）で
                    // 自然に止まるが、handle_charge_inputのjust_pressed/
                    // just_released判定はTimeのdeltaを見ないため、pause中に
                    // Spaceを押すとチャージ開始・離した瞬間の発射がそのまま
                    // 素通りしてしまう。ポーズメニュー表示中は入力そのものを
                    // 止める。
                    .run_if(not_paused),
            );
    }
}

fn spawn_player(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
) {
    commands.spawn((
        SoapDispenser,
        Mesh2d(meshes.add(Capsule2d::new(0.45, 1.1))),
        MeshMaterial2d(materials.add(ColorMaterial::from(Color::srgb(0.95, 0.55, 0.75)))),
        Transform::from_xyz(0.0, NOZZLE_HEIGHT, 0.0),
    ));
}

/// RotateLeft = 反時計回り、RotateRight = 時計回りでノズルの向きを回転させる。
fn update_aim(time: Res<Time>, actions: Res<GutzActionState<PlayerAction>>, mut aim: ResMut<Aim>) {
    let mut delta = 0.0;
    if actions.pressed(PlayerAction::RotateLeft) {
        delta += AIM_ROTATE_SPEED;
    }
    if actions.pressed(PlayerAction::RotateRight) {
        delta -= AIM_ROTATE_SPEED;
    }
    aim.angle = (aim.angle + delta * time.delta_secs()).rem_euclid(TAU);
}

fn rotate_player_visual(aim: Res<Aim>, mut query: Query<&mut Transform, With<SoapDispenser>>) {
    if let Ok(mut transform) = query.single_mut() {
        transform.rotation = Quat::from_rotation_z(aim.angle);
    }
}

/// Charge押下でチャージ開始、押しっぱなしでチャージ継続、離した瞬間に発射。
fn handle_charge_input(
    time: Res<Time>,
    actions: Res<GutzActionState<PlayerAction>>,
    mut charge: ResMut<Charge>,
    aim: Res<Aim>,
    mut commands: Commands,
    mut foam_allocator: ResMut<FoamSlotAllocator>,
    foam_quality: Res<FoamQualitySetting>,
) {
    if actions.just_pressed(PlayerAction::Charge) {
        charge.charging = true;
        charge.time = 0.0;
    }

    if charge.charging && actions.pressed(PlayerAction::Charge) {
        charge.time = (charge.time + time.delta_secs()).min(CHARGE_MAX_TIME);
    }

    if actions.just_released(PlayerAction::Charge) && charge.charging {
        let fraction = (charge.time / CHARGE_MAX_TIME).clamp(0.0, 1.0);
        fire_bubble(
            &mut commands,
            &mut foam_allocator,
            foam_quality.0.profile(),
            aim.direction(),
            fraction,
        );
        charge.charging = false;
        charge.time = 0.0;
    }
}

fn fire_bubble(
    commands: &mut Commands,
    foam_allocator: &mut FoamSlotAllocator,
    foam_quality_profile: FoamQualityProfile,
    aim_dir: Vec2,
    fraction: f32,
) {
    let base_dir = if aim_dir.length_squared() > 0.0001 { aim_dir.normalize() } else { Vec2::X };

    // 不器用な操作感：溜めが浅いほど狙いがブレる。
    let jitter = (1.0 - fraction) * CHARGE_MAX_JITTER;
    let jitter_angle = rand::rng().random_range(-jitter..=jitter);
    let dir = base_dir.rotate(Vec2::from_angle(jitter_angle));

    // サイドビューでは狙い方向自体が上下左右を含むので、山なり軌道は
    // 重力（main.rsのGravity）が自然に作る。チャージは初速の大きさだけを決める。
    let speed = CHARGE_MIN_SPEED + (CHARGE_MAX_SPEED - CHARGE_MIN_SPEED) * fraction;
    let velocity = dir * speed;

    let spawn_pos = dir * NOZZLE_RADIUS + Vec2::new(0.0, NOZZLE_HEIGHT);
    let radius = BUBBLE_MIN_RADIUS + (BUBBLE_MAX_RADIUS - BUBBLE_MIN_RADIUS) * fraction;
    let power = 1 + (fraction * 2.0).floor() as i32;

    // Avian2Dの当たり判定を持つBubbleを発射する。見た目（soap.rs）はこの
    // Bubbleエンティティの位置・速度を毎フレーム自動で観測するので、ここから
    // 別途何かを送信する必要はない（doc/soap-model.md 第28節）。
    spawn_bubble(
        commands,
        foam_allocator,
        foam_quality_profile,
        spawn_pos,
        velocity,
        radius,
        power,
    );
}
