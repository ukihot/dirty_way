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
    // 課題S-24：0=まだ飛行中、1=床に直接着地、2=既存の泡だまりの上に着地
    // （bubble::LandingSurfaceのエンコード）。Main World（Avian2D）側の
    // 衝突判定が権威で、ここは追従するだけ。着地の有無そのものの判定
    // （STATE_FLYING→STATE_IMPACT遷移）にも使う——以前は自前でY座標と
    // 床の高さを比較していたが、泡だまりの上に着地した泡は床よりずっと
    // 高い位置で静止するため、その比較では永久に着地を検知できなかった。
    landing: u32,
};

struct DriveEntry {
    target_slot: u32,
    position: vec2<f32>,
    velocity: vec2<f32>,
    base_radius: f32,
    generation: u32,
    landing: u32,
};

struct SimParams {
    dt: f32,
    floor_height: f32,
    impact_factor: f32,
    drive_count: u32,
};

const STATE_INACTIVE: u32 = 0u;
const STATE_FLYING: u32 = 1u;
const STATE_IMPACT: u32 = 2u;
const STATE_SPREADING: u32 = 3u;
const STATE_RESTING: u32 = 4u;

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
            // 課題S-24：landingは新規スポーン時だけでなく毎フレーム
            // 上書きする（position/velocityと同様）。Main World側で
            // 「今フレーム着地した」瞬間にFlying→Floor/Pileへ変わるのを
            // 見逃さないようにするため。
            inst.landing = drive_entries[i].landing;
        }
    }

    if (inst.state == STATE_INACTIVE) {
        foam_instances[index] = inst;
        return;
    }

    // 実機フィードバック（2026-07-30）：泡は時間経過で自然に消える必要は
    // ない。以前はここでlifetimeがMAX_LIFETIMEを超えたらSTATE_INACTIVEへ
    // 強制遷移させていたが撤去した（doc/soap-issues.md S-35）。lifetimeは
    // 今後使う可能性に備えて加算だけ残す。
    inst.lifetime = inst.lifetime + sim_params.dt;

    if (inst.state == STATE_FLYING) {
        // 課題S-24：着地したかどうかはMain World側（Avian2Dの衝突判定、
        // bubble::LandingSurface）の権威に従う。以前はここを
        // `position.y <= floor_height + base_radius`という床基準の絶対
        // 座標で自前判定していたが、既存の泡だまりの上に着地した泡は
        // 床よりずっと高い位置で静止するため、この条件が永久に成立せず、
        // 真ん丸のまま固まってしまっていた（S-14の修正はFloor直着地のみ
        // 対応していた）。
        if (inst.landing != 0u) {
            inst.state = STATE_IMPACT;
        }
    } else if (inst.state == STATE_IMPACT) {
        // 着弾速度→扁平化（第8節）。1フレームで即SPREADINGへ遷移する。
        // サイドビューでは画面の縦方向がそのままYなので、この扁平化は
        // 「床にぺたっと潰れて広がる水滴」として画面上にそのまま見える。
        //
        // 課題S-21：impact_speedだけに頼ると、浅い角度で着地した（下向き
        // 速度がほぼ無い）泡がほとんど扁平化せず、丸いまま着地して見えて
        // しまっていた。液体は着地速度が遅くても自重で広がるはずなので、
        // 最低保証の扁平化を設ける。
        //
        // 課題S-24：床への直接着地(landing==1)はしっかり潰れて広がる
        // （台形状の水たまり）が、既存の泡だまりの上への着地(landing==2)
        // は嵩をあまり失わず、「雪だるま」ではなく「積み上がる液体」に
        // 見えるよう、扁平化を弱めに留める（嵩の50%以上を保持）。
        let impact_speed = max(-inst.velocity.y, 0.0);
        var min_spread = 1.6;
        if (inst.landing == 2u) {
            min_spread = 1.3;
        }
        let spread = max(min_spread, 1.0 + impact_speed * sim_params.impact_factor);
        inst.scale = vec2<f32>(spread, 1.0 / spread) * inst.base_radius;
        inst.state = STATE_SPREADING;
    } else if (inst.state == STATE_SPREADING) {
        // 位置・速度の減衰はAvian側（Restitution/Friction/LinearDamping、
        // bubble.rs）が既に解いている。scale（見た目の扁平化）はIMPACT
        // 遷移の瞬間に着弾速度から一度だけ確定済み（第8節）。
        //
        // 課題S-36（2026-07-30、実機フィードバック）：以前はここで
        // target_spreadを毎フレーム0.5/秒で`max_spread`まで際限なく
        // 成長させ続けていた。泡だまりの奥のInstanceはAvian側の接触
        // ジッタ（他のBubbleに支えられ続けることで生じる微小な速度）で
        // 下のvelocity閾値判定が安定せずSTATE_RESTINGへなかなか遷移
        // できないため、SPREADINGに留まる時間が長引くほど山全体が際限
        // なく扁平になっていく不自然な見た目になっていた。IMPACTで
        // 確定したscaleをそのまま保持し、速度が閾値を下回った最初の
        // タイミングでRESTINGへ遷移するだけにする（追加の一方向的な
        // 成長はしない）。
        if (length(inst.velocity) < 0.05) {
            inst.state = STATE_RESTING;
        }
    }

    foam_instances[index] = inst;
}
