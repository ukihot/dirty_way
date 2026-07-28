// リアルタイム・ハンドソープ表現：Metaballレイマーチ描画（doc/soap-model.md 第10〜12,24,25,31節）。
// 画面全体を覆う1枚の三角形を描き、フラグメントシェーダー側でカメラレイを
// 再構築してFoam Instance Bufferを直接評価する（Phase 1: 3D Density Gridは使わない）。
//
// Phase 1では深度テストを行わない（詳細はsoap.rsの depth_stencil コメントを参照）。

// 1スロットは「泡粒子」ではなく「1個のFoam Aggregateの見た目の変形状態」を
// 表すため、`Particle`ではなく`FoamInstance`と呼ぶ（doc第31節）。
struct FoamInstance {
    position: vec3<f32>,
    velocity: vec3<f32>,
    scale: vec3<f32>,
    state: u32,
    lifetime: f32,
    base_radius: f32,
    generation: u32,
};

struct SoapView {
    clip_from_world: mat4x4<f32>,
    world_from_clip: mat4x4<f32>,
    camera_world_position: vec3<f32>,
    // Foam Quality（doc/soap-issues.md S-11a）に応じたレイマーチのステップ数。
    raymarch_steps: u32,
    // MICROSTRUCTURE_* 定数のいずれかと対応。
    microstructure_quality: u32,
};

const STATE_INACTIVE: u32 = 0u;

const MICROSTRUCTURE_SIMPLE: u32 = 0u;
const MICROSTRUCTURE_NORMAL: u32 = 1u;
const MICROSTRUCTURE_DETAILED: u32 = 2u;

// soap_compute.wgsl の MAX_LIFETIME と同じ値に保つこと。課題S-08：寿命の
// 終わり際に密度を薄めてフェードアウトさせる（scaleをcompute側で毎フレーム
// 縮めると乗算が重なって指数的に潰れてしまうため、render側でlifetimeから
// 都度計算する）。
const MAX_LIFETIME: f32 = 6.0;
const FADE_DURATION: f32 = 1.0;

@group(0) @binding(0) var<storage, read> foam_instances: array<FoamInstance>;
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

// 課題S-03：表面ノイズ用の簡易ハッシュ/バリューノイズ。Instanceの位置をシードに
// 混ぜているため、着弾位置が変わるたびに模様も変わり、滑らかな楕円体だけが
// 並ぶ「毎回同じに見える」印象を和らげる。
fn hash31(p: vec3<f32>) -> f32 {
    var p3 = fract(p * 0.1031);
    p3 = p3 + dot(p3, p3.zyx + 31.32);
    return fract((p3.x + p3.y) * p3.z);
}

fn value_noise3(p: vec3<f32>) -> f32 {
    let i = floor(p);
    let f = fract(p);
    let u = f * f * (3.0 - 2.0 * f);

    let c000 = hash31(i + vec3<f32>(0.0, 0.0, 0.0));
    let c100 = hash31(i + vec3<f32>(1.0, 0.0, 0.0));
    let c010 = hash31(i + vec3<f32>(0.0, 1.0, 0.0));
    let c110 = hash31(i + vec3<f32>(1.0, 1.0, 0.0));
    let c001 = hash31(i + vec3<f32>(0.0, 0.0, 1.0));
    let c101 = hash31(i + vec3<f32>(1.0, 0.0, 1.0));
    let c011 = hash31(i + vec3<f32>(0.0, 1.0, 1.0));
    let c111 = hash31(i + vec3<f32>(1.0, 1.0, 1.0));

    let x00 = mix(c000, c100, u.x);
    let x10 = mix(c010, c110, u.x);
    let x01 = mix(c001, c101, u.x);
    let x11 = mix(c011, c111, u.x);
    let y0 = mix(x00, x10, u.y);
    let y1 = mix(x01, x11, u.y);
    return mix(y0, y1, u.z);
}

fn instance_density(p: vec3<f32>, inst: FoamInstance) -> f32 {
    // 楕円体化：スケールで正規化してから単位球のカーネルを評価する（第10節）。
    let local = (p - inst.position) / inst.scale;
    let d = dot(local, local);
    let base = max(0.0, 1.0 - d);

    // 課題S-11a：Microstructureの詳細度をQualityで段階化する。Simpleでは
    // ノイズを一切評価しない（value_noise3はhash31を8回呼ぶのでLow環境では
    // 無視できないコスト）。Detailedは高周波オクターブを1つ重ねる。
    var noisy = base;
    if (view.microstructure_quality != MICROSTRUCTURE_SIMPLE) {
        // ノイズはbaseに比例させ、Instanceから十分離れた「何もない空間」に
        // 密度が生まれないようにする。
        let n1 = value_noise3(p * 4.0 + inst.position * 5.0) - 0.5;
        var n = n1;
        if (view.microstructure_quality == MICROSTRUCTURE_DETAILED) {
            let n2 = value_noise3(p * 9.0 + inst.position * 3.0) - 0.5;
            n = n1 * 0.7 + n2 * 0.3;
        }
        noisy = max(0.0, base + n * 0.35 * base);
    }

    // 課題S-08：寿命の最後のFADE_DURATION秒で密度を1.0→0.0へ薄める。
    // DENSITY_THRESHOLDに届かなくなり、外側から縮むように自然に消える。
    let fade_start = MAX_LIFETIME - FADE_DURATION;
    let fade = 1.0 - clamp((inst.lifetime - fade_start) / FADE_DURATION, 0.0, 1.0);
    return noisy * fade;
}

fn scene_density(p: vec3<f32>) -> f32 {
    var sum = 0.0;
    let count = arrayLength(&foam_instances);
    for (var i = 0u; i < count; i = i + 1u) {
        let inst = foam_instances[i];
        if (inst.state == STATE_INACTIVE) {
            continue;
        }
        sum = sum + instance_density(p, inst);
    }
    return sum;
}

// 泡が存在しうる範囲（アリーナ＋飛翔の高さ）を大まかに囲うAABB。
// レイがこの箱の外側にある区間はマーチさせず、空を無駄に評価しないための
// Phase 1の簡易最適化（第25.3節の「小規模プロトタイプ向け最小実装」の一部）。
const BOUNDS_MIN: vec3<f32> = vec3<f32>(-18.0, -0.5, -18.0);
const BOUNDS_MAX: vec3<f32> = vec3<f32>(18.0, 6.0, 18.0);

const DENSITY_THRESHOLD: f32 = 1.0;
// ステップ数はview.raymarch_steps（Foam Quality、doc S-11a）で決まる。
// 96より粗いと、AABB全体(最大36x6.5x36)に対してInstanceの塊
// （半径1〜2程度）をまたぎ越して一切ヒットしないことがあるので、
// Lowでもある程度のステップ数は確保する（quality.rs参照）。

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

    let max_steps = i32(view.raymarch_steps);
    let step_size = (t_max - t) / f32(max_steps);
    var hit = false;
    var hit_pos = origin;

    for (var step = 0; step < max_steps; step = step + 1) {
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
