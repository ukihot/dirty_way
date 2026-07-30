use std::f32::consts::TAU;

use avian2d::prelude::*;
use bevy::prelude::*;
use bevy_gutzgutz::lifecycle::in_game;
use rand::RngExt;

use crate::consts::*;
use crate::enemy::{Enemy, Trapped};
use crate::quality::FoamQualityProfile;
use crate::scene::Floor;
use crate::state::{GameState, Score};

/// 泡が何の上に着地したか（課題S-24、2026-07-29）。まだ飛行中はFlying。
/// soap.rs/soap_compute.wgsl側で見た目の扁平化具合を変えるのに使う——
/// 床への直接着地はしっかり潰れて広がるが、既存の泡だまりの上への着地は
/// 嵩をあまり失わず、メタボール融合で高さが積み上がっていくように見せる。
///
/// これはGPU側の「着地したかどうか」の判定信号も兼ねる。以前はGPU側
/// （soap_compute.wgsl）が`position.y <= floor_height + base_radius`という
/// 床基準の絶対座標だけで着地を判定していたが、泡だまりの上に着地した泡は
/// 床よりずっと高い位置で静止するため、この条件が永久に成立せず、
/// 丸いまま固まってしまっていた。着地の判定そのものをMain World側
/// （このenum）の権威に一本化し、Render World側は言われた通りに従うだけに
/// する（doc：「物理判定はAvian2D、見た目はGPU」という責務分担と同じ発想）。
#[derive(Clone, Copy, PartialEq, Eq, Default)]
pub enum LandingSurface {
    #[default]
    Flying,
    Floor,
    Pile,
}

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
    pub landing: LandingSurface,
    /// 課題S-26：床・泡だまりに触れていて、かつ速度がBUBBLE_SETTLE_SPEED
    /// を下回っている状態が続いている秒数。BUBBLE_SETTLE_DURATIONに達したら
    /// 本当に静止したとみなしてStatic化する（settle_landed_bubbles参照）。
    settle_timer: f32,
    /// 課題S-32（2026-07-29）：着地時、クラスター内の塊が一斉に同じ倍率で
    /// 「その場でぺたっと」潰れるだけで、塊同士の位置関係（cluster_offsets）
    /// は変わらなかった。勢いよく叩きつけられた液体は実際には飛び散る
    /// はずなので、着地の瞬間の速度に応じてクラスターのオフセット自体を
    /// 外側へ広げる倍率（soap.rs::extract_foam_aggregates参照）。0.0=
    /// まだ着地の瞬間を迎えていない（初期値）。
    pub impact_scatter: f32,
    /// `impact_scatter`を一度だけ確定させるためのフラグ。着地後、滑って
    /// 静止するまでの間に速度がどんどん落ちていくので、「静止直前の遅い
    /// 速度」ではなく「最初に接触した瞬間の速度」を捉えたい。
    impact_captured: bool,
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
    /// 常にFOAM_CLUSTER_MAX個確保するが、実際にGPUへ送る（＝見た目に
    /// 使う）のは先頭`active_count`個だけ（課題S-30）。
    pub slots: [u32; FOAM_CLUSTER_MAX],
    /// 各スロットの、Bubble中心からの相対オフセット（半径1.0基準の単位
    /// ベクトル）。`soap.rs`側で実際の`bubble.radius`を掛けて使う。
    /// 複数の塊を少しずつずらして重ねて配置することで、単一Instanceでは
    /// 出せない「寄り集まって融合した液体」の見た目を作る（consts::
    /// FOAM_CLUSTER_MAX参照）。
    pub offsets: [Vec2; FOAM_CLUSTER_MAX],
    /// 各塊の大きさの倍率（1.0基準）。塊ごとに大きさをばらつかせることで、
    /// 同じ大きさの塊が並ぶ単調な見た目を避ける（課題S-30）。
    pub size_scales: [f32; FOAM_CLUSTER_MAX],
    /// 今回のBubbleで実際に使う塊の個数（FOAM_CLUSTER_MIN〜MAX、課題S-30）。
    pub active_count: u32,
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

