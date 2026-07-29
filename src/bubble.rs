use std::f32::consts::TAU;

use avian3d::prelude::*;
use bevy::prelude::*;
use bevy_gutzgutz::lifecycle::in_game;
use rand::RngExt;

use crate::consts::*;
use crate::enemy::{Enemy, Trapped};
use crate::quality::FoamQualityProfile;
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
    /// このバブルの半径。`soap.rs` がFoam Aggregateの基準サイズとして読み出す
    /// （doc/soap-model.md 第27.3節）。
    pub radius: f32,
    /// このバブルが既にダメージを与えた敵（同じ相手に毎フレーム連続ダメージしないため）。
    hit_enemies: Vec<Entity>,
}

/// このBubbleエンティティが対応するGPU側Foam Instanceのスロット（doc第31節）。
/// 「EntityとSlotの対応そのもの」はEntity自身に持たせ（Component）、
/// 「空きSlotの管理」だけをResource（`FoamSlotAllocator`）に持たせる。
///
/// `generation`は、このスロットへの「今回の」割当を識別する番号。スロットは
/// despawn後すぐ別のBubbleへ再利用されうるが、GPU側（soap_compute.wgsl）の
/// 変形状態（scale等）はそのフレームまでの古いBubbleのものが残っている。
/// generationが前回と食い違っていたら、GPU側は現在の状態を無視して
/// 新規スポーンとして初期化し直す。これが無いと、直前のBubbleが扁平に
/// 潰れた状態のまま新しいBubbleの見た目として使われてしまう
/// （特にリスタート直後に即発射すると起きやすい。doc/soap-issues.md S-10）。
#[derive(Component, Clone, Copy)]
pub struct FoamGpuBinding {
    pub slots: [u32; FOAM_SUB_INSTANCES],
    /// 各スロットの、Bubble中心からの相対オフセット（半径1.
    /// 0基準の単位ベクトル。 XZ平面内、Y=0）。`soap.rs`側で実際の`bubble.
    /// radius`を掛けて使う。
    /// 複数の塊を少しずつずらして重ねて配置することで、単一Instanceでは
    /// 出せない「寄り集まって融合した液体」の見た目を作る（consts::
    /// FOAM_SUB_INSTANCES参照）。
    pub offsets: [Vec3; FOAM_SUB_INSTANCES],
    pub generation: u32,
}

/// GPU Foam Instance Poolの空きスロット管理。Main World側（このBubble
/// エンティティ自身）が対応表を持つので、
/// こちらは「今どのスロットが空いているか」
/// だけを覚えていればよい（doc第31節）。
#[derive(Resource)]
pub struct FoamSlotAllocator {
    free_slots: Vec<u32>,
    next_generation: u32,
    /// 現在FoamGpuBindingを持っているBubbleの数（＝スロットではなく
    /// Aggregateの数）。1 Bubbleが`FOAM_SUB_INSTANCES`個のスロットを
    /// まとめて使うようになったため、空きスロット数の逆算では
    /// 「Bubble何個分か」が分からなくなった。`quality.max_aggregates`は
    /// 「同時に見た目を持てるBubbleの上限」という意味なので、素直に
    /// Bubble数そのものを数える。
    active_bindings: u32,
}

impl Default for FoamSlotAllocator {
    fn default() -> Self {
        Self {
            free_slots: (0..FOAM_INSTANCE_POOL_SIZE).collect(),
            next_generation: 1,
            active_bindings: 0,
        }
    }
}

/// `FOAM_SUB_INSTANCES`個の塊を、Bubble中心の周りに少しずつ重なるように
/// 配置する（1個は中心、残りは円状に並べる）。乱数で回転させることで、
/// 発射のたびにクラスターの向きが変わり、毎回同じ形に見えるのを避ける
/// （doc/soap-issues.md S-01/S-02と同じ「毎回同じ形問題」への対策）。
fn cluster_offsets() -> [Vec3; FOAM_SUB_INSTANCES] {
    let mut offsets = [Vec3::ZERO; FOAM_SUB_INSTANCES];
    let rotation = rand::rng().random_range(0.0..TAU);
    let ring_count = FOAM_SUB_INSTANCES - 1;
    for (i, offset) in offsets.iter_mut().skip(1).enumerate() {
        let angle = rotation + i as f32 / ring_count as f32 * TAU;
        *offset = Vec3::new(angle.cos(), 0.0, angle.sin()) * 0.45;
    }
    offsets
}

