// リアルタイム・ハンドソープ表現：2Dメタボール直接評価（doc/soap-model.md 第10〜12,24,25,31節）。
// 画面全体を覆う1枚の三角形を描き、フラグメントシェーダー側で各ピクセルの
// ワールド座標（XY、Zは廃止）を1回だけ求め、Foam Instance
// Bufferをその点で直接評価する。
//
// 2026-07-29追記：3Dトップダウンから2Dサイドビューへ方針転換。以前はカメラ
// レイを画面奥へレイマーチして「泡の表面と交差する点」を探していたが、
// この構図では泡はカメラのビュー平面（Z=0）上に直接乗っているだけなので、
// マーチする奥行きが存在しない。ピクセルごとに対応するワールド座標を
// 1回だけ求めて密度場を評価すれば、それがそのまま表面判定になる
// （通常の2D SDFメタボールと同じ形）。

// 1スロットは「泡粒子」ではなく「1個のFoam Aggregateの見た目の変形状態」を
// 表すため、`Particle`ではなく`FoamInstance`と呼ぶ（doc第31節）。
struct FoamInstance {
    position: vec2<f32>,
    velocity: vec2<f32>,
    scale: vec2<f32>,
    state: u32,
    lifetime: f32,
    base_radius: f32,
    generation: u32,
    // 課題S-24：このシェーダーでは使わないが、soap_compute.wgslと同じ
    // FoamInstance構造体（同じバッファを共有）のバイトレイアウトを保つため
    // に必要。
    landing: u32,
};

struct SoapView {
    world_from_clip: mat4x4<f32>,
    // MICROSTRUCTURE_* 定数のいずれかと対応。
    microstructure_quality: u32,
    // active_slots の実際に有効な要素数（課題S-12）。
    active_count: u32,
};

const STATE_INACTIVE: u32 = 0u;
const STATE_FLYING: u32 = 1u;

const MICROSTRUCTURE_SIMPLE: u32 = 0u;
const MICROSTRUCTURE_NORMAL: u32 = 1u;
const MICROSTRUCTURE_DETAILED: u32 = 2u;

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

// 課題S-16（2026-07-29）：DENSITY_THRESHOLDを跨いだ瞬間にdiscardし、
// 跨いだ後は常に固定alpha=0.92で塗るハードカットオフだと、複数Instanceが
// 触れ合っていても輪郭がそれぞれ独立した硬い円のまま――「融合したフワフワの泡」
// ではなく「転がる小石」に見えてしまっていた（3D→2D方針転換後の実機確認で発覚）。
// alpha自体をdensityでなだらかに変化させることで、(1)輪郭が柔らかいグラデーション
// になる、(2)隣接Instance同士の低密度域が先に重なって視覚的に「橋渡し」され、
// 実際に融合して見える、という2つの効果を同時に得る。
const ALPHA_SOFT_START: f32 = DENSITY_THRESHOLD * 0.35;
const ALPHA_SOFT_END: f32 = DENSITY_THRESHOLD * 1.2;
const MAX_ALPHA: f32 = 0.92;

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
// 混ぜているため、着弾位置が変わるたびに模様も変わり、滑らかな楕円だけが
// 並ぶ「毎回同じに見える」印象を和らげる。
fn hash21(p: vec2<f32>) -> f32 {
    var p3 = fract(vec3<f32>(p.x, p.y, p.x) * 0.1031);
    p3 = p3 + dot(p3, p3.yzx + 33.33);
    return fract((p3.x + p3.y) * p3.z);
}

fn value_noise2(p: vec2<f32>) -> f32 {
    let i = floor(p);
    let f = fract(p);
    let u = f * f * (3.0 - 2.0 * f);

    let c00 = hash21(i + vec2<f32>(0.0, 0.0));
    let c10 = hash21(i + vec2<f32>(1.0, 0.0));
    let c01 = hash21(i + vec2<f32>(0.0, 1.0));
    let c11 = hash21(i + vec2<f32>(1.0, 1.0));

    let x0 = mix(c00, c10, u.x);
    let x1 = mix(c01, c11, u.x);
    return mix(x0, x1, u.y);
}

