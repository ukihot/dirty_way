//! ゲームバランス用の定数。
//!
//! ハンドソープ筐体を真横から見たサイドビュー（doc/soap-model.md
//! 2026-07-29追記：3Dトップダウンから2Dサイドビューへ方針転換）。
//! X=床に沿った水平位置、Y=高さ（重力方向）で、Zは廃止した。

use avian2d::prelude::*;

/// 課題S-17/S-23/S-24（2026-07-29、doc/soap-issues.md）：
/// - Bubble（飛行中）同士は衝突させない。SPRAY_INTERVAL間隔の連続噴射では
///   直前の粒と至近距離で常に接触することになり、衝突させると低反発・
///   高摩擦・回転ロックの組み合わせで互いに引っかかり、鎖状に連結して
///   重力に逆らって空中に留まってしまう（S-23）。
/// - しかし「積み上がらない」のも違う——既存の泡だまりの上に新しい泡が
///   乗って、メタボールとして融合しながら嵩が増えていくのが正しい液体の
///   挙動（S-24）。そこで「飛行中のBubble」と「着地済みのLandedBubble」を
///   別レイヤーに分け、Bubble×Bubbleは衝突させない一方、Bubble×
///   LandedBubbleは衝突させる。これにより新しい泡は他の飛行中の泡は
///   すり抜けつつ、既に着地した泡だまりの上には正しく乗って積み上がる。
/// - 課題S-34（2026-07-30）：LandedBubble×LandedBubbleも衝突させる必要が
///   ある。ここが抜けていると、2個目以降の泡が着地してLandedBubbleへ
///   切り替わった瞬間、真下の既存LandedBubbleとの衝突が消えて支えを
///   失い、床まで沈み込んでしまう（山が高さを持てず、常に床の高さで
///   扁平化するだけになる）。
#[derive(PhysicsLayer, Default)]
pub enum GameLayer {
    #[default]
    Default,
    Floor,
    Bubble,
    LandedBubble,
    Enemy,
}

/// 床の半幅（見た目にも使用）。中心にプレイヤーのノズルが立ち、
/// 左右の端から敵が侵入してくる。
pub const ARENA_RADIUS: f32 = 16.0;
/// 敵が左右の端からスポーンする、中心からの水平距離。
pub const ENEMY_SPAWN_RADIUS: f32 = 15.0;
/// これより中心から離れた泡は掃除（despawn）する。
pub const BUBBLE_DESPAWN_DISTANCE: f32 = ARENA_RADIUS + 4.0;
/// 敵がこの水平距離まで中心に近づいたらプレイヤーにダメージ。
pub const CENTER_KILL_RADIUS: f32 = 1.1;

/// ノズル（発射口）の、プレイヤー中心からの距離。狙い方向に応じて
/// このアンカー点の周りを一周回転する。
pub const NOZZLE_HEIGHT: f32 = 1.0;
pub const NOZZLE_RADIUS: f32 = 0.9;
/// キーボード（A/D）でのノズル回転速度（ラジアン/秒）。
pub const AIM_ROTATE_SPEED: f32 = 3.0;