impl FoamSlotAllocator {
    /// `quality.max_aggregates`（GPU Instance Poolの物理容量512とは独立な、
    /// 品質段階ごとの同時表示上限）に達していたらスロットを払い出さない
    /// （doc/soap-issues.md
    /// S-11a）。Poolの容量そのものは常に512のまま変えない。
    fn allocate(&mut self, quality: FoamQualityProfile) -> Option<FoamGpuBinding> {
        if self.active_bindings >= quality.max_aggregates {
            return None;
        }
        if self.free_slots.len() < FOAM_SUB_INSTANCES {
            return None;
        }

        let mut slots = [0u32; FOAM_SUB_INSTANCES];
        for slot in &mut slots {
            *slot = self.free_slots.pop().expect("checked len above");
        }
        let generation = self.next_generation;
        self.next_generation = self.next_generation.wrapping_add(1);
        self.active_bindings += 1;
        Some(FoamGpuBinding { slots, offsets: cluster_offsets(), generation })
    }

    pub fn release(&mut self, binding: &FoamGpuBinding) {
        self.free_slots.extend_from_slice(&binding.slots);
        self.active_bindings = self.active_bindings.saturating_sub(1);
    }
}

pub struct BubblePlugin;

impl Plugin for BubblePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<FoamSlotAllocator>().add_systems(
            Update,
            (
                tick_bubble_lifetime,
                despawn_out_of_bounds,
                bubble_enemy_interaction,
            )
                .run_if(in_game::<GameState>()),
        );
    }
}

pub fn spawn_bubble(
    commands: &mut Commands,
    allocator: &mut FoamSlotAllocator,
    quality: FoamQualityProfile,
    position: Vec3,
    velocity: Vec3,
    radius: f32,
    power: i32,
) {
    let mut entity = commands.spawn((
        Bubble { power, life: 0.0, radius, hit_enemies: Vec::new() },
        RigidBody::Dynamic,
        Collider::sphere(radius),
        // 泡の見た目（soap.rs）は「跳ねるボール」ではなく「着地して潰れる
        // 泡の塊」として描画したい。Restitution=0.12/Friction=0.05という
        // 旧設定は、実際に動かすと「ごつんと弾んで転がる硬いボール」に感じ
        // られてしまっていた（doc/soap-issues.md S-15）。ハンドソープらしい
        // 「その場でぺたっと潰れて止まる」感触にするため、反発をほぼ0にし、
        // 摩擦・減衰を強めて着地後ほぼ即座に静止させる。
        Restitution::new(0.02),
        Friction::new(0.7),
        LinearDamping(0.3),
        AngularDamping(0.8),
        // 球コライダーは、摩擦があっても一度「滑り」から「転がり」に転じると
        // 追加の減速要因が無くなり、AngularDamping頼みでずるずる長く転がって
        // しまう（doc/soap-issues.md 2026-07-28追記：MPM泥ソルバのように
        // 「着地したら飛び散ることはあっても転がることは絶対にない」挙動が
        // 欲しいのに、回転が自由だとFriction=0.7でも「転がる球」に見えてしまう
        // 問題）。回転そのものを禁止することで、接地時のFrictionが常に
        // 並進速度を直接削る「滑って止まる」挙動になり、転がりが原理的に
        // 発生しなくなる。見た目（soap.rs）はTransformの回転を一切参照しない
        // ので、副作用はない。
        LockedAxes::ROTATION_LOCKED,
        LinearVelocity(velocity),
        Transform::from_translation(position),
    ));
    // 品質上限に達している、またはプールが尽きていたら見た目（FoamGpuBinding）
    // だけ諦める。ゲームロジックには影響しない。
    if let Some(binding) = allocator.allocate(quality) {
        entity.insert(binding);
    }
}

fn tick_bubble_lifetime(
    time: Res<Time>,
    mut commands: Commands,
    mut allocator: ResMut<FoamSlotAllocator>,
    mut bubbles: Query<(Entity, &mut Bubble, Option<&FoamGpuBinding>)>,
) {
    for (entity, mut bubble, binding) in &mut bubbles {
        bubble.life += time.delta_secs();
        if bubble.life > BUBBLE_LIFETIME {
            if let Some(binding) = binding {
                allocator.release(binding);
            }
            commands.entity(entity).despawn();
        }
    }
}

fn despawn_out_of_bounds(
    mut commands: Commands,
    mut allocator: ResMut<FoamSlotAllocator>,
    bubbles: Query<(Entity, &Transform, Option<&FoamGpuBinding>), With<Bubble>>,
) {
    for (entity, transform, binding) in &bubbles {
        if transform.translation.y < -5.0
            || transform.translation.length() > BUBBLE_DESPAWN_DISTANCE
        {
            if let Some(binding) = binding {
                allocator.release(binding);
            }
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
