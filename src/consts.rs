//! ゲームバランス用の定数。

/// 円形ステージの半径（床の見た目にも使用）。
pub const ARENA_RADIUS: f32 = 16.0;
/// 敵がスポーンする円の半径。
pub const ENEMY_SPAWN_RADIUS: f32 = 15.0;
/// これより中心から離れた泡は掃除（despawn）する。
pub const BUBBLE_DESPAWN_DISTANCE: f32 = ARENA_RADIUS + 4.0;
/// 敵がこの距離まで中心に近づいたらプレイヤーにダメージ。
pub const CENTER_KILL_RADIUS: f32 = 1.1;

/// ノズル（発射口）の高さ・中心からの距離。
pub const NOZZLE_HEIGHT: f32 = 1.0;
pub const NOZZLE_RADIUS: f32 = 0.9;
/// キーボード（A/D）でのノズル回転速度（ラジアン/秒）。
pub const AIM_ROTATE_SPEED: f32 = 3.0;

/// 押し込み時間（チャージ）の上限秒数。
pub const CHARGE_MAX_TIME: f32 = 1.4;
/// チャージ量に応じた水平初速の範囲。
pub const CHARGE_MIN_SPEED: f32 = 6.0;
pub const CHARGE_MAX_SPEED: f32 = 16.0;
/// チャージ量に応じた山なり成分（上向き初速）の範囲。
pub const CHARGE_MIN_LOB: f32 = 3.0;
pub const CHARGE_MAX_LOB: f32 = 9.0;
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

/// 敵が泡に埋もれている間の鈍足倍率と、接触が切れてから鈍足が続く猶予秒数。
pub const TRAPPED_SPEED_MULTIPLIER: f32 = 0.15;
pub const TRAPPED_LINGER_TIME: f32 = 0.5;

/// プレイヤーの初期体力。
pub const PLAYER_MAX_HEALTH: i32 = 5;

/// 敵の出現間隔（開始値・下限）と難易度上昇レート。
pub const ENEMY_SPAWN_INTERVAL_START: f32 = 1.6;
pub const ENEMY_SPAWN_INTERVAL_MIN: f32 = 0.45;
pub const DIFFICULTY_RAMP_PER_SEC: f32 = 0.01;
