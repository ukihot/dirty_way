// リアルタイム・ハンドソープ表現：Foam Aggregate 変形シミュレーション
// （doc/soap-model.md 第27〜28,31,25節）。
//
// 位置・速度はAvian2D（Main World側のbubble.rs）が解いた実物理をそのまま
// 毎フレーム受け取るだけで、ここでは重力や地面接触の物理を自前で積分しない
// （旧版はここで独自に重力積分しており、bubble.rsのAvianボディと二重に物理を
// 解いてしまっていた。重力定数が2箇所に分散する不整合の原因だった＝S-09）。
// このシェーダーが担当するのは「Avianが解いた運動から、泡の塊がどう変形するか」
// というレオロジー（着弾判定・扁平化・広がり）だけ。
//
// 1スロットは「泡粒子」ではなく「1個のFoam Aggregateの見た目の変形状態」を
// 表すため、`Particle`ではなく`FoamInstance`と呼ぶ（doc第31節）。
//
// 2026-07-29追記：3Dトップダウンから2Dサイドビューへ方針転換。X=水平・
// Y=高さ（重力方向）で、Zは廃止した。ハンドソープ筐体を真横から見る
// 構図になったため、着地判定・扁平化の式はvec3をvec2にするだけで
// そのまま成立する（重力・地面ともに元々Y軸基準だったため）。

struct FoamInstance {
    position: vec2<f32>,
    velocity: vec2<f32>,
    scale: vec2<f32>,
    state: u32,
    lifetime: f32,
    base_radius: f32,
    // このスロットへの「今回の」割当を識別する世代番号。スロットは
    // despawn後すぐ別のBubbleへ再利用されうるが、そのままだと前のBubbleの
    // 変形状態（潰れた形）が新しいBubbleにそのまま引き継がれてしまう。
    // generationが前回と食い違っていたら、stateに関わらず新規スポーンとして
    // 初期化し直す（doc/soap-issues.md S-10）。
    generation: u32,
};

struct DriveEntry {
    target_slot: u32,
    position: vec2<f32>,
    velocity: vec2<f32>,
    base_radius: f32,
    generation: u32,
};

struct SimParams {
    dt: f32,
    floor_height: f32,
    impact_factor: f32,
    max_spread: f32,
    drive_count: u32,
};

const STATE_INACTIVE: u32 = 0u;
const STATE_FLYING: u32 = 1u;
const STATE_IMPACT: u32 = 2u;
const STATE_SPREADING: u32 = 3u;
const STATE_RESTING: u32 = 4u;

// 既存ゲームプレイの泡の寿命（BUBBLE_LIFETIME）と揃え、同じ起点（スポーン時刻）で
// カウントする。Avianの泡エンティティがdespawnするのとほぼ同時にGPU側も
// 非アクティブになる（doc第28.2節）。
// 課題S-08のフェード演出はsoap_render.wgsl側で行う（このファイルではなく）。
// 値を変える場合は soap_render.wgsl の同名定数も揃えて変更すること。
const MAX_LIFETIME: f32 = 6.0;

@group(0) @binding(0) var<storage, read_write> foam_instances: array<FoamInstance>;
@group(0) @binding(1) var<storage, read> drive_entries: array<DriveEntry>;
@group(0) @binding(2) var<uniform> sim_params: SimParams;

@compute @workgroup_size(64)
fn simulate(@builtin(global_invocation_id) gid: vec3<u32>) {
    let index = gid.x;
    if (index >= arrayLength(&foam_instances)) {
        return;
    }

    var inst = foam_instances[index];

    // このスロット宛のDrive Entryがあれば、Avianが解いた位置・速度で追従させる。
    // state==INACTIVE、または世代が食い違っている（＝スロットが別のBubbleに
    // 再割当された）場合だけ新規スポーンとして変形状態を初期化する。
    for (var i = 0u; i < sim_params.drive_count; i = i + 1u) {
        if (drive_entries[i].target_slot == index) {
            if (inst.state == STATE_INACTIVE || drive_entries[i].generation != inst.generation) {
                inst.generation = drive_entries[i].generation;
                inst.base_radius = drive_entries[i].base_radius;
                inst.scale = vec2<f32>(1.0, 1.0) * inst.base_radius;
                inst.state = STATE_FLYING;
                inst.lifetime = 0.0;
            }
            inst.position = drive_entries[i].position;
            inst.velocity = drive_entries[i].velocity;
        }
    }

    if (inst.state == STATE_INACTIVE) {
        foam_instances[index] = inst;
        return;
    }

    inst.lifetime = inst.lifetime + sim_params.dt;
    if (inst.lifetime > MAX_LIFETIME) {
        inst.state = STATE_INACTIVE;
        foam_instances[index] = inst;
        return;
    }

    if (inst.state == STATE_FLYING) {
        // 位置・速度はAvianが解いたものをそのまま使う（このシェーダーは
        // 重力を積分しない）。ここでは「床に着いたか」だけ判定する。
        //
        // 課題S-14：ここを`inst.position.y <= sim_params.floor_height`
        // （中心Yが0以下）で判定していたが、Avianの円コライダーは床に
        // めり込まないため、静止時の中心の高さは半径分だけ浮いた
        // `floor_height + base_radius`になる。中心が0以下になることは
        // 実質起こらず、STATE_IMPACTへ一切遷移せずに永遠にFLYING＝
        // 真ん丸のまま跳ね続けていた。自身の半径を考慮して判定する。
        if (inst.position.y <= sim_params.floor_height + inst.base_radius + 0.02) {
            inst.state = STATE_IMPACT;
        }
    } else if (inst.state == STATE_IMPACT) {
        // 着弾速度→扁平化（第8節）。1フレームで即SPREADINGへ遷移する。
        // サイドビューでは画面の縦方向がそのままYなので、この扁平化は
        // 「床にぺたっと潰れて広がる水滴」として画面上にそのまま見える。
        let impact_speed = max(-inst.velocity.y, 0.0);
        let spread = 1.0 + impact_speed * sim_params.impact_factor;
        inst.scale = vec2<f32>(spread, 1.0 / spread) * inst.base_radius;
        inst.state = STATE_SPREADING;
    } else if (inst.state == STATE_SPREADING) {
        // 位置・速度の減衰はAvian側（Restitution/Friction/LinearDamping、
        // bubble.rs）が既に解いている。ここではscale（見た目の広がり）の
        // 補間だけを行う（第9節）。
        let current_spread = inst.scale.x / inst.base_radius;
        let target_spread = min(current_spread + 0.5 * sim_params.dt, sim_params.max_spread);
        let target_scale = vec2<f32>(target_spread, 1.0 / target_spread) * inst.base_radius;
        inst.scale = mix(inst.scale, target_scale, sim_params.dt * 4.0);

        if (length(inst.velocity) < 0.05) {
            inst.state = STATE_RESTING;
        }
    }

    foam_instances[index] = inst;
}