/// FOAM_CLUSTER_MIN〜MAX個の塊を、Bubble中心の周りに少しずつ重なるように
/// 配置する（1個は中心、残りは円状に並べる）。
///
/// 課題S-30（2026-07-29）：個数を毎回FOAM_CLUSTER_MAX固定・角度も均等割り
/// （乱数は全体の回転だけ）にしていたところ、どのBubbleも同じ「花びら」型に
/// 見えてしまい、「毎回同じでありきたり」という指摘を受けた（実機確認）。
/// 実際のハンドソープの泡は5〜9個くらいの不揃いな塊が寄り集まっている
/// はずなので、個数・各塊の角度（均等割りからのブレ）・中心からの距離・
/// 大きさをすべてランダム化し、毎回違う不揃いな「もくもく」とした外形に
/// する。
/// 戻り値は`(offsets, size_scales, active_count)`。`active_count`より
/// 後ろの要素は未使用（呼び出し側がFOAM_CLUSTER_MAX個の配列を要求する
/// ためダミー値で埋めるだけ）。
fn cluster_offsets() -> ([Vec2; FOAM_CLUSTER_MAX], [f32; FOAM_CLUSTER_MAX], u32) {
    let mut rng = rand::rng();
    let mut offsets = [Vec2::ZERO; FOAM_CLUSTER_MAX];
    let mut size_scales = [1.0f32; FOAM_CLUSTER_MAX];
    let active_count = rng.random_range(FOAM_CLUSTER_MIN..=FOAM_CLUSTER_MAX as u32);

    // 課題S-31（2026-07-29）：最初のランダム化幅（距離0.22〜0.4、大きさ
    // 0.55〜1.05）は控えめすぎて、実機で見ると「思ったより不揃いになって
    // いない」という指摘を受けた。飛行中の速度方向ストリーク伸長（課題
    // S-28）が塊のシルエットを覆い隠してしまう面もあるため、ブレの幅
    // 自体をもっと大胆に広げる。

    // 中心の塊（常に存在）。ここも大きさをばらつかせる。
    size_scales[0] = rng.random_range(0.8..1.3);

    let ring_count = active_count - 1;
    let rotation = rng.random_range(0.0..TAU);
    for i in 1..active_count as usize {
        // 均等割りの角度から±65%の範囲でランダムにずらし、正多角形的な
        // 規則性を大きく崩す。
        let ideal_angle = rotation + (i - 1) as f32 / ring_count as f32 * TAU;
        let jitter = rng.random_range(-0.65..0.65) * (TAU / ring_count as f32);
        let angle = ideal_angle + jitter;
        // 中心からの距離を大きくばらつかせる（近くにぎゅっと寄る塊も、
        // 離れてぽつんと飛び出す塊もある方が「もくもく」して見える）。
        let dist = rng.random_range(0.12..0.6);
        offsets[i] = Vec2::new(angle.cos(), angle.sin()) * dist;
        size_scales[i] = rng.random_range(0.4..1.35);
    }

    (offsets, size_scales, active_count)
}

impl FoamSlotAllocator {
    /// `quality.max_aggregates`（GPU Instance Poolの物理容量512とは独立な、
    /// 品質段階ごとの同時表示上限）に達していたらスロットを払い出さない
    /// （doc/soap-issues.md
    /// S-11a）。Poolの容量そのものは常に512のまま変えない。
    ///
    /// `pub(crate)`：Bubble以外（player.rsのチャージ中プレビュー）からも
    /// 同じスロットプールを間借りするため、クレート内には公開する。
    pub(crate) fn allocate(&mut self, quality: FoamQualityProfile) -> Option<FoamGpuBinding> {
        if self.active_bindings >= quality.max_aggregates {
            return None;
        }
        // 課題S-30：実際に使うのは`active_count`（FOAM_CLUSTER_MIN〜MAX）
        // 個だけだが、可変長のスロット確保は複雑になるため、常に
        // FOAM_CLUSTER_MAX個を確保する（使わない分は単に駆動されない
        // ままになるだけで、GPU側の見た目には一切現れない）。
        if self.free_slots.len() < FOAM_CLUSTER_MAX {
            return None;
        }

        let mut slots = [0u32; FOAM_CLUSTER_MAX];
        for slot in &mut slots {
            *slot = self.free_slots.pop().expect("checked len above");
        }
        let generation = self.next_generation;
        self.next_generation = self.next_generation.wrapping_add(1);
        self.active_bindings += 1;
        let (offsets, size_scales, active_count) = cluster_offsets();
        Some(FoamGpuBinding { slots, offsets, size_scales, active_count, generation })
    }

    pub fn release(&mut self, binding: &FoamGpuBinding) {
        self.free_slots.extend_from_slice(&binding.slots);
        self.active_bindings = self.active_bindings.saturating_sub(1);
    }
}

/// 着地済み（＝次のBubbleが乗れる「地形」になった）の泡につける目印。
/// `settle_landed_bubbles`は毎フレーム全Bubbleを見なくて済むよう、まだ
/// 着地していないもの（このマーカーが無いもの）だけを見る（課題S-27：
/// 着地後もBubble自体はDynamicのままで、Staticにはしない）。
#[derive(Component)]
struct Landed;

