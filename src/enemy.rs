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
            // 1発ごとにFoamPuffを2個ずつ増やす。最低でも数発は必要にして、
            // 「付く → ふくらむ → 完全に包まれる」という視覚的な成長を
            // プレイヤーが読めるようにする。
            EnemyKind::Dust => 3,
            EnemyKind::Oil => 6,
            EnemyKind::Mud => 9,
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
    /// 値は体力というより「完全に泡で包むまでに必要な残りの噴射回数」。
    /// 物理的な圧殺ではなく、泡の量が敵を覆うことで無力化するゲーム性に
    /// 置き換える。見た目の蓄積は`FoamCoat`が担当する。
    pub health: i32,
}

/// 敵の周囲に生成済みの、見た目専用の泡塊数。泡塊は剛体ではないため、
/// 敵を何体出してもAvianの衝突ペアを増やさない。
#[derive(Component, Default)]
pub struct FoamCoat {
    puff_count: u8,
}

/// `FoamCoat`を構成する、敵へ追従する1個の泡塊。個別Spriteを少しずつ
/// 呼吸するように揺らすことで、シェーダによる大規模な密度場を使わずに
/// ファンタジー調の「ふわふわ・もこもこ」を出す。
#[derive(Component)]
struct FoamPuff {
    target: Entity,
    offset: Vec2,
    base_size: f32,
    phase: f32,
}

/// 命中した敵へ、物理を持たない泡Spriteを追加する。`FoamCoat`が上限を
/// 持つので、長時間当て続けても描画Entity数は敵1体あたり一定である。
pub fn add_foam_puffs(commands: &mut Commands, target: Entity, radius: f32, coat: &mut FoamCoat) {
    let mut rng = rand::rng();
    for _ in 0..FOAM_PUFFS_PER_HIT {
        if coat.puff_count >= FOAM_PUFFS_MAX_PER_ENEMY {
            return;
        }

        // 先に付いた泡ほど中心寄り、後から付いた泡ほど外へ膨らむ。規則的な
        // 円ではなく不揃いな輪郭にすることで、雲・綿菓子のような量感を作る。
        let fullness = coat.puff_count as f32 / FOAM_PUFFS_MAX_PER_ENEMY as f32;
        let angle = rng.random_range(0.0..std::f32::consts::TAU);
        let distance = radius * rng.random_range(0.12..(0.55 + fullness * 0.8));
        let base_size = radius * rng.random_range(0.7..1.25) * (1.0 + fullness * 0.3);
        let alpha = rng.random_range(0.72..0.92);
        let tint = rng.random_range(0.92..1.0);

        commands.spawn((
            FoamPuff {
                target,
                offset: Vec2::new(angle.cos(), angle.sin()) * distance,
                base_size,
                phase: rng.random_range(0.0..std::f32::consts::TAU),
            },
            Sprite {
                color: Color::srgba(tint, tint + (1.0 - tint) * 0.5, 1.0, alpha),
                custom_size: Some(Vec2::splat(base_size)),
                ..default()
            },
            // soapの全画面密度場（Z=0.5）より前に出し、敵を包む輪郭を確実に
            // 読ませる。剛体もColliderも付けない。
            Transform::from_xyz(0.0, 0.0, 1.0 + coat.puff_count as f32 * 0.001),
        ));
        coat.puff_count += 1;
    }
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
            (spawn_enemies, tick_trapped, move_enemies, animate_foam_puffs, enemy_reach_center)
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
            FoamCoat::default(),
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
        FoamCoat::default(),
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

/// 泡塊は敵へ追従するだけで、物理には一切参加しない。サイズと位置を位相ごとに
/// 少しだけ変えると、複数の円Spriteでも生きた泡が呼吸しているように見える。
fn animate_foam_puffs(
    time: Res<Time>,
    mut commands: Commands,
    // FoamPuff自身はEnemyを持たないため、`Without<FoamPuff>`で追従先と
    // 更新対象が絶対に同一EntityにならないことをECSへ明示する。
    targets: Query<&Transform, (With<Enemy>, Without<FoamPuff>)>,
    mut puffs: Query<(Entity, &FoamPuff, &mut Transform)>,
) {
    let elapsed = time.elapsed_secs();
    for (entity, puff, mut transform) in &mut puffs {
        let Ok(target) = targets.get(puff.target) else {
            commands.entity(entity).despawn();
            continue;
        };

        let breath = 1.0 + (elapsed * 2.1 + puff.phase).sin() * 0.09;
        let drift =
            Vec2::new((elapsed * 1.7 + puff.phase).cos(), (elapsed * 2.4 + puff.phase * 1.7).sin())
                * puff.base_size
                * 0.08;
        transform.translation =
            target.translation + (puff.offset + drift).extend(transform.translation.z);
        transform.scale = Vec3::splat(breath);
    }
}

fn enemy_reach_center(
    mut commands: Commands,
    mut health: ResMut<Health>,
    mut next_state: ResMut<NextState<GameState>>,
    enemies: Query<(Entity, &Enemy, &Transform)>,
    puffs: Query<(Entity, &FoamPuff)>,
) {
    for (entity, enemy, transform) in &enemies {
        let dist = transform.translation.x.abs();
        if dist <= CENTER_KILL_RADIUS {
            // bubble.rs 側の撃破処理と同一フレームで競合しうるため try_despawn にする。
            commands.entity(entity).try_despawn();
            // GameOverへ入るとInGameの更新システムは停止する。その前に追従先を
            // 失った泡塊も消して、タイトル／リザルト画面に残らないようにする。
            for (puff_entity, puff) in &puffs {
                if puff.target == entity {
                    commands.entity(puff_entity).try_despawn();
                }
            }
            health.0 -= enemy.kind.contact_damage();
            if health.0 <= 0 {
                health.0 = 0;
                next_state.set(GameState::GameOver);
            }
        }
    }
}