/// 2026-07-29追記（doc/soap-issues.md）：「チャージして離すと1発飛ぶ」を
/// やめ、本物のハンドソープ容器のポンプのように「押している間、ノズルが
/// 沈み込みながら連続的にソープが出続け、沈みきったら（＝ポンプストローク
/// を使い切ったら）止まる。離すとバネで元の高さに戻る」という機構にした。
/// 残量ゲージのUIは不要——ノズル自体の高さがそのまま残量を表す。
///
/// 押しっぱなしでノズルが最深部まで沈み込むのにかかる時間。
pub const NOZZLE_PRESS_TIME: f32 = 1.6;
/// 離してからノズルが元の高さまでバネで戻るのにかかる時間
/// （押し込みより速く跳ね返る）。
pub const NOZZLE_RELEASE_TIME: f32 = 0.6;
/// フルに押し込んだときにノズルが沈み込む距離（Y方向、NOZZLE_HEIGHTからの
/// オフセット）。
pub const NOZZLE_PRESS_STROKE: f32 = 0.55;
/// 噴射中、泡を発射する間隔（秒）。ノズルが沈みきる（NOZZLE_PRESS_TIME）
/// までの間、この間隔で連続的に小さな泡を出し続ける。
///
/// 課題S-22（2026-07-29）：0.12秒間隔だと1粒ごとの間隔が空きすぎて
/// 「ポポポ」と個別の丸が飛んでいるように見えてしまっていた（実機確認）。
/// 間隔を詰め、ブレも減らして、隣り合う粒同士がsoap_render.wgslの
/// MERGE_REACHで繋がりやすい密な列にすることで「1本の連続した液体の
/// 流れ」に見せる。
pub const SPRAY_INTERVAL: f32 = 0.045;
/// 連射される泡1個ごとの初速・半径・威力。チャージという概念が無くなった
/// ので、蛇口から出る水のように毎回ほぼ均一にする（わずかな乱数はある）。
pub const SPRAY_SPEED: f32 = 11.0;
pub const SPRAY_BUBBLE_RADIUS: f32 = 0.3;
pub const SPRAY_POWER: i32 = 1;
/// 課題S-25（2026-07-29）：ノズル付近まで泡だまりが育つと、生まれたばかりの
/// 泡がまだ何の速度も得ていないその場で既存の泡だまりと接触判定してしまい、
/// 飛び出す前に着地扱いされて固まってしまっていた（実機確認：連射する
/// たびに射出口の真下で「かまくら」状に積み上がり、前に飛ばなくなる）。
/// スポーンからこの秒数が経つまでは着地判定そのものを無視し、必ず一定
/// 距離飛んでから初めて着地できるようにする（bubble::settle_landed_bubbles
/// 参照）。
pub const BUBBLE_LANDING_GRACE_PERIOD: f32 = 0.12;
/// 課題S-26（2026-07-29）：床・泡だまりに触れた瞬間に速度を0にして即座に
/// Staticへ固定していたため、飛んで来た軌跡の形（弧・蛇状）がそのまま
/// 固まって残ってしまっていた（実機確認：「山が更新される」のではなく
/// 「蛇のように積み上がる」）。液体らしく、実際に静止するまでは重力・
/// 摩擦に従って山の斜面を滑らせ続け（Dynamicのまま）、本当に落ち着いて
/// からようやくStatic化する。これにより新しい泡は既存の山の低いところへ
/// 自然に流れ込み、山の形そのものが更新されていくように見える。
/// 「静止」とみなす速度のしきい値。
pub const BUBBLE_SETTLE_SPEED: f32 = 0.35;
/// 速度がしきい値を下回った状態がこの秒数続いたら、本当に静止したと
/// 判断してStatic化する（一瞬の速度低下で早まって固まらないように）。
pub const BUBBLE_SETTLE_DURATION: f32 = 0.12;
/// 連射中、毎回わずかに狙いをブレさせる最大角度（ラジアン）。課題S-22：
/// 0.12だと粒ごとの軌道がバラけすぎて列が繋がって見えなかったため、
/// 「一続きの水流」に見える程度まで弱めた。
pub const SPRAY_JITTER: f32 = 0.04;

/// 課題S-32（2026-07-29）：着地時、クラスター内の塊が一斉に同じ倍率で
/// 「その場でぺたっと」潰れるだけで、塊同士の位置関係は変わらなかった。
/// 勢いよく叩きつけられた液体は実際には飛び散るはずなので、着地の瞬間の
/// 速度（`impact_speed`）に応じてクラスターのオフセット自体を外側へ
/// 広げる（bubble.rs::settle_landed_bubbles、soap.rs::
/// extract_foam_aggregates参照）。scatter = min(impact_speed *
/// SCATTER_FACTOR, SCATTER_MAX)倍、オフセットが広がる。
pub const IMPACT_SCATTER_FACTOR: f32 = 0.12;
pub const IMPACT_SCATTER_MAX: f32 = 2.0;

