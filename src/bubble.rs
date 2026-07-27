use avian3d::prelude::*;
use bevy::prelude::*;

use crate::consts::*;
use crate::enemy::{Enemy, Trapped};
use crate::state::{GameState, Score};

/// プレイヤーが発射する泡の当たり判定（ゲームロジック専用、見た目は持たない）。
/// 見た目は `soap.rs` のGPUリアルタイム液体表現が別途担当する。
/// power はダメージ量（チャージが深いほど増える）。
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

pub fn spawn_bubble(commands: &mut Commands, position: Vec3, velocity: Vec3, radius: f32, power: i32) {
    commands.spawn((
        Bubble { power, life: 0.0, hit_enemies: Vec::new() },
        RigidBody::Dynamic,
        Collider::sphere(radius),
        Restitution::new(0.55),
        Friction::new(0.05),
        LinearDamping(0.15),
        AngularDamping(0.4),
        LinearVelocity(velocity),
        Transform::from_translation(position),
    ));
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

            // try_insert/try_despawn: 同じ敵が複数の泡に同時接触していると、
            // 片方の泡が先に倒して despawn を積んだ後にもう片方が Trapped を
            // 挿入しようとすることがある（Commandsの反映は次の同期点まで
            // 遅延するため、このフレーム中はまだ同じEntityが両方から見える）。
            // 素朴な insert/despawn だとその場合に「既に消えたEntity」エラーになる。
            commands.entity(other).try_insert(Trapped { remaining: TRAPPED_LINGER_TIME });

            if bubble.hit_enemies.contains(&other) {
                continue;
            }
            bubble.hit_enemies.push(other);

            enemy.health -= bubble.power;
            if enemy.health <= 0 {
                score.0 += enemy.kind.score_value();
                commands.entity(other).try_despawn();
            }
        }
    }
}
