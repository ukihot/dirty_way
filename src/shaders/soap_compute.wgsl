// リアルタイム・ハンドソープ表現：GPU粒子シミュレーション（doc/soap-model.md 第6〜9,25節）。
// CPUは毎フレーム全粒子を転送しない。ここが粒子の位置・速度・状態の唯一の所有者。

struct Particle {
    position: vec3<f32>,
    velocity: vec3<f32>,
    scale: vec3<f32>,
    state: u32,
    lifetime: f32,
};

struct SpawnRequestGpu {
    target_slot: u32,
    position: vec3<f32>,
    velocity: vec3<f32>,
};

struct SimParams {
    dt: f32,
    gravity: f32,
    table_height: f32,
    impact_factor: f32,
    damping: f32,
    max_spread: f32,
    spawn_count: u32,
};

const STATE_INACTIVE: u32 = 0u;
const STATE_FLYING: u32 = 1u;
const STATE_IMPACT: u32 = 2u;
const STATE_SPREADING: u32 = 3u;
const STATE_RESTING: u32 = 4u;

// 既存ゲームプレイの泡の寿命（BUBBLE_LIFETIME）と揃え、
// リングバッファが一周する前に見た目の泡だまりが無限に蓄積しないようにする。
const MAX_LIFETIME: f32 = 6.0;

@group(0) @binding(0) var<storage, read_write> particles: array<Particle>;
@group(0) @binding(1) var<storage, read> spawn_requests: array<SpawnRequestGpu>;
@group(0) @binding(2) var<uniform> sim_params: SimParams;

@compute @workgroup_size(64)
fn simulate(@builtin(global_invocation_id) gid: vec3<u32>) {
    let index = gid.x;
    if (index >= arrayLength(&particles)) {
        return;
    }

    var p = particles[index];

    // このスロット宛のSpawn Requestがあれば上書きしてFLYINGにする。
    for (var i = 0u; i < sim_params.spawn_count; i = i + 1u) {
        if (spawn_requests[i].target_slot == index) {
            p.position = spawn_requests[i].position;
            p.velocity = spawn_requests[i].velocity;
            p.scale = vec3<f32>(1.0, 1.0, 1.0);
            p.state = STATE_FLYING;
            p.lifetime = 0.0;
        }
    }

    if (p.state == STATE_INACTIVE) {
        particles[index] = p;
        return;
    }

    p.lifetime = p.lifetime + sim_params.dt;
    if (p.lifetime > MAX_LIFETIME) {
        p.state = STATE_INACTIVE;
        particles[index] = p;
        return;
    }

    if (p.state == STATE_FLYING) {
        p.velocity.y = p.velocity.y - sim_params.gravity * sim_params.dt;
        p.position = p.position + p.velocity * sim_params.dt;

        if (p.position.y <= sim_params.table_height) {
            p.position.y = sim_params.table_height;
            p.state = STATE_IMPACT;
        }
    } else if (p.state == STATE_IMPACT) {
        // 着弾速度→扁平化（第8節）。1フレームで即SPREADINGへ遷移する。
        let impact_speed = max(-p.velocity.y, 0.0);
        let spread = 1.0 + impact_speed * sim_params.impact_factor;
        p.scale = vec3<f32>(spread, 1.0 / spread, spread);
        p.velocity = vec3<f32>(p.velocity.x, 0.0, p.velocity.z);
        p.state = STATE_SPREADING;
    } else if (p.state == STATE_SPREADING) {
        // 粘性減衰＋目標スプレッドへの補間（第9節）。
        p.velocity = p.velocity * sim_params.damping;
        let target_spread = min(p.scale.x + 0.5 * sim_params.dt, sim_params.max_spread);
        let target_scale = vec3<f32>(target_spread, 1.0 / target_spread, target_spread);
        p.scale = mix(p.scale, target_scale, sim_params.dt * 4.0);
        p.position = p.position + p.velocity * sim_params.dt;

        if (length(p.velocity) < 0.01) {
            p.state = STATE_RESTING;
        }
    }

    particles[index] = p;
}
