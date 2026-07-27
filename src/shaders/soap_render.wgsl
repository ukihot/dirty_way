// リアルタイム・ハンドソープ表現：Metaballレイマーチ描画（doc/soap-model.md 第10〜12,24,25節）。
// 画面全体を覆う1枚の三角形を描き、フラグメントシェーダー側でカメラレイを
// 再構築してParticle Bufferを直接評価する（Phase 1: 3D Density Gridは使わない）。
//
// Phase 1では深度テストを行わない（詳細はsoap.rsの depth_stencil コメントを参照）。

struct Particle {
    position: vec3<f32>,
    velocity: vec3<f32>,
    scale: vec3<f32>,
    state: u32,
    lifetime: f32,
};

struct SoapView {
    clip_from_world: mat4x4<f32>,
    world_from_clip: mat4x4<f32>,
    camera_world_position: vec3<f32>,
};

const STATE_INACTIVE: u32 = 0u;

@group(0) @binding(0) var<storage, read> particles: array<Particle>;
@group(0) @binding(1) var<uniform> view: SoapView;

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) ndc: vec2<f32>,
};

@vertex
fn vertex(@builtin(vertex_index) vertex_index: u32) -> VertexOutput {
    // 標準的な「フルスクリーン三角形」トリック：頂点バッファなしで
    // クリップ空間全体を覆う1枚の三角形を3頂点だけで作る。
    let uv = vec2<f32>(f32((vertex_index << 1u) & 2u), f32(vertex_index & 2u));
    let ndc = uv * 2.0 - 1.0;

    var out: VertexOutput;
    out.clip_position = vec4<f32>(ndc, 0.0, 1.0);
    out.ndc = ndc;
    return out;
}

fn particle_density(p: vec3<f32>, particle: Particle) -> f32 {
    // 楕円体化：スケールで正規化してから単位球のカーネルを評価する（第10節）。
    let local = (p - particle.position) / particle.scale;
    let d = dot(local, local);
    return max(0.0, 1.0 - d);
}

fn scene_density(p: vec3<f32>) -> f32 {
    var sum = 0.0;
    let count = arrayLength(&particles);
    for (var i = 0u; i < count; i = i + 1u) {
        let particle = particles[i];
        if (particle.state == STATE_INACTIVE) {
            continue;
        }
        sum = sum + particle_density(p, particle);
    }
    return sum;
}

// 泡が存在しうる範囲（アリーナ＋飛翔の高さ）を大まかに囲うAABB。
// レイがこの箱の外側にある区間はマーチさせず、空を無駄に評価しないための
// Phase 1の簡易最適化（第25.3節の「小規模プロトタイプ向け最小実装」の一部）。
const BOUNDS_MIN: vec3<f32> = vec3<f32>(-18.0, -0.5, -18.0);
const BOUNDS_MAX: vec3<f32> = vec3<f32>(18.0, 6.0, 18.0);

const DENSITY_THRESHOLD: f32 = 1.0;
// 32だとAABB全体(最大36x6.5x36)に対してステップが粗すぎ、粒子の塊
// （半径1〜2程度）をまたぎ越して一切ヒットしないことがあった。解像度を上げる。
const MAX_STEPS: i32 = 96;

fn ray_box_intersect(origin: vec3<f32>, inv_dir: vec3<f32>) -> vec2<f32> {
    let t0 = (BOUNDS_MIN - origin) * inv_dir;
    let t1 = (BOUNDS_MAX - origin) * inv_dir;
    let tmin = min(t0, t1);
    let tmax = max(t0, t1);
    let t_enter = max(max(tmin.x, tmin.y), tmin.z);
    let t_exit = min(min(tmax.x, tmax.y), tmax.z);
    return vec2<f32>(t_enter, t_exit);
}

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    // near: NDC z=1(reverse-Zの近平面)を世界座標へ逆変換した「点」。w≠0なので
    // 通常通り同次除算する。
    let near = view.world_from_clip * vec4<f32>(in.ndc, 1.0, 1.0);
    let origin = near.xyz / near.w;

    // far: NDC z=0(reverse-Zの無限遠平面)は、射影行列の性質上「無限遠点」に
    // 対応し、world_from_clip変換後は w がほぼ0になる（同次座標で w=0 は
    // 「方向」を表す）。0除算(NaN/Inf)になるためw除算はせず、xyzをそのまま
    // 方向ベクトルとして使う。
    let far_dir = view.world_from_clip * vec4<f32>(in.ndc, 0.0, 1.0);
    let direction = normalize(far_dir.xyz);

    let bounds = ray_box_intersect(origin, 1.0 / direction);
    var t = max(bounds.x, 0.0);
    let t_max = bounds.y;
    if (t_max <= t) {
        discard;
    }

    let step_size = (t_max - t) / f32(MAX_STEPS);
    var hit = false;
    var hit_pos = origin;

    for (var step = 0; step < MAX_STEPS; step = step + 1) {
        let sample_pos = origin + direction * t;
        if (scene_density(sample_pos) > DENSITY_THRESHOLD) {
            hit = true;
            hit_pos = sample_pos;
            break;
        }
        t = t + step_size;
    }

    if (!hit) {
        discard;
    }

    let eps = 0.03;
    let normal = normalize(vec3<f32>(
        scene_density(hit_pos + vec3<f32>(eps, 0.0, 0.0)) - scene_density(hit_pos - vec3<f32>(eps, 0.0, 0.0)),
        scene_density(hit_pos + vec3<f32>(0.0, eps, 0.0)) - scene_density(hit_pos - vec3<f32>(0.0, eps, 0.0)),
        scene_density(hit_pos + vec3<f32>(0.0, 0.0, eps)) - scene_density(hit_pos - vec3<f32>(0.0, 0.0, eps))
    ) * -1.0);

    let light_dir = normalize(vec3<f32>(0.3, 0.8, 0.4));
    let ndotl = max(dot(normal, light_dir), 0.15);
    let view_dir = normalize(view.camera_world_position - hit_pos);
    let rim = pow(1.0 - max(dot(normal, view_dir), 0.0), 2.0);
    let base_color = vec3<f32>(0.85, 0.95, 1.0);

    return vec4<f32>(base_color * ndotl + rim * 0.25, 0.92);
}