// 2026-07-30追記：泡のミクロ構造（doc/soap-model.md 第27.2節「FoamField =
// LiquidFilmField − BubbleVoidField」の簡略実装）。これまでのvalue_noise2は
// 滑らかにうねる表面ノイズで、Detailed品質でしか有効になっていなかった
// （デフォルトのMedium品質＝MICROSTRUCTURE_NORMALでは表面が完全な滑らかな
// 楕円のままだった）。これだと艶々の「ジェル玉」には見えても、無数の微細
// 気泡が集まった「ハンドソープの泡」には見えない、という指摘（実機確認）を
// 受けて、粒状の凹凸（無数の小さな泡の粒）を作るWorley風セルノイズを追加する。
//
// `nearest_bubble`が返すのは「最も近い泡粒の中心までの距離（セルサイズ基準に
// 正規化）」と「その粒固有の乱数」。`bubble_microstructure`はこれを、粒の
// 中心付近で盛り上がり・粒と粒の境界（薄膜）付近で凹む山形カーブに変換する。
// この値をinstance_densityへ小さい振幅で加算するだけで、scene_densityの
// 勾配（=法線）に無数の細かい凹凸が乗り、シェーディングに無数のハイライトが
// 散らばる「泡立った」質感になる（幾何形状そのものは変えないのでシルエットは
// ほぼ保たれる）。
const BUBBLE_CELL_SIZE: f32 = 0.16;

fn bubble_hash2(p: vec2<f32>) -> vec2<f32> {
    let x = hash21(p);
    let y = hash21(p + vec2<f32>(19.19, 7.7));
    return vec2<f32>(x, y);
}

// pに最も近い泡粒の中心までの距離（Worley F1、セルサイズ基準に正規化）と、
// その粒固有の乱数（x成分。大きさのばらつきに使う）を返す。
fn nearest_bubble(p: vec2<f32>) -> vec2<f32> {
    let cell = floor(p / BUBBLE_CELL_SIZE);
    var best_dist = 10.0;
    var best_rand = 0.0;
    for (var y = -1; y <= 1; y = y + 1) {
        for (var x = -1; x <= 1; x = x + 1) {
            let neighbor = cell + vec2<f32>(f32(x), f32(y));
            let jitter = bubble_hash2(neighbor);
            let center = (neighbor + 0.15 + jitter * 0.7) * BUBBLE_CELL_SIZE;
            let d = distance(p, center) / BUBBLE_CELL_SIZE;
            if (d < best_dist) {
                best_dist = d;
                best_rand = jitter.x;
            }
        }
    }
    return vec2<f32>(best_dist, best_rand);
}

// 粒の中心(dist=0)で+1、粒の半径(dist=size)で0、それより外で-1に近づく
// 山形カーブ。粒ごとに`size`をばらつかせ、大きさの不揃いな気泡の集まりに見せる。
fn bubble_microstructure(p: vec2<f32>) -> f32 {
    let b = nearest_bubble(p);
    let dist = b.x;
    let size = 0.35 + b.y * 0.4;
    let bump = (size - dist) / size;
    return clamp(bump, -1.0, 1.0);
}

// 課題S-20（2026-07-29）：1つのBubbleの中の5個の塊同士は重なるように
// 配置しているので融合して見えるが、「別々に着地したBubble」同士は
// 物理的にぴったり重ならない限りそれぞれの半径でスパッと密度が0になり、
// 独立した丸い粒のまま——本来のメタボールが持つ「多少離れていても
// 近ければ融合する」という性質が出ていなかった（実機確認：斜めに隣接
// した泡が繋がらずおだんご状にならない）。
//
// 本来の半径（コア）のカーブはそのまま保ちつつ、その外側にもっと広く・
// もっと弱い「橋渡し用」の裾野を足す。単体では閾値に届かない弱さなので
// 孤立した泡の見た目はほぼ変わらないが、2つの泡が近づくと互いの裾野が
// 重なって合算値が閾値を超え、繋ぎ目（ネック）が見えるようになる。
const MERGE_REACH: f32 = 2.1;
const BRIDGE_STRENGTH: f32 = 0.3;

