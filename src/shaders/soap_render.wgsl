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
    // active_slots の実際に有効な要素数（課題S-12）。
    active_count: u32,
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

// 課題S-13：`instance_density`のbase(=max(0,1-d))はInstance中心(d=0)で
// ちょうど1.0に達し、表面(d=1)で0まで落ちる形をしている。旧アーキテクチャ
// （1発=10〜30個の重なり合う粒子）は、複数Instanceの合算でこの閾値を
// 超えさせる「メタボール融合」を前提に1.0へ調整されていた。今は1発=1個の
// 孤立したFoam Aggregateが基本形なので、単体では中心のほぼ一点でしか
// 1.0に届かず、しかもMicrostructureノイズが密度を最大±17.5%
// （Detailedならさらに）揺らすため、その一点すらノイズ次第で閾値を割って
// しまい、「まばらな小石」のような見た目になっていた。孤立した1個でも
// 十分な体積（中心から見て半径の6〜7割程度）を持てるよう、ノイズによる
// 落ち込み分に余裕を持たせて下げる。
const DENSITY_THRESHOLD: f32 = 0.5;
// 適応ステップ（bounding sphereの外にいる間の安全なジャンプ距離の下限）。
// 距離が0に近い状況での停留を避けるための最小前進量。
const MIN_MARCH_STEP: f32 = 0.02;
// 少なくとも1つのbounding sphere内部にいる間に使う、細かい固定ステップ。
// Instanceの半径（BUBBLE_MAX_RADIUS×最大spread、概ね2弱）に対して十分
// 細かく、表面を取りこぼしにくい値。
const FINE_STEP: f32 = 0.08;

@group(0) @binding(0) var<storage, read> foam_instances: array<FoamInstance>;
@group(0) @binding(1) var<uniform> view: SoapView;
// 課題S-12：poolの512スロット全部ではなく、実際に生きているAggregateの
// スロット番号だけを並べたリスト（先頭 view.active_count 個だけが有効）。
@group(0) @binding(2) var<storage, read> active_slots: array<u32>;

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

// Instanceを球（半径は最長の半軸長）で包む。楕円体の表面上のどの点も
// 中心からmax(scale.x,scale.y,scale.z)より遠くにはならないので、これは
// 「密度が非ゼロになりうる範囲」を過不足なく覆う安全な外接球になる。
fn bounding_radius(inst: FoamInstance) -> f32 {
    return max(inst.scale.x, max(inst.scale.y, inst.scale.z));
}

// 課題S-12：`foam_instances`はpool容量分（512）の固定長配列で、そのうち
// 実際にアクティブなのはせいぜい数個〜数十個でしかない。以前はここを
// 「0..arrayLength(&foam_instances)」で総当たりしており、レイマーチの
// 各ステップで常に512回のループが走っていた（ノイズ計算自体はbounding
// sphereでスキップできても、ループの回数そのものは減らせていなかった）。
// 低性能GPU（GTX 1650 SUPER）で実測FPS=1という壊滅的な結果になったのは
// これが原因。`active_slots`（Aggregate数個分しかない配列）だけを辿る
// ことで、ループ回数を「常に512」から「今アクティブな数だけ」に減らす。
//
// 全Instanceを総当たりで評価する素朴な実装は、
//     Ray March Steps × Foam Instances × Microstructure Noise
// というコスト構造になり、Detailedなら1 Instanceあたりhash31が最大16回
// 走る。ここではさらに2つの最適化を組み合わせて無駄な評価を減らす。
//
// 1. 粗判定（broad phase）：bounding sphereの外側にあるInstanceは、
//    ノイズ込みの高価な instance_density を呼ぶ前に安価な距離比較だけで
//    スキップする（early_exitの有無に関わらず常に安全・正確）。
// 2. 早期終了：密度の合計が閾値に達した時点で、残りのInstanceを評価せずに
//    打ち切る。「閾値を超えているか」という真偽値だけが必要なレイマーチの
//    表面判定では安全（超えた事実は変わらない）。ただし打ち切ると返り値は
//    「真の合計」ではなく「閾値をわずかに超えた時点の部分和」になるため、
//    法線を有限差分で求める際にサンプル間で不揃いな値を引き算してしまう
//    おそれがある。法線計算では early_exit=false を渡し、常に真の合計を使う。
fn scene_density(p: vec3<f32>, early_exit: bool) -> f32 {
    var sum = 0.0;
    for (var i = 0u; i < view.active_count; i = i + 1u) {
        let inst = foam_instances[active_slots[i]];
        if (inst.state == STATE_INACTIVE) {
            continue;
        }
        if (distance(p, inst.position) > bounding_radius(inst)) {
            continue;
        }
        sum = sum + instance_density(p, inst);
        if (early_exit && sum >= DENSITY_THRESHOLD) {
            return sum;
        }
    }
    return sum;
}