/// 課題S-40（2026-07-30、実機フィードバック）：S-35で泡が時間経過で
/// 自然に消えなくなった結果、床全体が見えない泡で埋め尽くされ、敵が
/// 出現直後に即座に倒されて先へ進めなくなる（ゲームとして事実上
/// 終わらない）不具合が起きた。個々の泡を時間で消す仕組みは復活させず、
/// 「同時に存在できる泡の総数」に上限を設け、超えた分は最も古い泡から
/// 実体ごと（物理・当たり判定込みで）despawnする（bubble.rs::
/// BubblePopulation）。SPRAY_INTERVAL間隔・NOZZLE_PRESS_TIMEでの連射量を
/// 踏まえ、数分程度遊べば入れ替わりが起きる程度の値。
pub const MAX_LIVE_BUBBLES: usize = 500;

/// GPU側でFoam Aggregateの見た目を同時に表現できる最大数（doc/soap-model.md
/// 第28.2節）。 `bubble.rs`（Main World、スロット割当）と`soap.rs`（Render
/// World、GPUバッファ確保）の
/// 両方がこの値を共有する（2箇所に独立した定数を置いて食い違わせない、
/// S-09の教訓）。課題S-30でFOAM_CLUSTER_MAXを5→9に上げた分、比例して
/// 増やしてある（同時に見た目を持てるBubble数の実質上限を維持するため）。
pub const FOAM_INSTANCE_POOL_SIZE: u32 = 1024;

/// 1つのBubbleが同時に使うFoam Instanceスロット数の範囲（doc/soap-issues.md
/// 2026-07-28追記、課題S-30で2026-07-29追記）。1発=1個の孤立した楕円体だと、
/// どれだけ扁平化しても「潰れたボール」にしか見えず、メタボール本来の
/// 「複数の塊が寄り集まって融合した液体」という見た目にならない
/// （soap_render.wgslのDENSITY_THRESHOLDコメント参照）。ゲームロジック
/// （Avianの当たり判定）は引き続き1発=1RigidBodyのままにし、見た目だけ
/// 複数の小さな塊をBubble中心の周りに重ねて配置し直すことで、その融合
/// 効果を取り戻す。
///
/// 課題S-30：塊の個数を毎回FOAM_CLUSTER_MAX固定にすると、配置パターン
/// （cluster_offsets）が毎回同じ形の“花びら”になり、「ありきたりで
/// 面白くない」見た目になっていた（実機確認）。実際のハンドソープの泡は
/// 4〜7個くらいの不揃いな塊が集まってできているはずなので、個数自体も
/// Bubbleごとにランダム化する（bubble::cluster_offsets参照）。
/// スロットは常にFOAM_CLUSTER_MAX個確保するが（可変長確保は複雑なので
/// 避ける）、実際に使う（GPUへ送る）個数はFOAM_CLUSTER_MIN〜MAXの範囲で
/// 毎回ランダムに決める。
pub const FOAM_CLUSTER_MIN: u32 = 4;
pub const FOAM_CLUSTER_MAX: usize = 7;

/// 敵が泡に埋もれている間の鈍足倍率と、接触が切れてから鈍足が続く猶予秒数。
pub const TRAPPED_SPEED_MULTIPLIER: f32 = 0.15;
pub const TRAPPED_LINGER_TIME: f32 = 0.5;

/// プレイヤーの初期体力。
pub const PLAYER_MAX_HEALTH: i32 = 5;

/// 敵の出現間隔（開始値・下限）と難易度上昇レート。
pub const ENEMY_SPAWN_INTERVAL_START: f32 = 1.6;
pub const ENEMY_SPAWN_INTERVAL_MIN: f32 = 0.45;
pub const DIFFICULTY_RAMP_PER_SEC: f32 = 0.01;