// 課題S-28（2026-07-29）：射出された瞬間から完全な丸い玉に見えてしまって
// いた——実際のハンドソープのノズルから出るのは丸い玉ではなく、勢いよく
// 伸びる液の筋のはず（実機確認）。飛行中（STATE_FLYING）だけ、速度方向に
// 伸ばした涙形（ストリーク）で密度場を評価することで、「連続して伸びる
// 液体」に見せる。着地して速度が失われれば自動的に元の丸い（そして
// soap_compute.wgslのIMPACT扁平化で潰れた）形に戻る。
// 課題S-31：STREAK_MAX=3.2は伸ばしすぎで、bubble.rsのクラスター内の
// 塊ごとの不揃いさ（課題S-30）を覆い隠して均された1本の筋に見せてしまって
// いた（実機確認：「思ったより不揃いになっていない」）。ストリークは
// 「飛んでいる感」を出す程度に弱め、クラスターの凹凸が見える余地を残す。
const STREAK_MIN_SPEED: f32 = 1.0;
const STREAK_FACTOR: f32 = 0.035;
const STREAK_MAX: f32 = 1.8;

// `p`から見た`inst`のローカル座標を、半径`scale_mult`倍の基準で正規化する。
// 飛行中は速度方向に伸ばし、それに直交する方向を少し細くする（伸びた分
// だけ体積感が保たれ、単に膨張したのではなく「伸びた」ように見える）。
fn instance_local(p: vec2<f32>, inst: FoamInstance, scale_mult: f32) -> vec2<f32> {
    let scale = inst.scale * scale_mult;
    let speed = length(inst.velocity);
    if (inst.state != STATE_FLYING || speed < STREAK_MIN_SPEED) {
        return (p - inst.position) / scale;
    }
    let dir = inst.velocity / speed;
    let perp = vec2<f32>(-dir.y, dir.x);
    let rel = p - inst.position;
    let along = dot(rel, dir);
    let across = dot(rel, perp);
    let stretch = min(1.0 + speed * STREAK_FACTOR, STREAK_MAX);
    return vec2<f32>(along / (scale.x * stretch), across / (scale.y / sqrt(stretch)));
}

fn instance_density(p: vec2<f32>, inst: FoamInstance) -> f32 {
    // 楕円化：スケールで正規化してから単位円のカーネルを評価する（第10節）。
    let local = instance_local(p, inst, 1.0);
    let d = dot(local, local);
    let core = max(0.0, 1.0 - d);

    let reach_local = instance_local(p, inst, MERGE_REACH);
    let reach_d = dot(reach_local, reach_local);
    let bridge = max(0.0, 1.0 - reach_d) * BRIDGE_STRENGTH;

    let base = max(core, bridge);

    // 課題S-16：以前はNormal/Detailed両方で±0.35*baseという強いノイズを
    // 掛けており、これが石のようなまだらな質感の主因になっていた（実機確認）。
    // ソープの泡らしい滑らかさを優先し、ノイズはDetailedのみ・振幅も大幅に
    // 弱めた「表面の微妙な揺らぎ」程度に留める。
    //
    // S-33（2026-07-30）：上のS-16対応だけだと、デフォルト品質（Medium＝
    // MICROSTRUCTURE_NORMAL）では表面ノイズが一切乗らず、完全に滑らかな
    // 楕円のまま＝艶々の「ジェル玉」にしか見えなかった。無数の微細気泡の
    // 集合という「泡」らしさを出すため、Worley風の粒状凹凸
    // （`bubble_microstructure`）をNormal以上に適用する。
    var noisy = base;
    if (view.microstructure_quality != MICROSTRUCTURE_SIMPLE) {
        let bubbles = bubble_microstructure(p * 1.0 + inst.position * 0.01);
        noisy = max(0.0, base + bubbles * 0.07 * base);
    }
    if (view.microstructure_quality == MICROSTRUCTURE_DETAILED) {
        let n1 = value_noise2(p * 4.0 + inst.position * 5.0) - 0.5;
        let n2 = value_noise2(p * 9.0 + inst.position * 3.0) - 0.5;
        let n = n1 * 0.7 + n2 * 0.3;
        noisy = max(0.0, noisy + n * 0.08 * base);
    }

    // 実機フィードバック（2026-07-30）：泡は時間経過で自然に消える必要は
    // ない。以前はここで寿命終了間際に密度をフェードさせていたが撤去した
    // （doc/soap-issues.md S-35）。
    return noisy;
}

