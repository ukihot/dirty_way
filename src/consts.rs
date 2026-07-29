//! ゲームバランス用の定数。
//!
//! ハンドソープ筐体を真横から見たサイドビュー（doc/soap-model.md
//! 2026-07-29追記：3Dトップダウンから2Dサイドビューへ方針転換）。
//! X=床に沿った水平位置、Y=高さ（重力方向）で、Zは廃止した。

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

/// 押し込み時間（チャージ）の上限秒数。
pub const CHARGE_MAX_TIME: f32 = 1.4;
/// チャージ量に応じた初速（狙った方向への発射速度）の範囲。サイドビューでは
/// 狙い自体が上下左右を含む2D方向なので、以前のような水平/山なり成分の
/// 分離は不要——重力が弾道の山なりを自然に作る。
pub const CHARGE_MIN_SPEED: f32 = 8.0;
pub const CHARGE_MAX_SPEED: f32 = 18.0;
/// チャージが浅いほど狙いがブレる最大角度（ラジアン）。「不器用な操作感」用。
pub const CHARGE_MAX_JITTER: f32 = 0.5;

/// チャージ量に応じた泡の大きさ。
pub const BUBBLE_MIN_RADIUS: f32 = 0.28;
pub const BUBBLE_MAX_RADIUS: f32 = 0.55;
/// 泡の寿命（秒）。当たらなくても時間切れで消える。
pub const BUBBLE_LIFETIME: f32 = 6.0;

/// GPU側でFoam Aggregateの見た目を同時に表現できる最大数（doc/soap-model.md
/// 第28.2節）。 `bubble.rs`（Main World、スロット割当）と`soap.rs`（Render
/// World、GPUバッファ確保）の
/// 両方がこの値を共有する（2箇所に独立した定数を置いて食い違わせない、
/// S-09の教訓）。
pub const FOAM_INSTANCE_POOL_SIZE: u32 = 512;

/// 1つのBubbleが同時に使うFoam Instanceスロット数（doc/soap-issues.md
/// 2026-07-28追記）。1発=1個の孤立した楕円体だと、どれだけ扁平化しても
/// 「潰れたボール」にしか見えず、メタボール本来の「複数の塊が寄り集まって
/// 融合した液体」という見た目にならない（soap_render.wgslのDENSITY_THRESHOLD
/// コメント参照：旧アーキテクチャは複数Instanceの合算で閾値を超えることを
/// 前提にしていた）。ゲームロジック（Avianの当たり判定）は引き続き1発=1
/// RigidBodyのままにし、見た目だけ複数の小さな塊をBubble中心の周りに重ねて
/// 配置し直すことで、その融合効果を取り戻す。
pub const FOAM_SUB_INSTANCES: usize = 5;

/// 敵が泡に埋もれている間の鈍足倍率と、接触が切れてから鈍足が続く猶予秒数。
pub const TRAPPED_SPEED_MULTIPLIER: f32 = 0.15;
pub const TRAPPED_LINGER_TIME: f32 = 0.5;

/// プレイヤーの初期体力。
pub const PLAYER_MAX_HEALTH: i32 = 5;

/// 敵の出現間隔（開始値・下限）と難易度上昇レート。
pub const ENEMY_SPAWN_INTERVAL_START: f32 = 1.6;
pub const ENEMY_SPAWN_INTERVAL_MIN: f32 = 0.45;
pub const DIFFICULTY_RAMP_PER_SEC: f32 = 0.01;