// レイマーチの可変ステップ用：アクティブな全Instanceのbounding sphere境界
// までの符号付き距離のうち最小のものを返す。正の値なら「そこまでは何の
// 密度も無いと保証できる」安全なジャンプ距離。0以下なら、少なくとも1つの
// bounding sphere内部にいる（＝密度を実際に評価すべき領域）。
fn nearest_bound_distance(p: vec3<f32>) -> f32 {
    var nearest = 1.0e9;
    for (var i = 0u; i < view.active_count; i = i + 1u) {
        let inst = foam_instances[active_slots[i]];
        if (inst.state == STATE_INACTIVE) {
            continue;
        }
        let d = distance(p, inst.position) - bounding_radius(inst);
        nearest = min(nearest, d);
    }
    return nearest;
}

// 泡が存在しうる範囲（アリーナ＋飛翔の高さ）を大まかに囲うAABB。
// レイがこの箱の外側にある区間はマーチさせず、空を無駄に評価しないための
// Phase 1の簡易最適化（第25.3節の「小規模プロトタイプ向け最小実装」の一部）。
const BOUNDS_MIN: vec3<f32> = vec3<f32>(-18.0, -0.5, -18.0);
const BOUNDS_MAX: vec3<f32> = vec3<f32>(18.0, 6.0, 18.0);

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

    // 適応ステップ・レイマーチ（doc/soap-issues.md S-11a追記）。固定刻みで
    // AABB全体を均等に調べる素朴な実装は「泡が無い空間」も律儀に細かく
    // サンプリングしてしまう。bounding sphereの外にいる間は
    // nearest_bound_distance() が返す「安全にジャンプできる距離」だけ
    // 一気に進み、少なくとも1つのbounding sphere内部に入ってから初めて
    // FINE_STEP刻みで実際の密度を評価する。
    let max_steps = i32(view.raymarch_steps);
    var hit = false;
    var hit_pos = origin;

    for (var step = 0; step < max_steps; step = step + 1) {
        if (t >= t_max) {
            break;
        }
        let sample_pos = origin + direction * t;

        let empty_dist = nearest_bound_distance(sample_pos);
        if (empty_dist > MIN_MARCH_STEP) {
            t = t + max(empty_dist, MIN_MARCH_STEP);
            continue;
        }

        if (scene_density(sample_pos, true) > DENSITY_THRESHOLD) {
            hit = true;
            hit_pos = sample_pos;
            break;
        }
        t = t + FINE_STEP;
    }

    if (!hit) {
        discard;
    }

    let eps = 0.03;
    // 法線は真の合計値の差分から求める（early_exit=false、上のコメント参照）。
    let normal = normalize(vec3<f32>(
        scene_density(hit_pos + vec3<f32>(eps, 0.0, 0.0), false) - scene_density(hit_pos - vec3<f32>(eps, 0.0, 0.0), false),
        scene_density(hit_pos + vec3<f32>(0.0, eps, 0.0), false) - scene_density(hit_pos - vec3<f32>(0.0, eps, 0.0), false),
        scene_density(hit_pos + vec3<f32>(0.0, 0.0, eps), false) - scene_density(hit_pos - vec3<f32>(0.0, 0.0, eps), false)
    ) * -1.0);

    let light_dir = normalize(vec3<f32>(0.3, 0.8, 0.4));
    let ndotl = max(dot(normal, light_dir), 0.15);
    let view_dir = normalize(view.camera_world_position - hit_pos);
    let rim = pow(1.0 - max(dot(normal, view_dir), 0.0), 2.0);
    let base_color = vec3<f32>(0.85, 0.95, 1.0);

    return vec4<f32>(base_color * ndotl + rim * 0.25, 0.92);
}