// Instanceを円（半径は長い方の軸長）で包む。楕円の外周上のどの点も
// 中心からmax(scale.x,scale.y)より遠くにはならないので、これは
// 「密度が非ゼロになりうる範囲」を過不足なく覆う安全な外接円になる。
// 課題S-20：橋渡し用の裾野（MERGE_REACH）まで含めて非ゼロになりうるので、
// そこまで広げないと早期スキップで橋渡しの寄与を取りこぼす。
// 課題S-28：飛行中はSTREAK_MAXまで速度方向に伸びうるので、その分も
// 余裕を持たせないと、伸びた先端が早期スキップで切り取られてしまう。
fn bounding_radius(inst: FoamInstance) -> f32 {
    return max(inst.scale.x, inst.scale.y) * MERGE_REACH * STREAK_MAX;
}

// 課題S-12：`foam_instances`はpool容量分（512）の固定長配列で、そのうち
// 実際にアクティブなのはせいぜい数個〜数十個でしかない。`active_slots`
// （Aggregate数個分しかない配列）だけを辿ることで、ピクセルごとのループ
// 回数を「常に512」から「今アクティブな数だけ」に減らす。
//
// bounding circleの外側にあるInstanceは、ノイズ込みの高価な
// instance_density を呼ぶ前に安価な距離比較だけでスキップする。
fn scene_density(p: vec2<f32>) -> f32 {
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
    }
    return sum;
}

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    // 2Dの正投影ビューでは、NDCからワールドXYへの変換はZに依存しない
    // アフィン変換になる（カメラのロールが無い前提）。ncd.z=0を1点だけ
    // 逆変換すれば、そのピクセルに対応するワールド座標が一意に求まる
    // ——3Dのようにレイをマーチする必要はない。
    let world4 = view.world_from_clip * vec4<f32>(in.ndc, 0.0, 1.0);
    let world = world4.xy / world4.w;

    // 課題S-21：床（Y=0）より下には何も描かない。着地した泡が扁平化しても
    // 楕円の下半分は必ず床の下へわずかにめり込む（第14節）ので、これを
    // 描かないことで「床に接する面で平らに切り取られた」シルエットになり、
    // 丸い塊ではなく角の丸い台形のような、液体が着地して広がった見た目になる。
    if (world.y < 0.0) {
        discard;
    }

    let density = scene_density(world);
    if (density < ALPHA_SOFT_START) {
        discard;
    }
    // 課題S-16：alphaをdensityでなだらかに変化させる（コメント参照）。
    let alpha = smoothstep(ALPHA_SOFT_START, ALPHA_SOFT_END, density) * MAX_ALPHA;

    // 密度場を疑似的な高さ場として扱い、勾配から「液体表面の法線」を
    // でっち上げる（2Dゲームでもぷるっとした立体感を出すための定番の手法）。
    let eps = 0.03;
    let dx = scene_density(world + vec2<f32>(eps, 0.0)) - scene_density(world - vec2<f32>(eps, 0.0));
    let dy = scene_density(world + vec2<f32>(0.0, eps)) - scene_density(world - vec2<f32>(0.0, eps));
    let normal = normalize(vec3<f32>(-dx, -dy, 0.6));

    let light_dir = normalize(vec3<f32>(-0.35, 0.55, 0.75));
    let ndotl = max(dot(normal, light_dir), 0.45);
    let view_dir = vec3<f32>(0.0, 0.0, 1.0);
    // 課題S-16：rimの指数を上げてハイライトを鋭く・強く効かせ、艶っぽい
    // 「泡」の質感を強調する（ndotlの下限も上げ、影が石のように暗く沈むのを防ぐ）。
    let rim = pow(1.0 - max(dot(normal, view_dir), 0.0), 3.0);
    let base_color = vec3<f32>(0.88, 0.96, 1.0);

    let color = base_color * ndotl + vec3<f32>(1.0, 1.0, 1.0) * rim * 0.6;
    return vec4<f32>(color, alpha);
}
