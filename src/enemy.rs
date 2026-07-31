use avian2d::prelude::*;
use bevy::prelude::*;
use bevy_gutzgutz::atlas::GutzAtlasRegistry;
use bevy_gutzgutz::atlas_sprite2d::GutzSpriteAnimation;
use bevy_gutzgutz::lifecycle::in_game;
use bevy_gutzgutz::pacing::GutzRampTimer;
use rand::RngExt;

use crate::consts::*;
use crate::state::{GameState, Health};

/// `assets/character/knight/walk_10`のアトラス名（bevy_gutzgutz atlas
/// 命名規約：namespace + leaf名。`build.rs`が生成する
/// `assets/generated/atlas/manifest.toml`参照）。
const KNIGHT_WALK: &str = "knight/walk";
/// 1フレームあたりの表示秒数（10fps相当）。フレーム数自体は
/// `GutzAtlasRegistry`（マニフェスト由来）から自動で引かれるため、ここでは
/// 持たない（bevy_gutzgutz::atlas_sprite2d::GutzSpriteAnimation参照）。
const KNIGHT_FRAME_DURATION: f32 = 0.1;
/// ナイトの元画像の縦横比（587×707px）。表示サイズを`EnemyKind::radius`
/// から決めるときにこの比率を保つ。
const KNIGHT_ASPECT: f32 = 587.0 / 707.0;
/// 敵の当たり判定（EnemyKind::radius）に対して、見た目のナイトを
/// どれだけ大きく表示するか。円のコライダーそのままの大きさだと
/// キャラクターとして小さすぎるため。
const KNIGHT_VISUAL_SCALE: f32 = 1.8;

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

    pub fn radius(self) -> f32 {
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

/// 敵の出現間隔（開始値・下限）と難易度上昇レートは`GutzRampTimer`（gutzgutz、
/// `bevy_gutzgutz::pacing`）へ委譲する。
/// 「時間経過で間隔が短くなる周期タイマー」 はplayer.rsの`NozzlePress.
/// spray_cooldown`（ランプなし版）と合わせ、この
/// コードベースで2箇所独立に書かれていたパターンだったため抽出した。
#[derive(Resource)]
pub struct EnemySpawnTimer(GutzRampTimer);

impl Default for EnemySpawnTimer {
    fn default() -> Self {
        Self(GutzRampTimer::new(
            ENEMY_SPAWN_INTERVAL_START,
            ENEMY_SPAWN_INTERVAL_MIN,
            DIFFICULTY_RAMP_PER_SEC,
        ))
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
    atlas: Res<GutzAtlasRegistry>,
) {
    if !spawn_timer.0.tick(time.delta()) {
        return;
    }

    let mut rng = rand::rng();
    // サイドビュー：床（Y=0）の左右どちらかの端から出現させる。
    let side = if rng.random_bool(0.5) { 1.0 } else { -1.0 };
    let kind = EnemyKind::random(&mut rng);
    let pos = Vec2::new(side * ENEMY_SPAWN_RADIUS, kind.radius());

    // 見た目はグツグツ（bevy_gutzgutz）のアトラスから読み込んだ
    // `assets/character/knight`のナイトに統一する（旧：種類ごとの色付き円）。
    // 当たり判定はこれまで通りEnemyKind::radiusの円のまま——見た目と
    // 判定サイズを一致させる必要は無い（doc：GutzAtlasFrameは描画方式を
    // 問わない生データを返すだけで、Sprite/Meshどちらで使うかは呼び出し側次第）。
    let Some(frame) = atlas.frame(KNIGHT_WALK, 0) else {
        // アトラス未ロード・manifest読み込み失敗時は見た目だけ諦める
        // （ゲームロジックには影響しない。当たり判定・移動・撃破は機能する）。
        commands.spawn((
            Enemy { kind, health: kind.max_health() },
            RigidBody::Kinematic,
            Collider::circle(kind.radius()),
            CollisionLayers::new(
                GameLayer::Enemy,
                [GameLayer::Floor, GameLayer::Bubble, GameLayer::LandedBubble],
            ),
            Transform::from_translation(pos.extend(0.0)),
        ));
        return;
    };

    let height = kind.radius() * 2.0 * KNIGHT_VISUAL_SCALE;
    let width = height * KNIGHT_ASPECT;

    commands.spawn((
        Enemy { kind, health: kind.max_health() },
        GutzSpriteAnimation::new(KNIGHT_WALK, KNIGHT_FRAME_DURATION),
        RigidBody::Kinematic,
        Collider::circle(kind.radius()),
        // 課題S-17/S-24：Bubble（飛行中）・LandedBubble（着地済み）・床とは
        // 衝突させ、敵同士は衝突させない（consts::GameLayer参照。密集しても
        // 押し合わない）。
        CollisionLayers::new(
            GameLayer::Enemy,
            [GameLayer::Floor, GameLayer::Bubble, GameLayer::LandedBubble],
        ),
        Sprite {
            image: frame.image,
            rect: Some(frame.pixel_rect),
            custom_size: Some(Vec2::new(width, height)),
            color: kind.color(),
            // 右端からスポーンした（中心へ向かって左へ進む）ナイトは反転して
            // 進行方向を向かせる。元画像がどちらを向いているかは実機で
            // 確認して必要なら符号を反転する。
            flip_x: side > 0.0,
            ..default()
        },
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