pub struct BubblePlugin;

impl Plugin for BubblePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<FoamSlotAllocator>().add_systems(
            Update,
            (
                tick_bubble_lifetime,
                settle_landed_bubbles,
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
    position: Vec2,
    velocity: Vec2,
    radius: f32,
    power: i32,
) {
    let mut entity = commands.spawn((
        Bubble {
            power,
            life: 0.0,
            radius,
            hit_enemies: Vec::new(),
            landing: LandingSurface::Flying,
            settle_timer: 0.0,
            impact_scatter: 0.0,
            impact_captured: false,
        },
        RigidBody::Dynamic,
        Collider::circle(radius),
        // 課題S-18：飛行中はドロドロした重い液体でも「重力だけで山なりに飛ぶ」
        // のが自然で、LinearDampingを飛行中にもかけていると空気抵抗のような
        // 不自然なもったり感が出て、「滑りの悪い雪玉」のように見えてしまって
        // いた（実機確認）。着地は`settle_landed_bubbles`が検知した瞬間に
        // RigidBody::Staticへ切り替えて完全停止させる（S-15の「転がらず
        // 即座に静止する」という要求は、摩擦・減衰のチューニングではなく
        // Static化そのもので満たす）。そのため、ここでのFriction/Damping系は
        // 着地からStatic化までのごく短い1フレームの保険程度の意味しか無い。
        Restitution::new(0.02),
        Friction::new(0.7),
        LinearDamping(0.02),
        AngularDamping(0.2),
        // 球コライダーは、摩擦があっても一度「滑り」から「転がり」に転じると
        // 追加の減速要因が無くなり、AngularDamping頼みでずるずる長く転がって
        // しまう。回転そのものを禁止することで、着地後の余計な転がりが
        // 発生しない。見た目（soap.rs）はTransformの回転を一切参照しない
        // ので、副作用はない。
        LockedAxes::ROTATION_LOCKED,
        // 課題S-23：飛行中のBubble同士は衝突させない。SPRAY_INTERVAL間隔の
        // 連続噴射では直前に撃った粒と至近距離で常に接触することになり、
        // 衝突させると低反発・高摩擦・回転ロックの組み合わせで互いに
        // 引っかかり合い、鎖のように連結して重力に逆らって空中に留まって
        // しまっていた（実機確認）。
        // 課題S-24：ただし既に着地した泡だまり（LandedBubble）とは衝突
        // させる——そうしないと新しい泡が既存の山を素通りして必ず床まで
        // 落ちてしまい、「積み上がる」ことが一切できなくなる。これにより
        // 新しい泡は他の飛行中の泡はすり抜けつつ、既存の泡だまりの上には
        // 正しく乗って積み上がっていく。
        // 課題S-27（2026-07-29）：Enemyはここに含めない。敵との接触判定は
        // `bubble_enemy_interaction`が距離ベースで別途行う（コメント参照）。
        // Avianの物理衝突に任せると、KinematicのEnemyがDynamicのBubbleを
        // 押し出してしまう（Kinematicは常に他を押しのける側になるのが
        // 物理エンジンの標準挙動）。以前はBubbleを`RigidBody::Static`化
        // することでこれを回避していたが、Staticは重力を一切受けないため、
        // 積み上がった泡の下側だけが寿命で消えても上側が宙に浮いたまま
        // 残ってしまうという致命的な副作用があった（実機確認）。Bubbleは
        // 常にDynamicのまま——支えを失えば自然に落下し直す——という原則を
        // 守るため、Enemyとの接触検知だけを物理から切り離す。
        CollisionLayers::new(GameLayer::Bubble, [GameLayer::Floor, GameLayer::LandedBubble]),
        LinearVelocity(velocity),
        Transform::from_translation(position.extend(0.0)),
    ));
    // 品質上限に達している、またはプールが尽きていたら見た目（FoamGpuBinding）
    // だけ諦める。ゲームロジックには影響しない。
    if let Some(binding) = allocator.allocate(quality) {
        entity.insert(binding);
    }
}

/// 課題S-27：床・泡だまりに触れた瞬間ではなく、本当に静止する（BUBBLE_
/// SETTLE_SPEED未満の速度がBUBBLE_SETTLE_DURATION秒続く）まで待ってから
/// 「着地済み」とみなす。触れた瞬間に即座に確定していた頃は、飛んで来た
/// 軌跡の形がそのまま「蛇」のように固まって残ってしまっていた。液体らしく、
/// 実際に落ち着くまでは重力・摩擦に従って山の斜面を滑らせ続けることで、
/// 新しい泡が既存の山の低いところへ自然に流れ込み、「山が更新されていく」
/// ように見せる（consts::BUBBLE_SETTLE_SPEEDのコメント参照）。
///
/// 着地後もBubbleは`RigidBody::Dynamic`のまま変えない（課題S-27）——
/// CollisionLayersのmembershipsだけをBubble→LandedBubbleへ切り替え、
/// 以後「次のBubbleが乗れる地形」として振る舞うようにする（課題S-24）。
/// Dynamicのまま・重力を受け続けるままにしておくことで、支えにしていた
/// 別のBubbleが寿命で消えたときに、その上の泡が自然に落下し直せる
/// （以前`RigidBody::Static`にしていた頃は、重力を一切受けないせいで
/// 下が消えても上が宙に浮いたまま残ってしまっていた）。
fn settle_landed_bubbles(
    mut commands: Commands,
    time: Res<Time>,
    collisions: Collisions,
    floor: Query<Entity, With<Floor>>,
    landed: Query<Entity, With<Landed>>,
    mut bubbles: Query<(Entity, &mut Bubble, &LinearVelocity), Without<Landed>>,
) {
    let Ok(floor_entity) = floor.single() else { return };
    let dt = time.delta_secs();
    for (entity, mut bubble, velocity) in &mut bubbles {
        // 課題S-25：スポーン直後は着地判定そのものを無視する（コメント参照）。
        // 泡だまりがノズル付近まで育つと、生まれたばかりの泡がまだ動いて
        // いないその場で既存の山と接触してしまい、飛び出す前に固まって
        // 「かまくら」状に積み上がってしまっていた。
        if bubble.life < BUBBLE_LANDING_GRACE_PERIOD {
            continue;
        }

        let landed_on_pile =
            collisions.entities_colliding_with(entity).any(|other| landed.contains(other));
        let touched_floor =
            collisions.entities_colliding_with(entity).any(|other| other == floor_entity);
        let touching_ground = landed_on_pile || touched_floor;

        // 課題S-32：クラスターの飛び散り具合（impact_scatter）は、着地して
        // 滑りながらどんどん減速していく途中の速度ではなく、最初に接触
        // した瞬間の速度で決めたい。着地後は既にDynamicのまま滑っている
        // ことがあるので、`touching_ground`になった最初のフレームだけ捉える。
        if touching_ground && !bubble.impact_captured {
            bubble.impact_captured = true;
            bubble.impact_scatter =
                (velocity.0.length() * IMPACT_SCATTER_FACTOR).min(IMPACT_SCATTER_MAX);
        }

        if touching_ground && velocity.0.length() < BUBBLE_SETTLE_SPEED {
            bubble.settle_timer += dt;
        } else {
            bubble.settle_timer = 0.0;
        }
        if bubble.settle_timer < BUBBLE_SETTLE_DURATION {
            continue;
        }

        bubble.landing = if landed_on_pile { LandingSurface::Pile } else { LandingSurface::Floor };
        commands.entity(entity).insert((
            Landed,
            CollisionLayers::new(GameLayer::LandedBubble, [GameLayer::Floor, GameLayer::Bubble]),
        ));
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
///
/// 課題S-27：Avianの物理衝突（`Collisions`）ではなく、距離ベースの手動判定
/// にしている。物理衝突に任せると、KinematicのEnemyがDynamicのBubbleを
/// 押し出してしまう（Kinematicは常に他を押しのける側になるのが物理
/// エンジンの標準挙動で、これを防ぐにはBubbleをStatic/Kinematic化する
/// しかなく、Staticには「支えが消えても宙に浮いたまま」という別の致命的
/// 問題があった）。接触検知だけを物理から切り離すことで、Bubbleは常に
/// Dynamicのまま（＝常に重力に従う）にしつつ、Enemyから押されることも
/// 無くなる。
fn bubble_enemy_interaction(
    mut commands: Commands,
    mut bubbles: Query<(&Transform, &mut Bubble)>,
    mut enemies: Query<(Entity, &Transform, &mut Enemy)>,
    mut score: ResMut<Score>,
) {
    for (bubble_transform, mut bubble) in &mut bubbles {
        let bubble_pos = bubble_transform.translation.xy();
        for (other, enemy_transform, mut enemy) in &mut enemies {
            let dist = bubble_pos.distance(enemy_transform.translation.xy());
            if dist > bubble.radius + enemy.kind.radius() {
                continue;
            }

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
