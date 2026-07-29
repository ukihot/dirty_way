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

/// ノズルの押し込み具合（doc/soap-issues.md 2026-07-29追記：本物のハンド
/// ソープ容器のポンプ機構）。0.0=完全に戻った状態、1.0=最深部まで
/// 押し込んだ状態（それ以上は噴射できない）。このリソース自体が「残量
/// ゲージ」を兼ねる——別途UIのゲージは持たない。ノズルの見た目の高さは
/// 常にこの値から直接計算する。
#[derive(Resource, Default)]
pub struct NozzlePress {
    pub depth: f32,
    /// 次に泡を発射できるようになるまでの残り秒数（SPRAY_INTERVAL間隔で
    /// 連射するためのクールダウン）。
    spray_cooldown: f32,
}

pub struct PlayerPlugin;

impl Plugin for PlayerPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Aim>()
            .init_resource::<NozzlePress>()
            .add_systems(Startup, spawn_player)
            .add_systems(
                Update,
                (update_aim, update_nozzle_press, update_player_visual)
                    .chain()
                    .run_if(in_game::<GameState>())
                    // ポーズ中はTime<Virtual>が止まるのでdelta_secsベースの
                    // press/release進行は自然に止まるが、`pressed()`の入力
                    // 判定自体は止まらないため、念のためポーズ中は入力を
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

/// 本物のハンドソープ容器のポンプ機構（doc/soap-issues.md 2026-07-29追記）。
/// Chargeを押している間はノズルが沈み込みながら連続的に泡を噴射し続け、
/// 最深部まで沈みきったら（＝ポンプストロークを使い切ったら）それ以上は
/// 出ない。離すとバネで元の高さまで戻り、再び噴射できるようになる。
fn update_nozzle_press(
    time: Res<Time>,
    actions: Res<GutzActionState<PlayerAction>>,
    mut press: ResMut<NozzlePress>,
    aim: Res<Aim>,
    mut commands: Commands,
    mut foam_allocator: ResMut<FoamSlotAllocator>,
    foam_quality: Res<FoamQualitySetting>,
) {
    let dt = time.delta_secs();
    let held = actions.pressed(PlayerAction::Charge);

    if held {
        press.depth = (press.depth + dt / NOZZLE_PRESS_TIME).min(1.0);
    } else {
        press.depth = (press.depth - dt / NOZZLE_RELEASE_TIME).max(0.0);
        // 離している間はクールダウンを持ち越さない（次に押した瞬間、
        // 間を置かず最初の一発が出るようにする）。
        press.spray_cooldown = 0.0;
    }

    // ノズルが最深部（depth>=1.0、ポンプストロークを使い切った状態）に
    // 達すると、押し続けていても何も出ない——本物のポンプが「空押し」に
    // なるのと同じ。
    if !held || press.depth >= 1.0 {
        return;
    }

    press.spray_cooldown -= dt;
    if press.spray_cooldown > 0.0 {
        return;
    }
    press.spray_cooldown += SPRAY_INTERVAL;

    fire_spray_droplet(&mut commands, &mut foam_allocator, foam_quality.0.profile(), &aim);
}

/// ノズルの見た目の高さ・向きを、押し込み具合（NozzlePress::depth）と
/// 狙い方向から毎フレーム計算する。UIのゲージを持たない代わりに、この
/// ノズル自体の沈み込みが残量表示を兼ねる。
fn update_player_visual(
    aim: Res<Aim>,
    press: Res<NozzlePress>,
    mut query: Query<&mut Transform, With<SoapDispenser>>,
) {
    if let Ok(mut transform) = query.single_mut() {
        transform.rotation = Quat::from_rotation_z(aim.angle);
        transform.translation = nozzle_anchor(press.depth).extend(0.0);
    }
}

/// ノズルのアンカー点（ここを中心に狙い方向へ回転する）。押し込み具合に
/// 応じて沈み込む。
fn nozzle_anchor(depth: f32) -> Vec2 {
    Vec2::new(0.0, NOZZLE_HEIGHT - depth * NOZZLE_PRESS_STROKE)
}

/// 噴射中に連続して発射される、小さく均一な泡1粒。
fn fire_spray_droplet(
    commands: &mut Commands,
    foam_allocator: &mut FoamSlotAllocator,
    foam_quality_profile: FoamQualityProfile,
    aim: &Aim,
) {
    let base_dir = aim.direction();

    // 蛇口の水流のような自然な散らばり。
    let jitter_angle = rand::rng().random_range(-SPRAY_JITTER..=SPRAY_JITTER);
    let dir = base_dir.rotate(Vec2::from_angle(jitter_angle));

    let velocity = dir * SPRAY_SPEED;
    let spawn_pos = dir * NOZZLE_RADIUS + nozzle_anchor(0.0);

    // Avian2Dの当たり判定を持つBubbleを発射する。見た目（soap.rs）はこの
    // Bubbleエンティティの位置・速度を毎フレーム自動で観測するので、ここから
    // 別途何かを送信する必要はない（doc/soap-model.md 第28節）。
    spawn_bubble(
        commands,
        foam_allocator,
        foam_quality_profile,
        spawn_pos,
        velocity,
        SPRAY_BUBBLE_RADIUS,
        SPRAY_POWER,
    );
}
