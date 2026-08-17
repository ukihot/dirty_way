# リアルタイム・ハンドソープ表現

> **現状ステータス（2026-07-29以降）**：本ドキュメントの第1〜31節は、3Dトップダウン視点・
> Avian3D・`vec3`粒子・`Transparent3d`フェーズへのレイマーチという前提で書かれた設計記録である。
> 2026-07-29、プロジェクトは**2Dサイドビュー**（Avian2D・`vec2`・`Transparent2d`フェーズへの
> ピクセルごとの密度場直接評価）へ方針転換した。着弾判定・扁平化・メタボール融合といった
> **力学モデルの考え方そのもの**（第7〜13節、第27〜31節のFoam Aggregateモデル）は変更後も
> そのまま成立しているが、`Avian3D`という固有名詞・`vec3`という型・`Transparent3d`/レイマーチと
> いう具体的なAPI名は、以下のように読み替える必要がある。詳しい対応は末尾の
> **第32節「2Dサイドビューへの方針転換」** にまとめた。実装の一次情報源は常にコード
> （[src/soap.rs](../src/soap.rs)、[src/shaders/soap_compute.wgsl](../src/shaders/soap_compute.wgsl)、
> [src/shaders/soap_render.wgsl](../src/shaders/soap_render.wgsl)、[src/bubble.rs](../src/bubble.rs)）
> であり、本ドキュメント中のコードスニペットは設計当時の思想を示す資料として読むこと。
>
> **2026-08-17追記**：床に積む`Bubble`は飛翔・着地の手触りを担う物理層として
> 最大96個に制限する。敵を覆う「ふわふわ・もこもこ」の主役は、命中時に生成する
> 非物理の`FoamPuff` Sprite群へ移した。各敵は最大18個の泡塊だけを持ち、追従・
> 呼吸アニメーションで表現する。これにより、物理衝突数と全画面密度場の走査量を
> 上限化しつつ、「泡が付く→育つ→包み込む」というゲーム上の読みやすさを得る。
>
> | 第1〜31節の記述 | 現状（コード） |
> |---|---|
> | Avian3D | **Avian2D**（`avian2d` crate） |
> | `vec3`の位置・速度・スケール | **`vec2`**（Zは廃止、X=水平・Y=高さ） |
> | `Transparent3d`フェーズ＋レイマーチ | **`Transparent2d`**フェーズ＋ピクセルごとの密度場直接評価（奥行きが無いため） |
> | `RenderGraphSystems::Begin`でのCompute Dispatch（第23節） | 変更なし。2Dピボット後も同じ仕組みのまま有効 |

## Prototype Architecture Design

### 1. 目的

本プロトタイプは、ゲーム内でノズルから射出されるハンドソープ状の液体を、事前ベイクされたアニメーションではなく、プレイヤーの操作や環境状態に応じてリアルタイムに生成・変形・融合することを目的とする。

対象とする表現は以下である。

* ノズルの向きによって射出方向が変化する
* 押し込み強度によって射出速度・量が変化する
* 空中を飛翔する
* テーブルなどの平面に着弾する
* 着弾時に垂直方向の運動を横方向の広がりへ変換する
* 着弾した液体が扁平化する
* 複数の液体粒子がメタボール的に融合する
* 既存の液体の塊と新しい液体が自然に一体化する
* 同じ入力でも着弾位置や速度によって異なる形状になる

本プロトタイプでは、物理的に正確な流体シミュレーションは目標としない。

目標は、

> 「プレイヤーの操作に応じて毎回異なる形状を生成し、視覚的には粘性の高い液体がぬちゃっと広がる」

というゲーム体験を、可能な限り小さな実装で成立させることである。

---

# 2. 設計原則

本システムでは、以下の原則を採用する。

## 2.1 シミュレーションとレンダリングを分離する

液体の「状態」と「見た目」を同一のデータ構造に依存させない。

```text
Simulation
    Particle Position
    Particle Velocity
    Particle Scale
    Particle State
    Lifetime

          ↓

Rendering

    Metaball Field
    Surface Reconstruction
    Shading
```

粒子は液体そのものではなく、液体の状態を表現するシミュレーション要素である。

レンダリング側は粒子群からImplicit Surfaceを構築し、液体として表示する。

この分離により、将来的に以下の変更が可能になる。

* Metaball → Marching Cubes
* Metaball → 3D Density Grid
* Raymarching → Mesh Rendering
* 簡易物理 → PBF / SPH
* 粒子 → Voxel / Grid Simulation

シミュレーションとレンダリングのインターフェースを維持したまま、内部実装を交換できる。

## 2.2 GPUを粒子シミュレーションの所有者とする

粒子状態は原則としてGPU上に保持する。

CPUからGPUへ毎フレーム全粒子を転送する方式は採用しない。

望ましいデータフローは以下である。

```text
Main World
    │
    │ Spawn Request
    ▼
Render World
    │
    ▼
GPU Particle Buffer
    │
    ├── Compute Shader
    │       ├── Gravity
    │       ├── Collision
    │       ├── Impact
    │       └── Spreading
    │
    ▼
Updated Particle Buffer
    │
    ▼
Render Shader
    │
    ▼
Screen
```

CPUは「粒子そのもの」を管理するのではなく、「粒子を生成する要求」を発行する。

GPUは粒子の位置・速度・状態を継続的に保持する。

この構造により、将来的にPBFやSpatial Hashなどを導入しても、GPU上のParticle Bufferを中心とした設計を維持できる。

## 2.3 Phase 1では物理的正確性より視覚的妥当性を優先する

以下は初期実装では行わない。

* 完全なNavier-Stokes流体
* SPH
* PBF
* 粒子間衝突
* 厳密な表面張力
* 高精度なSDF
* 3D Density Grid
* Marching Cubes

これらは将来的な拡張候補とする。

Phase 1〜3では、

```text
Ballistic Particle
+
Impact Flattening
+
Viscosity-like Damping
+
Metaball Field
```

だけで視覚的な液体表現を成立させる。

---

# 3. システム全体構成

```text
                    ┌─────────────────────┐
                    │      Main World     │
                    │                     │
                    │  Input              │
                    │  Nozzle             │
                    │  SoapEmitter        │
                    └──────────┬──────────┘
                               │
                         Spawn Request
                               │
                               ▼
                    ┌─────────────────────┐
                    │     Render World    │
                    │                     │
                    │  Spawn Queue        │
                    │  GPU Resources      │
                    └──────────┬──────────┘
                               │
                               ▼
                    ┌─────────────────────┐
                    │  GPU Particle Pool  │
                    │                     │
                    │  Position           │
                    │  Velocity           │
                    │  Scale              │
                    │  State              │
                    │  Lifetime            │
                    └──────────┬──────────┘
                               │
                               ▼
                    ┌─────────────────────┐
                    │   Compute Shader    │
                    │                     │
                    │  Spawn              │
                    │  Gravity            │
                    │  Collision          │
                    │  Impact             │
                    │  Spread             │
                    │  Damping            │
                    └──────────┬──────────┘
                               │
                               ▼
                    ┌─────────────────────┐
                    │  Particle Buffer    │
                    │   GPU Resident      │
                    └──────────┬──────────┘
                               │
                               ▼
                    ┌─────────────────────┐
                    │  Metaball Renderer  │
                    │                     │
                    │  Density Field      │
                    │  Surface Detection  │
                    │  Normal            │
                    │  Lighting           │
                    └──────────┬──────────┘
                               │
                               ▼
                          Final Image
```

# 4. CPU側の責務

CPU / Bevy ECSは以下のみを担当する。

* プレイヤー入力
* ノズル位置
* ノズル方向
* 押し込み強度
* 射出量
* Spawn Request生成
* ゲームロジックとの連携

CPUは粒子の毎フレームの位置を管理しない。

CPUからGPUへ渡す情報は、概念的には以下とする。

```rust
struct SpawnRequest {
    position: Vec3,
    direction: Vec3,
    pressure: f32,
    amount: u32,
}
```

必要に応じて以下を追加する。

```rust
struct SpawnRequest {
    position: Vec3,
    direction: Vec3,
    pressure: f32,
    amount: u32,

    spread_angle: f32,
    particle_speed: f32,
}
```

重要なのは、`SpawnRequest`をGPU側の粒子データと分離することである。

# 5. GPU Particle Pool

GPU上に固定サイズのParticle Poolを確保する。

Phase 1では100〜1024粒子程度を想定する。

```text
ParticlePool
    Particle[0]
    Particle[1]
    ...
    Particle[N]
```

粒子データは概念的に以下とする。

```rust
struct Particle {
    position: Vec3,
    velocity: Vec3,
    scale: Vec3,

    state: u32,
    lifetime: f32,
}
```

必要に応じて以下を追加する。

```text
base_radius
mass
temperature
age
random_seed
```

ただしPhase 1では最小限にする。

# 6. Particle State Machine

粒子状態は以下とする。

```text
INACTIVE
    │
    │ Spawn
    ▼
FLYING
    │
    │ Table Collision
    ▼
IMPACT
    │
    │ Flatten
    ▼
SPREADING
    │
    │ Velocity < Threshold
    ▼
RESTING
```

状態遷移は以下。

```text
INACTIVE
    粒子未使用

FLYING
    重力下で飛翔

IMPACT
    テーブルへの衝突処理

SPREADING
    テーブル上で粘性により減速しながら広がる

RESTING
    ほぼ静止
```

`IMPACT`を明示的な状態として持つことで、着弾時の一時的な変形を独立して制御できる。

# 7. 射出モデル

1回の「ハンドソープ押下」を1個の粒子として扱わない。

1回の押下から複数粒子を生成する。

例えば、

```text
1 Push
    ↓
10〜30 Particles
```

とする。

各粒子には小さなランダム性を与える。

```text
Position
    = Nozzle Position
    + Random Offset

Velocity
    = Nozzle Direction * Pressure
    + Random Cone Offset
```

これにより、

```text
         ●
      ● ● ●
     ● ● ● ●
      ● ●
        ↓
```

のような液体の塊を形成する。

全粒子を完全に同一位置・同一速度にすると、レーザーのような直線的な液体になるため避ける。

# 8. 着弾モデル

テーブルを最初は単純な平面として扱う。

```text
SDF(p) = p.y - table_height
```

粒子がテーブル面に到達したら、着弾速度を取得する。

```text
impact_speed = max(-velocity.y, 0)
```

この値を扁平化へ変換する。

```text
spread = 1.0 + impact_speed * impact_factor
```

粒子の形状を、

```text
scale.x = base_scale.x * spread
scale.y = base_scale.y / spread
scale.z = base_scale.z * spread
```

のように変形させる。

目的は、

```text
高速着弾
    ↓
横に広く
縦に薄い

低速着弾
    ↓
丸く厚い
```

という視覚的関係を作ることである。

厳密な体積保存はPhase 1では要求しない。

# 9. Spreadingモデル

テーブル着弾後は、粒子間物理を使用せず、簡易的な粘性モデルで広がりを表現する。

```text
velocity *= damping
```

さらに、一定時間だけspreadを増加させる。

ただし、無限膨張を防ぐため最大値を設定する。

```text
spread = min(
    spread + spread_rate * dt,
    max_spread
)
```

または、

```text
spread → target_spread
```

への補間を使用する。

推奨は後者。

```text
spread = smooth_interpolate(
    current_spread,
    target_spread,
    dt
)
```

これにより、着弾直後の急激な扁平化から、徐々に静止する状態までを制御できる。

# 10. Metaball Rendering

Phase 1ではParticle Bufferを直接参照し、Implicit Fieldを計算する。

概念式：

```text
Density(p)
    = Σ Kernel(p, Particle_i)
```

各Particleは楕円体として扱う。

```text
local_position
    = (p - particle.position)
    / particle.scale
```

これにより、

```text
       ●
```

を、

```text
    ███████
  ███████████
    ███████
```

のような扁平形状へ変換できる。

# 11. Surface Reconstruction

Phase 1では3D Density Gridを使用しない。

Render Shader内で直接Particle Bufferを評価する。

```text
Ray
 │
 ├─ Sample Density
 │      │
 │      ├─ Particle 1
 │      ├─ Particle 2
 │      ├─ Particle 3
 │      └─ ...
 │
 ├─ Density > Threshold
 │
 └─ Surface Hit
```

これにより100〜1024粒子程度の小規模プロトタイプを最小実装で成立させる。

Raymarchingの実装は固定ステップでもよい。

ただし、Phase 1では厳密なSphere Tracingを要求しない。

# 12. Metaball融合

複数粒子のDensityを加算することで融合を表現する。

```text
Particle A      Particle B

    ●              ●

        ↓

    ███████████
```

この方式では物理的な粒子間相互作用は発生しない。

しかし、

```text
Particle Simulation
        ≠
Visual Surface
```

と割り切ることで、視覚的には液体の融合を表現できる。

Phase 1ではこの「見た目だけの融合」を採用する。

# 13. 既存の泡との融合

既存の粒子も同じParticle Pool内に保持する。

新規射出粒子が既存粒子群に近づくと、Density Field上で自動的に融合する。

```text
Existing Soap
████████

New Soap
   ● ● ●
      ↓

██████████████
██████████████
```

これにより、明示的な「液体Aと液体Bを結合する」処理は不要となる。

融合はレンダリングレイヤーのDensity Fieldによって自然に発生する。

# 14. Bevy 0.19との接続方針

Bevy側は以下の責務に分離する。

```text
Main World
    │
    ├─ Input
    ├─ Nozzle
    └─ Spawn Request
           │
           ▼
Extract
           │
           ▼
Render World
           │
           ├─ GPU Particle Buffer
           ├─ Spawn Buffer
           └─ Simulation Params
           │
           ▼
Compute Pipeline
           │
           ▼
Render Pipeline
```

重要な原則は、

> `Particle Buffer`そのものを毎フレームMain WorldからExtractしない

ことである。

Main WorldからRender Worldへ渡すのは、

* Spawn Request
* Simulation Parameters
* 必要な環境情報

に限定する。

GPU Particle BufferはRender World側で生成・保持し、Compute Shaderが更新する。

# 15. Phase 1

## 目標

100粒子程度をGPU上で飛翔させる。

```text
Nozzle
 ↓
Spawn
 ↓
Gravity
 ↓
Table Collision
 ↓
Render
```

この段階ではMetaballの融合を最低限実装する。

成功条件：

* GPU上でParticle Bufferが保持される
* Compute ShaderがParticleを更新する
* 粒子が重力で落下する
* テーブルに衝突する
* 粒子が画面に表示される

# 16. Phase 2

## 目標

着弾時の「ぬちゃっ」を作る。

追加要素：

* Impact State
* Impact Speed
* Anisotropic Scale
* Spread
* Viscosity-like Damping

成功条件：

```text
高速着弾
    ↓
大きく扁平化

低速着弾
    ↓
小さく厚い

着弾後
    ↓
徐々に広がる
    ↓
停止
```

この段階で、視覚的な「液体らしさ」を評価する。

# 17. Phase 3

## 目標

複数粒子をMetaballとして融合させる。

```text
Particle Buffer
       ↓
Density Field
       ↓
Threshold
       ↓
Surface
       ↓
Lighting
```

成功条件：

* 1回の射出で複数粒子が生成される
* 粒子が一つの液体塊に見える
* 連続射出すると既存液体と融合する
* 射出方向によって着弾位置が変わる
* 射出速度によって扁平率が変わる
* 毎回異なる形状が生成される

# 18. Phase 4以降の拡張候補

Phase 1〜3で見た目が成立した後に、必要に応じて以下を追加する。

```text
Phase 4
Particle Neighbor Interaction

Phase 5
Spatial Hash

Phase 6
PBF / SPH

Phase 7
3D Density Grid

Phase 8
GPU Surface Reconstruction

Phase 9
Dynamic Mesh / Marching Cubes

Phase 10
Advanced Material
    Refraction
    Fresnel
    Subsurface
    Thickness
    Foam
```

優先順位は、

```text
Visual Quality
    ↑
    │
    │        Material
    │       /
    │   Metaball
    │   /
    │ Spread
    │ /
    │
    └──────────────→ Simulation Complexity
             PBF
             SPH
```

とする。

物理精度を上げることが必ずしも見た目の品質向上につながるとは限らない。

# 19. 最終的な設計判断

本プロトタイプの最終的な設計方針は以下とする。

```text
CPU

Input
Nozzle
Spawn Request
Game Logic


GPU

Particle Pool
    ↓
Compute Simulation
    ↓
Metaball Rendering


Phase 1〜3

Simple Particle Physics
+
Anisotropic Flattening
+
Metaball Field
+
GPU-Owned State


Future

PBF / SPH
+
Spatial Hash
+
Density Grid
+
Advanced Surface Reconstruction
```

最初から完全な流体シミュレーションを実装しない。

まず、

> 「押す」
> ↓
> 「飛ぶ」
> ↓
> 「当たる」
> ↓
> 「ぬちゃっと潰れる」
> ↓
> 「既存の液体と融合する」

という一連のゲーム体験を成立させる。

その後、見た目上不足している部分だけを物理シミュレーションとして追加する。

この順序により、不要なPBF/SPH実装に時間を費やすことを避けつつ、将来的にGPU流体シミュレーションへ拡張可能なアーキテクチャを維持する。

---

# 20. Bevy 0.19 実装対応（検証済みAPI）

ここまでの設計は概念レベル（疑似Rust構造体・概念図）だった。本節以降は、実際にインストールされている `bevy 0.19.0` / `bevy_render 0.19.0` / `bevy_pbr 0.19.0` / `bevy_core_pipeline 0.19.0` のソース（`~/.cargo/registry/src/index.crates.io-*/bevy_render-0.19.0` 等、ローカルにベンダリング済み）を直接読んで検証した、具体的なAPI設計である。

## 20.1 前提の訂正：Bevy 0.19ではNodeベースのRenderGraphが廃止されている

Web上のBevy compute shader解説（0.12〜0.17時点のもの）の多くは、

```text
impl render_graph::Node for MyNode {
    fn run(&self, graph, render_context, world) -> Result<...>
}
RenderGraph::add_node(...)
RenderGraph::add_node_edge(...)
```

という「Nodeトレイト実装＋エッジ接続」方式を前提にしている。

しかし `bevy_render-0.19.0/src/renderer/mod.rs` を確認すると、`RenderGraph` は次のように**ただのScheduleLabel**になっている。

```rust
#[derive(ScheduleLabel, Debug, Clone, PartialEq, Eq, Hash, Default)]
pub struct RenderGraph;

impl RenderGraph {
    pub fn base_schedule() -> Schedule {
        let mut schedule = Schedule::new(Self);
        schedule.configure_sets(
            (
                RenderGraphSystems::Begin,
                RenderGraphSystems::Render,
                RenderGraphSystems::Submit,
                RenderGraphSystems::Finish,
            )
                .chain(),
        );
        schedule
    }
}
```

つまり0.19では「ノードグラフを構築する」のではなく、**通常のBevyシステムを`RenderGraph`スケジュールに追加し、`RenderGraphSystems`（`Begin → Render → Submit → Finish`）で順序付けする**方式に変わっている。`render_system`（毎フレーム1回だけ呼ばれる）が`world.run_schedule(RenderGraph)`を実行する。

`Core3d`（3Dカメラのパス一式）のような「カメラ単位で走る処理」は、`Camera3d`が持つ`CameraRenderGraph(Core3d)`コンポーネントを通じて、`camera_driver`システム（`RenderGraphSystems::Render`に属する）がビュー単位で`Core3d`スケジュールを実行する形に変わっている。`main_opaque_pass_3d_node.rs`のようにファイル名は`_node`のままだが、中身は「Nodeトレイトの実装」ではなく「`RenderContext`をシステム引数として受け取る普通の関数」になっている。

**この変更は本プロトタイプの設計にとって都合が良い。** 「粒子シミュレーションはビューに依存しない、フレームに1回だけ実行すべき処理」であるため、Nodeグラフの中に無理に組み込む必要がなく、`RenderGraphSystems::Begin`（カメラ処理より前）にシステムを1つ追加するだけで済む。

## 20.2 概念 → Bevy 0.19 実API 対応表

| 設計上の概念（第2〜19節） | Bevy 0.19での実体 | 備考 |
|---|---|---|
| Main World: Input / Nozzle / SoapEmitter | 既存の`player.rs`のシステム（Update schedule） | 変更なし |
| Spawn Request（Main→Render） | `Extract<EventReader<SpawnRequest>>`を使う`ExtractSchedule`システム | `bevy_render::{Extract, ExtractSchedule}` |
| GPU Particle Pool（GPU常駐・固定サイズ） | `RenderStartup`で1回だけ作る生の`wgpu::Buffer`（`STORAGE`使用） | CPUから毎フレーム書き込まない |
| Spawn Queue（CPU→GPU、毎フレーム小さいバッファ） | `StorageBuffer<Vec<SpawnRequestGpu>>` | `bevy_render::render_resource::StorageBuffer` |
| Compute Shader（Spawn/Gravity/Collision/Impact/Spread/Damping） | `ComputePipelineDescriptor` + `PipelineCache::queue_compute_pipeline` | `bevy_render::render_resource::{ComputePipelineDescriptor, PipelineCache}` |
| Compute Shaderの実行タイミング | `RenderGraph`スケジュールに`.in_set(RenderGraphSystems::Begin)`で追加するシステム | ビュー非依存・1フレーム1回を保証 |
| Metaball Renderer（Density Field→Surface→Lighting） | `Transparent3d`フェーズへの自前`RenderCommand`（フルスクリーン三角形のレイマーチ） | 既存の不透明ジオメトリとの深度・ブレンディングをフェーズ機構に任せる |
| Metaball Rendererの実行タイミング | `queue_metaballs`システムを`RenderSystems::Queue`に追加し、ビューごとに`Transparent3d`アイテムを1つpush | `bevy_render::{Render, RenderSystems}` |

以降の第21〜24節でこの対応表の各行を具体化する。

# 21. GPU Particle Pool の確保と初期化

Particle PoolはPhase 1で100〜1024個の固定長配列とする（第5節）。CPUはこのバッファの中身を毎フレーム読み書きしないが、**バッファの正しいバイトサイズ（ストライド）を知る必要はある**。

そこで、Rust側の`Particle`構造体には`encase::ShaderType`を導出しておき、「初期化用のゼロ埋めバイト列を1回だけ作る」ためだけに使う。以降このバッファはGPU側（Compute Shader）だけが書き換える。

```rust
use bevy::render::render_resource::{ShaderType, encase::StorageBuffer as EncaseBuffer};

#[derive(Clone, Copy, ShaderType, Default)]
struct GpuParticle {
    position: Vec3,
    velocity: Vec3,
    scale: Vec3,
    state: u32,
    lifetime: f32,
}

const PARTICLE_POOL_SIZE: usize = 1024;

// RenderStartup で1回だけ実行する。
fn init_particle_pool(mut commands: Commands, render_device: Res<RenderDevice>) {
    let zeroed = vec![GpuParticle::default(); PARTICLE_POOL_SIZE];
    let mut bytes = EncaseBuffer::new(Vec::new());
    bytes.write(&zeroed).unwrap();

    let buffer = render_device.create_buffer_with_data(&BufferInitDescriptor {
        label: Some("soap_particle_pool"),
        contents: bytes.as_ref(),
        usage: BufferUsages::STORAGE,
    });

    commands.insert_resource(ParticlePoolBuffer { buffer, capacity: PARTICLE_POOL_SIZE as u32 });
}
```

Bind Group Layoutは第20節の対応表通り、`BindGroupLayoutEntries::sequential`で組み立てる（`bevy_pbr`の`gpu_preprocess.rs`で使われているのと同じイディオム）。

```rust
use bevy::render::render_resource::binding_types::{storage_buffer, storage_buffer_read_only, uniform_buffer};

let layout_entries = BindGroupLayoutEntries::sequential(
    ShaderStages::COMPUTE,
    (
        // @group(0) @binding(0) var<storage, read_write> particles: array<Particle>;
        storage_buffer::<GpuParticle>(false),
        // @group(0) @binding(1) var<storage, read> spawn_requests: array<SpawnRequestGpu>;
        storage_buffer_read_only::<GpuSpawnRequest>(false),
        // @group(0) @binding(2) var<uniform> sim_params: SimParams;
        uniform_buffer::<SimParams>(false),
    ),
);
let layout_descriptor = BindGroupLayoutDescriptor::new("soap_compute_layout", &layout_entries);
```

`ComputePipelineDescriptor.layout`には`BindGroupLayoutDescriptor`をそのまま渡せる（`PipelineCache`が内部でキャッシュ・解決する）。実際にBind Group（インスタンス）を作るときだけ`pipeline_cache.get_bind_group_layout(&layout_descriptor)`で`BindGroupLayout`を取得する。

# 22. SpawnRequestの Extract → Prepare 経路

Main World側は既存方針（第4節）のまま、`Events<SpawnRequest>`にpushするだけでよい。

```rust
// Main World（既存のnozzleシステムから呼ぶ）
fn nozzle_fire(mut events: EventWriter<SpawnRequest>, /* ... */) {
    events.write(SpawnRequest { position, direction, pressure, amount });
}
```

`ExtractSchedule`でRender Worldへコピーする。

```rust
fn extract_spawn_requests(
    mut extracted: ResMut<ExtractedSpawnRequests>,
    mut events: Extract<EventReader<SpawnRequest>>,
) {
    extracted.0.extend(events.read().copied());
}
```

Prepare段階（`RenderSystems::PrepareResourcesFlush`が適切。バインドグループ構築より前、他のバッファ書き込みと足並みを揃えられる）で、各`SpawnRequest`を複数の`GpuParticle`スロットに展開し、Poolのどこに書き込むかを決める。

Phase 1ではAtomic Counterを使わず、**CPU（Render World）側で持つリングカーソル**で十分とする。

```rust
#[derive(Resource, Default)]
struct NextSlotCursor(u32);

fn prepare_spawn_queue(
    mut extracted: ResMut<ExtractedSpawnRequests>,
    mut cursor: ResMut<NextSlotCursor>,
    mut spawn_buffer: ResMut<SpawnQueueBuffer>, // StorageBuffer<Vec<GpuSpawnRequest>>
    render_device: Res<RenderDevice>,
    render_queue: Res<RenderQueue>,
) {
    spawn_buffer.get_mut().clear();

    for request in extracted.0.drain(..) {
        let mut rng = rand::thread_rng();
        for _ in 0..request.amount {
            let slot = cursor.0;
            cursor.0 = (cursor.0 + 1) % PARTICLE_POOL_SIZE as u32;

            spawn_buffer.get_mut().push(GpuSpawnRequest {
                target_slot: slot,
                position: request.position + random_offset(&mut rng),
                velocity: request.direction * request.pressure + random_cone(&mut rng),
            });
        }
    }

    spawn_buffer.write_buffer(&render_device, &render_queue);
}
```

リングカーソル方式は「Poolが一周する前に古い粒子がまだ画面上で目立っている」場合に上書きが起きうるが、Phase 1〜3では物理的厳密性を要求しない（第2.3節）ため許容する。将来Atomic Free-Listに置き換える際も、Compute Shader側のインターフェース（`spawn_requests`バッファ）は変わらない。

# 23. Compute Dispatch の組み込み（RenderGraphSystems::Begin）

シミュレーションはビューに依存しないため、`Core3d`のようなカメラ単位のスケジュールではなく、**フレームに1回だけ**実行されることを保証できる`RenderGraph`スケジュールの`RenderGraphSystems::Begin`に置く。`camera_driver`（描画本体）は`RenderGraphSystems::Render`に属するため、`Begin`はそれより確実に先に実行される。

```rust
fn simulate_particles(
    mut render_context: RenderContext,
    pipeline_cache: Res<PipelineCache>,
    pipelines: Res<SoapComputePipelines>,
    bind_group: Res<SoapComputeBindGroup>,
    pool: Res<ParticlePoolBuffer>,
) {
    let Some(pipeline) = pipeline_cache.get_compute_pipeline(pipelines.simulate) else {
        return; // シェーダーコンパイル待ち
    };

    let encoder = render_context.command_encoder();
    let mut pass = encoder.begin_compute_pass(&ComputePassDescriptor {
        label: Some("soap_simulate"),
        timestamp_writes: None,
    });
    pass.set_pipeline(pipeline);
    pass.set_bind_group(0, &bind_group.0, &[]);

    let workgroup_count = pool.capacity.div_ceil(64);
    pass.dispatch_workgroups(workgroup_count, 1, 1);
}
```

登録側：

```rust
render_app.add_systems(RenderGraph, simulate_particles.in_set(RenderGraphSystems::Begin));
```

`bevy_pbr`の`gpu_preprocess.rs`にある同種のCompute Dispatchシステム（`early_gpu_preprocess`等）は、あえて`Core3d`スケジュール側に登録されている。これはメッシュのカリング/前処理が「ビュー（シャドウマップ含む）ごとに結果が変わる」ためで、本プロトタイプの「泡の物理状態はビューに関係なく1つ」というケースとは性質が異なる。両者を混同して`Core3d`側に置くと、シャドウを落とすライトの数だけ毎フレーム多重にシミュレーションが進行してしまうため、`RenderGraph`直下に置くことが重要な設計判断となる。

# 24. Metaball描画パスの組み込み（Transparent3dフェーズアイテム）

自前のポストプロセスNode（画面全体を専用パスで上書きする方式）ではなく、**既存の`Transparent3d`フェーズに1個のカスタム描画アイテムとして参加する**方式を採る。理由は、テーブルやキャラクターなど既存の不透明ジオメトリとの深度テスト・アルファブレンディングを、Bevyのフェーズ機構にそのまま任せられるため。

```rust
type DrawMetaballs = (
    SetItemPipeline,
    SetSoapBindGroup<0>,
    DrawFullscreenTriangle,
);

fn queue_metaballs(
    draw_functions: Res<DrawFunctions<Transparent3d>>,
    pipeline_cache: Res<PipelineCache>,
    pipelines: Res<SoapRenderPipelines>,
    mut views: Query<(Entity, &ExtractedView)>,
    mut phases: ResMut<ViewSortedRenderPhases<Transparent3d>>,
) {
    let draw_function = draw_functions.read().id::<DrawMetaballs>();

    for (view_entity, view) in &views {
        let Some(phase) = phases.get_mut(&view_entity) else { continue };
        phase.add(Transparent3d {
            distance: 0.0, // 常に最後に近い側で描く。必要ならPool AABBから算出する
            pipeline: pipelines.metaball_pipeline,
            entity: (view_entity, MainEntity::from(Entity::PLACEHOLDER)),
            draw_function,
            batch_range: 0..1,
            extra_index: PhaseItemExtraIndex::None,
            indexed: false,
        });
    }
}

render_app.add_systems(Render, queue_metaballs.in_set(RenderSystems::Queue));
```

Bind GroupはComputeとは別レイアウトで、フラグメントシェーダーから粒子バッファとビューの深度テクスチャを読む。

```rust
let render_layout_entries = BindGroupLayoutEntries::sequential(
    ShaderStages::FRAGMENT,
    (
        // @group(0) @binding(0) var<storage, read> particles: array<Particle>;
        storage_buffer_read_only::<GpuParticle>(false),
        // @group(0) @binding(1) var scene_depth: texture_depth_2d<f32>;
        binding_types::texture_depth_2d(),
        // @group(0) @binding(2) var<uniform> view: ViewUniform; (カメラ逆行列など)
        uniform_buffer::<ViewUniform>(true),
    ),
);
```

`scene_depth`を使うことで、レイマーチが既存の不透明ジオメトリより奥まで進まないようにできる（第10〜12節の融合表現を、既存シーンの背後に描かせないためのガード）。

# 25. WGSL 具体設計

## 25.1 共有データ構造

WGSLの`vec3<f32>`はstorage/uniformアドレス空間ではアラインメント16バイト（サイズ12バイト＋パディング4バイト）になる点に注意する。Rust側は`encase::ShaderType`導出により自動的にこのレイアウトへ変換されるため、フィールド順を変えても手動でパディングを気にする必要はない。

```wgsl
struct Particle {
    position: vec3<f32>,
    velocity: vec3<f32>,
    scale:    vec3<f32>,
    state:    u32,
    lifetime: f32,
};

struct SpawnRequestGpu {
    target_slot: u32,
    position:    vec3<f32>,
    velocity:    vec3<f32>,
};

struct SimParams {
    dt:            f32,
    gravity:       f32,
    table_height:  f32,
    impact_factor: f32,
    damping:       f32,
    max_spread:    f32,
    spawn_count:   u32,
};

const STATE_INACTIVE:  u32 = 0u;
const STATE_FLYING:    u32 = 1u;
const STATE_IMPACT:    u32 = 2u;
const STATE_SPREADING: u32 = 3u;
const STATE_RESTING:   u32 = 4u;
```

## 25.2 Compute Shader スケルトン（第6〜9節のステートマシンをそのまま実装）

```wgsl
@group(0) @binding(0) var<storage, read_write> particles: array<Particle>;
@group(0) @binding(1) var<storage, read> spawn_requests: array<SpawnRequestGpu>;
@group(0) @binding(2) var<uniform> sim_params: SimParams;

@compute @workgroup_size(64)
fn simulate(@builtin(global_invocation_id) gid: vec3<u32>) {
    let index = gid.x;
    if (index >= arrayLength(&particles)) {
        return;
    }

    // 1. このスロット宛のSpawn Requestがあれば上書きしてFLYINGにする。
    for (var i = 0u; i < sim_params.spawn_count; i = i + 1u) {
        if (spawn_requests[i].target_slot == index) {
            particles[index].position = spawn_requests[i].position;
            particles[index].velocity = spawn_requests[i].velocity;
            particles[index].scale    = vec3<f32>(1.0, 1.0, 1.0);
            particles[index].state    = STATE_FLYING;
            particles[index].lifetime = 0.0;
        }
    }

    var p = particles[index];
    if (p.state == STATE_INACTIVE) {
        return;
    }

    p.lifetime = p.lifetime + sim_params.dt;

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
        p.scale = mix(p.scale, vec3<f32>(target_spread, 1.0 / target_spread, target_spread), sim_params.dt * 4.0);
        p.position = p.position + p.velocity * sim_params.dt;

        if (length(p.velocity) < 0.01) {
            p.state = STATE_RESTING;
        }
    }

    particles[index] = p;
}
```

## 25.3 Metaball Raymarch フラグメントシェーダー スケルトン（第10〜12節）

```wgsl
@group(0) @binding(0) var<storage, read> particles: array<Particle>;
@group(0) @binding(1) var scene_depth: texture_depth_2d<f32>;
@group(0) @binding(2) var<uniform> view: ViewUniform;

const DENSITY_THRESHOLD: f32 = 1.0;
const MAX_STEPS: i32 = 48;
const STEP_SIZE: f32 = 0.05;

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
        if (particles[i].state == STATE_INACTIVE) {
            continue;
        }
        sum = sum + particle_density(p, particles[i]);
    }
    return sum;
}

@fragment
fn fragment(@builtin(position) frag_coord: vec4<f32>) -> @location(0) vec4<f32> {
    let uv = frag_coord.xy / view.viewport_size;
    let scene_depth_value = textureLoad(scene_depth, vec2<i32>(frag_coord.xy), 0);
    let max_distance = depth_to_view_distance(scene_depth_value, view); // 既存ジオメトリより奥へは進ませない

    let ray = camera_ray_for_uv(uv, view); // origin + direction（第11節）
    var t = 0.0;
    var hit = false;

    for (var step = 0; step < MAX_STEPS; step = step + 1) {
        if (t >= max_distance) {
            break;
        }
        let sample_pos = ray.origin + ray.direction * t;
        if (scene_density(sample_pos) > DENSITY_THRESHOLD) {
            hit = true;
            break;
        }
        t = t + STEP_SIZE; // Phase 1では固定ステップでよい（第11節、Sphere Tracing不要）
    }

    if (!hit) {
        discard;
    }

    let hit_pos = ray.origin + ray.direction * t;
    let normal = normalize(vec3<f32>(
        scene_density(hit_pos + vec3<f32>(0.01, 0.0, 0.0)) - scene_density(hit_pos - vec3<f32>(0.01, 0.0, 0.0)),
        scene_density(hit_pos + vec3<f32>(0.0, 0.01, 0.0)) - scene_density(hit_pos - vec3<f32>(0.0, 0.01, 0.0)),
        scene_density(hit_pos + vec3<f32>(0.0, 0.0, 0.01)) - scene_density(hit_pos - vec3<f32>(0.0, 0.0, 0.01)),
    ) * -1.0);

    let light_dir = normalize(vec3<f32>(0.3, 0.8, 0.4));
    let ndotl = max(dot(normal, light_dir), 0.1);
    let base_color = vec3<f32>(0.85, 0.95, 1.0);

    return vec4<f32>(base_color * ndotl, 0.9);
}
```

`scene_density`の全粒子ループはPhase 1〜3（粒子数100〜1024）を想定した最小実装であり、第18節でいうPhase 7（3D Density Grid）以降で空間分割による高速化に置き換える候補である。

# 26. 本節のまとめ

第20〜25節で追加した内容は、第2〜19節の設計判断を変更するものではなく、**それをBevy 0.19の実APIにどう落とし込むかを具体化しただけ**である。特に重要な発見は、

* Bevy 0.19で旧来のNode/RenderGraphトレイト方式が廃止され、スケジュール＋システムセット方式に統一されたこと
* この変更により、ビュー非依存の粒子シミュレーションを`RenderGraphSystems::Begin`に素直に置けること（旧方式でグラフのノード接続を無理に組む必要がない）
* Metaball描画は専用ポストプロセスNodeを新設するより、既存の`Transparent3d`フェーズに参加する方が、深度・ブレンディング処理を再実装せずに済むこと

の3点である。実装フェーズ（Phase 1〜4、第15〜18節）に着手する際は、まず本節のプラグイン骨格（第20.2節の対応表）を`src/soap.rs`として立ち上げ、既存の`bubble.rs`とは独立に検証してから置き換えるとよい。

---

# 27. 設計の再定義：Particle-based Liquid → Macro Foam Aggregate + Microstructure

第1〜26節の実装は`doc/soap-issues.md`のS-01〜S-03で報告された通り、「泡っぽい形にはなるが毎回ほぼ同じ形状に収束する」という問題を抱えていた。原因を掘り下げると、これは単なるパラメータ調整の問題ではなく、**力学モデルそのものが「粘性のある液体・ジェル」を対象にしていた**ことに起因する。

```text
射出 → 飛翔 → 着弾 → 扁平化 → 粘性で広がる → Metaball融合
```

これはハンドソープの**液体**としては妥当だが、「ハンドソープの**泡**」は本来、

```text
大量の微細気泡 + 薄い液膜 + 液体のネットワーク + 気泡同士の変形
```

という別の力学対象である。液体は放っておけば滑らかな平衡形状に収束するため、S-01〜S-03の対症療法（`max_spread`の上限緩和、`scatter`の拡大、表面への加算ノイズ）は「均質な液体モデルに泡らしさを後付けしていただけ」であり、根本原因の解消にはなっていなかった。

## 27.1 再定義：Particle = 泡の塊（Foam Aggregate）

そこで、GPU Particleの意味を次のように再定義する。

```text
旧: Particle = 液体の小片
新: Particle = 泡の集合体の「局所的な塊」
     （数十〜数百個の微細気泡を平均化したマクロな泡構造）
```

1回の射出は、複数の独立した液体粒子（旧: 10〜30個）ではなく、**1〜数個のFoam Aggregate**として扱う。見た目の複雑さ（無数の泡が集まっているように見えること）は、粒子数を増やすのではなく、後述のMicrostructure層が担当する。

```text
旧: 1 Push → 10〜30 Particles（それぞれが独立した液滴）
新: 1 Push → 1〜数個の Foam Aggregate（内部にMicrostructureを持つ）
```

これにより、**シミュレーション解像度**（GPU上で実際に動かす塊の数）と**知覚解像度**（見た目上いくつの泡があるように見えるか）を完全に分離できる。数千個の泡を個別にシミュレーションしなくても、数千個の泡が存在するように見せられる。

## 27.2 三層分離

第2.1節の「シミュレーションとレンダリングを分離する」原則を、次のように三層へ拡張する。

```text
┌─────────────────────────────┐
│ Simulation                  │
│                              │
│ Foam Aggregate               │
│ 粘弾性・圧縮・移流・着弾      │
└──────────────┬───────────────┘
               │
               ▼
┌─────────────────────────────┐
│ Macro Surface                │
│                              │
│ Metaball / Density Field     │
│ 泡の塊の外形                  │
└──────────────┬───────────────┘
               │
               ▼
┌─────────────────────────────┐
│ Microstructure                │
│                              │
│ Bubble Pattern / Thin Film    │
│ Void / Foam Texture           │
└─────────────────────────────┘
```

第2.1節の原則（`Particle Simulation ≠ Visual Surface`）を、

> **Simulation ≠ Macro Surface ≠ Microstructure**

まで拡張する。Macro Surfaceは既存のMetaball/Density Fieldをそのまま流用できる。Microstructureは、密度場を「加算」するのではなく、大きな塊の内部から気泡状のボイドを「減算」で刳り抜くイメージで、概念的には

```text
FoamField(p) = LiquidFilmField(p) − BubbleVoidField(p)
```

と表現できる（第29節Phase 4で具体化する。現行実装のS-03ノイズは、この層の暫定的な代用品にすぎない）。

## 27.3 Foam Aggregateの内部状態

```rust
struct FoamAggregate {
    position: Vec3,
    velocity: Vec3,
    scale: Vec3,

    bubble_size: f32,
    bubble_density: f32,
    void_fraction: f32,

    anisotropy: Vec3,
    random_seed: u32,

    state: u32,
}
```

第8節のAnisotropic Scale（着弾による扁平化）はそのまま残す。ただし解釈を「着弾速度→運動エネルギー→横方向への広がり」から「圧縮・せん断→気泡構造の再配列→集合体の扁平化」に読み替える。実装（WGSLの数式）は変わらない。

# 28. Avian3Dによるマクロ力学の分離

第27節の三層のうち、Simulation層をさらに分離する。**「泡そのものをシミュレーションする」のではなく、「泡集合体のマクロな力学（位置・速度・接触）」をAvian3Dに任せ、GPU Compute Shaderは「その結果、集合体がどう変形するか」というレオロジー変換だけを担当する**。

```text
┌──────────────┐
│   Bevy ECS   │
└──────┬───────┘
       │
   Spawn Foam
       │
       ▼
┌──────────────┐
│  Avian 3D    │
│              │
│ RigidBody    │
│ Collider     │
│ Contact      │
└──────┬───────┘
       │
Contact Events
       │
       ▼
┌─────────────────────┐
│ Foam Aggregate State│
│                      │
│ Compression          │
│ Shear                │
│ Plasticity            │
│ Spread                │
│ Void Fraction          │
└──────────┬───────────┘
       │
       ▼
   GPU Buffer
       │
┌──────┴──────┐
│             │
Macro Shape   Microstructure
│             │
Density Field  Bubble Pattern
│             │
Metaball/SDF   Foam Noise
│             │
└──────┬──────┘
       ▼
   Rendering
```

Avianの`RigidBody`をそのまま描画対象にしない。Avianは「この泡の塊は今どこにいて、どういう速度で、何にぶつかっているか」だけを解く。GPU側は「その結果、集合体はどういう形に変形しているか」を解く。レンダリング側は「その形状をどう泡に見せるか」を解く。単なる「Avian＋Metaball」の組み合わせではなく、**接触力学から泡のレオロジーへの変換モデル**として設計する。

## 28.1 既存コードとの接続

`bubble.rs`はゲームプレイの当たり判定として既にAvian3Dの`RigidBody::Dynamic`＋`Collider::sphere`を持っている。第20〜26節までの実装は、この存在を知らずに`soap_compute.wgsl`が重力積分・地面接触判定を独自に再実装していた（`SimParams.gravity`が`main.rs`の`Gravity`リソースと無関係に`14.0`という別の定数として存在していた。値はたまたま一致していたが、2つの真実の源が独立に存在する状態で、片方だけ変更すればズレる潜在バグだった。`doc/soap-issues.md`のS-09として記録する）。

この節の設計は、**`bubble.rs`のAvianボディそのものをFoam Aggregateのマクロ状態の一次情報源にする**ことを意味する。すなわち、

* 1個のゲームプレイBubbleエンティティ = 1個のFoam Aggregate
* Compute Shaderは自前の重力積分・FLYING状態の位置更新をやめ、Avianが解いた`Transform`/`LinearVelocity`を毎フレーム受け取ってそのまま採用する
* 地面接触の判定は（Phase 1では簡略化のため）引き続き`position.y <= table_height`のシェーダー内チェックで行う。Avianの`CollisionStart`/`Collisions`（既に`bubble.rs`がbubble-enemy判定で使っている仕組み）から接触インパルスを取る拡張は、将来Phase（第29節Phase 2以降の精緻化）の候補として残す

## 28.2 唯一新しく必要になる仕組み：永続的なEntity⇔GPUスロット対応

第22節のリングカーソル方式は「1回のSpawn Requestをその場でスロットに割り振って終わり」という**一回性**の仕組みだった。Avian駆動にすると、生きているBubbleエンティティの位置・速度を**毎フレーム継続的に**GPU側へ送る必要があるため、「どのMain WorldエンティティがどのGPUスロットに対応しているか」を複数フレームにわたって保持する対応表が必要になる。

```rust
#[derive(Resource)]
struct FoamSlotAllocator {
    slot_of_entity: HashMap<Entity, u32>,
    free_slots: Vec<u32>,
}
```

* 新しく見つかったBubbleエンティティ（前フレームまで存在しなかった）→ `free_slots`から1つ払い出して対応表に登録（＝新規スポーン）
* 前フレームまで存在したが今フレーム消えたBubbleエンティティ（despawn済み）→ 対応するスロットを`free_slots`へ返却

これは第14節の「Particle Bufferそのものを毎フレームExtractしない」という原則に反しない。原則が禁じているのは256〜512スロットの**プール全体**を毎フレーム転送することであり、同時に飛んでいる（実際にはせいぜい数個の）Bubbleエンティティの位置・速度だけを送るのは、既存の`SoapSpawnRequest`（1回の発射イベント）を「継続的なDrive更新」に置き換えるだけで、規模としては変わらない。

なお、`bubble.rs`側の寿命（`BUBBLE_LIFETIME`）とGPU側の寿命（`MAX_LIFETIME`）は同じ値・同じ起点（スポーン時刻）でカウントされるため、Bubbleエンティティがdespawnするタイミングと、対応するGPU粒子が自然にフェードアウトし終える（第8.3項、S-08）タイミングはほぼ一致する。そのためスロットは対応するBubbleエンティティが消えた瞬間に即座に回収してよい。

# 29. 改訂後のPhase計画

第15〜18節のPhase計画を、次のように置き換える。

```text
Phase 1
Foam Aggregate 基盤
    Avian駆動のposition/velocity抽出
    永続的なEntity⇔GPUスロット対応表
    Compute Shaderから自前の重力積分を除去

Phase 2
Aggregate Deformation
    Avianが解いた着弾速度→圧縮・せん断
    扁平化・広がり（レオロジー変換）
    ※第8〜9節の数式は変更なし、解釈と入力元だけ変わる

Phase 3
Macro Fusion
    Density Field / Metaball
    ※第10〜12節のまま。変更不要

Phase 4
Microstructure
    Bubble Void / Foam Texture / Thin Film
    第27.2節のFoamField = LiquidFilmField − BubbleVoidField を実装する
    現行のS-03加算ノイズを、減算ボイドへ拡張する形で置き換える
    → 2026-07-30、doc/soap-issues.md S-33として簡略版を先行実装した。
      正式なBubbleVoidFieldの「減算」ではなく、Worley風セルノイズによる
      粒状の凹凸を密度へ加算する近似（`bubble_microstructure`、
      soap_render.wgsl）。デフォルト品質（Normal）でも効くようにし、
      「艶々のジェル玉」から「無数の気泡が集まった泡」への見た目の
      改善を優先した。真のFoamField減算モデルへの置き換えは未着手のまま
      Phase 4として残る。

Phase 5+
Bubble Dynamics（将来候補）
    Coalescence / Drainage / Bubble Rearrangement
    Macro PBD / XPBD による粘弾塑性体としての挙動
```

第18節にあった「Phase 4以降 = PBF/SPH」というロードマップは撤回する。泡の集合体は液体のように流れる一方、低応力では固体的に形を保つため、単純なSPHよりも**粘弾塑性体（Macro PBD/XPBD）**として扱う方が自然という判断による。PBF/SPHは必須の次ステップではなく、Phase 5以降の一候補にとどめる。

# 30. 第27〜29節のまとめ

* 「毎回同じ形になる」問題の根本原因は、パラメータではなく力学モデル（液体 vs 泡）の不一致だった
* `Particle = 液体の小片`を`Particle = Foam Aggregate（泡の集合体の局所的な塊）`に再定義し、Simulation / Macro Surface / Microstructureの三層に分離する
* Foam Aggregateのマクロな力学（位置・速度・接触）はAvian3Dに一本化する。`bubble.rs`の既存Avianボディがそのまま一次情報源になり、`soap_compute.wgsl`が独自に持っていた重力定数の二重管理（S-09）が解消される
* 新たに必要になるのは、Bubbleエンティティ⇔GPUスロットの永続対応表のみ。Extract自体は「1回のSpawn Request」から「毎フレームのDrive更新」に変わるが、転送量は同時に生きているBubble数のオーダーのままで、第14節の原則（プール全体を毎フレーム転送しない）には反しない
* 第8〜12節（扁平化・広がり・Metaball）の数式はそのまま流用できる。実装コストが大きいのはPhase 1（Entity⇔スロット対応表）とPhase 4（Microstructure）の2箇所に限られる

---

# 31. Phase 1実装の精緻化：命名・所有権・Phase 1という前提の明示

第28.2節で導入した「Entity⇔スロット対応表」を実装する過程で、3つの改善点が明らかになった。

## 31.1 命名：「GPU Particle Pool」→「GPU Foam Instance Pool」

「GPU Particle Pool」という名前は、GPU上の粒子が物理状態の主体であるというニュアンスを引きずっている。第28節の設計では、位置・速度の物理的な真実はAvian3D（Main World）が持ち、GPU上の1スロットは「1個のFoam Aggregateをレンダリングするための状態（変形・寿命・世代番号）」でしかない。そこで名称を次のように改める。

```text
旧: GPU Particle Pool          新: GPU Foam Instance Pool
旧: Particle（WGSL構造体）      新: FoamInstance
旧: GpuParticle（Rust構造体）   新: FoamInstance
```

「1スロット＝1泡粒子」ではなく「1スロット＝1つのFoam Aggregateの見た目のインスタンス」であることをコード上の名前でも表現する。

## 31.2 所有権：Entity⇔Slot対応はEntity自身に持たせる

第28.2節では「Entity⇔スロット対応表」をRender World側のResource（`slot_of_entity: HashMap<Entity, u32>`）として設計した。動作はするが、ECS的にはもう一歩自然な設計がある。

```text
旧: Resource（FoamSlotAllocator）が「どのEntityがどのSlotか」を丸ごと持つ
新: Entity自身が「自分のSlot」をComponentとして持つ
    Resourceは「空きSlotのリスト」だけを持つ
```

```rust
// Bubble Entity（Main World）
Bubble Entity
├─ Bubble
├─ RigidBody
├─ Collider
└─ FoamGpuBinding { slot: 37, generation: 12 }

// Resource（Main World）
struct FoamSlotAllocator {
    free_slots: Vec<u32>,
    next_generation: u32,
}
```

「EntityとSlotの対応そのもの」はEntityに所有させ、「空きSlotの管理」だけをResourceに持たせる方が、ECS的に自然（データの所有者が単一になる）。

ここで重要な制約がある。**この割当（`allocate()`）はMain World側で行わなければならない。** Render WorldはExtractを通じてMain Worldのデータを読み取れるだけで、Main World側のEntityにComponentを書き込むことはできない（`Extract<Query<...>>`は読み取り専用）。第28.2節の設計はスロット割当をRender World側（`prepare_foam_drive_queue`内の`HashMap`によるreconcile処理）で行っていたため、動作はするものの、本質的にはMain World側で完結できる責務をRender World側に置いてしまっていた。

割当をMain World（`bubble.rs`の`spawn_bubble`が呼ばれた瞬間）に移すと、次のように単純化される。

```text
Spawn
  ↓
allocator.allocate() → FoamGpuBinding { slot, generation }
  ↓
commands.spawn((Bubble, ..., FoamGpuBinding))

Update（毎フレーム）
  ↓
Extract<Query<(&Transform, &LinearVelocity, &Bubble, &FoamGpuBinding)>>
  ↓
そのままGPUへ転送（Render World側にreconcile処理は不要）

Despawn
  ↓
Option<&FoamGpuBinding> を見て allocator.release()
```

Render World側（`soap.rs`）はスロット管理から完全に解放され、「Extractした状態をそのままGPUのDrive Queueへ流す」だけになる。`FOAM_INSTANCE_POOL_SIZE`という同じ定数を、Main World側の割当（`bubble.rs`）とRender World側のバッファ確保（`soap.rs`）の両方で共有する必要があるため、`consts.rs`に置いて一元化する（S-09と同じ「2箇所に独立した定数を置かない」という教訓の適用）。

## 31.3 「1 Bubble Entity = 1 Foam Aggregate」はPhase 1の前提であり物理的真理ではない

第27.1節以降、「1個のBubbleエンティティ＝1個のFoam Aggregate」という対応を暗黙に使ってきたが、これは**物理的な真理としてではなく、Phase 1におけるマクロ表現の単位という前提として明示する**べきである。

物理的には、Foam Aggregate（泡の塊）を1個の剛体として扱うのは近似にすぎない。将来的に「1個のAggregateが複数のRigidBodyから構成される」「1個のRigidBodyが複数のAggregateに分裂する」といった表現力が必要になれば、1 Aggregate = 1 RigidBodyという対応では足りなくなる。現時点でこの制約を設計の隅々に固定的に埋め込まない（例えば`FoamGpuBinding`をBubbleに1個だけ持たせる、という実装は変えないが、将来「1 Bubbleが複数のFoamGpuBindingを持つ」拡張がありうることを排除しない）。

## 31.4 課題S-10：スロット再利用時の変形状態残留と、その対策としての世代番号

第28.2節では「BubbleのdespawnタイミングとGPU粒子のフェード完了タイミングはほぼ一致するので、スロットは即座に回収してよい」としていたが、これは**タイミングが一致する場合の話であり、保証ではない**。特にリスタート（`state.rs`の`reset_game`）は、生存中のBubbleを寿命を待たずに一斉despawnさせるため、まだ大きく扁平化した状態のFoam Instanceのスロットが即座に解放され、直後に発射された新しいBubbleに再割当される。

このとき、GPU側（`soap_compute.wgsl`）の新規スポーン判定が`state == STATE_INACTIVE`だけだと、再利用されたスロットの`state`はまだ`STATE_SPREADING`や`STATE_RESTING`のままなので「既存の変形済みFoam Instance」として扱われ続けてしまう。結果、リスタート直後に発射した泡が、着弾もしていないのに最初から扁平に潰れた見た目で飛んでいく、という不具合になる（`doc/soap-issues.md` S-10）。

対策として、`FoamGpuBinding`に`generation: u32`を持たせる。スロットを割り当てるたびに`FoamSlotAllocator`内の単調増加カウンタから新しい世代番号を払い出し、Drive Entryに含めてGPUへ送る。GPU側（`FoamInstance`にも同じ`generation`フィールドを持たせる）は、

```text
新規スポーンとみなす条件:
    state == INACTIVE
    または
    drive_entry.generation != instance.generation
```

という判定に変える。これにより、「スロット番号は同じだが論理的には別のBubble」を確実に区別でき、despawn時に明示的な後始末（GPU側へのリセット通知）を送る必要がなくなる。世代番号という1つの`u32`フィールドを毎フレームのDrive Entryに乗せるだけで、スロット再利用に関する正しさが保証される。

## 31.5 本設計の中心原則（再掲）

本設計の本質は、泡を高解像度で物理シミュレーションすることではない。

> 泡集合体のマクロな運動をAvian3Dで解き、その結果をGPU上のレオロジー変換へ入力し、さらにMacro SurfaceとMicrostructureを分離することで、**シミュレーション解像度と知覚解像度を意図的に分離する**こと。

これにより、数千個の微細気泡を個別にシミュレーションすることなく、泡集合体としての「圧縮・せん断・広がり・融合」と、微細気泡としての「多孔性・薄膜・不均一性」を同時に表現できる。

```text
Avian3D
    泡を解くのではなく、泡が存在する世界との力学的相互作用を解く

GPU Compute
    その結果を泡集合体の内部状態（レオロジー）へ変換する

Renderer
    その状態を人間が泡として知覚する表現へ変換する
```

この三段階の分離こそが、本設計の中心原則である。第20〜26節（Bevy 0.19実API対応）や第31節（命名・所有権・世代番号）で行った変更は、いずれもこの原則をBevy/Avian3D/WGSLの実装へ落とし込む過程での具体化であり、原則そのものを変更するものではない。

---

# 32. 2026-07-29 追記：2Dサイドビューへの方針転換

第1〜31節はすべて3Dトップダウン視点（Avian3D・`vec3`・`Transparent3d`へのレイマーチ）を前提に
書かれていた。2026-07-29、実機での見た目確認（`doc/soap-issues.md` S-16以降）を経て、
**ハンドソープ筐体を真横から見た2Dサイドビュー**へアーキテクチャごと方針転換した。第31.5節の
「中心原則」（Simulation ≠ Macro Surface ≠ Microstructure、シミュレーション解像度と知覚解像度の分離）
はそのまま維持されており、変わったのは視点と次元、それに伴う具体的なAPIだけである。

## 32.1 変更の理由

3Dトップダウンでは「円形ステージを真上から見る」構図だったため、泡の高さ方向（重力方向）は
画面の奥行きとして表現され、レイマーチで奥行き方向にサンプリングする必要があった。しかし
ゲーム全体が2Dサイドビュー（`README.md`参照：床の左右端から敵が迫る構成）へ切り替わったことで、
泡はカメラのビュー平面上に直接乗る2D表現で十分になった。奥行きのレイマーチは不要になり、
ピクセルごとにワールド座標を1回だけ求めて密度場を評価すれば、それがそのまま表面判定になる
（`soap_render.wgsl`冒頭のコメント参照）。

## 32.2 概念 → 現状（2D実装）対応表

第20.2節の対応表を、現状に合わせて次のように更新する。

| 設計上の概念（第2〜19節） | 3D時代の実体（第20〜26節） | 現状（2D、2026-07-29〜） |
|---|---|---|
| マクロ力学の一次情報源 | Avian3D（`RigidBody`/`Collider`が`vec3`） | **Avian2D**（`avian2d` crate、`RigidBody`/`Collider`は`vec2`平面。[src/bubble.rs](../src/bubble.rs)） |
| GPU Particle Pool | `GpuParticle { position: vec3, velocity: vec3, scale: vec3, ... }` | **`FoamInstance { position: vec2, velocity: vec2, scale: vec2, ..., base_radius: f32, generation: u32, landing: u32 }`**（[src/shaders/soap_compute.wgsl](../src/shaders/soap_compute.wgsl)） |
| Compute Dispatchのタイミング | `RenderGraphSystems::Begin` | **変更なし**。2Dピボット後も同じ仕組みのまま（[src/soap.rs](../src/soap.rs) `simulate_foam_instances`） |
| Metaball Renderer | `Transparent3d`フェーズへのカスタム`RenderCommand`＋カメラレイのレイマーチ（`MAX_STEPS`固定ステップ） | **`Transparent2d`**フェーズへのカスタム`RenderCommand`＋ピクセルごとにワールド座標を1回だけ求める直接評価（奥行きが無いのでマーチ不要。[src/soap.rs](../src/soap.rs) `queue_soap_metaballs`、[src/shaders/soap_render.wgsl](../src/shaders/soap_render.wgsl)） |
| 着地判定 | シェーダー内で`position.y <= table_height`のY座標比較（自前） | **Main World（Avian2D）側の権威**。`bubble::LandingSurface`（Flying/Floor/Pile）をDrive Entry経由で毎フレーム受け取るだけ（`doc/soap-issues.md` S-24） |
| 融合（Metaball） | 密度の単純加算のみ | 密度の加算に加え、コア外側に弱い「橋渡し」の裾野（`MERGE_REACH`/`BRIDGE_STRENGTH`）を追加し、離れた泡同士も近づけば融合して見えるようにした（`doc/soap-issues.md` S-20） |

## 32.3 第14節・第20〜26節の扱いについて

第14節（Bevy 0.19との接続方針）・第20〜26節（GPU Particle Poolの確保、Extract→Prepare経路、
Compute Dispatch、Metaball描画パス、WGSL具体設計）は、`vec3`/`Avian3D`/`Transparent3d`という
語句を`vec2`/`Avian2D`/`Transparent2d`に読み替えれば、**アーキテクチャの構造としては現状の
実装とほぼ一致する**（Compute Shaderをビュー非依存の`RenderGraphSystems::Begin`に置く、
Metaball描画を専用ポストプロセスではなく既存の透過フェーズに1アイテムとして参加させる、
という2つの重要な設計判断はどちらも[src/soap.rs](../src/soap.rs)にそのまま残っている）。
一方、各節のWGSL/Rustコードスニペットそのもの（フィールドの型、フェーズ名、レイマーチの
ループ構造など）は設計当時のものであり、現状の正確な実装は上記32.2節の対応表からリンクした
実ファイルを参照すること。

## 32.4 まとめ

- 3D→2Dの方針転換は、第31.5節の中心原則（Simulation ≠ Macro Surface ≠ Microstructure）を
  変更するものではなく、それを実現する次元とAPIを差し替えただけである。
- Avian3D→Avian2D、`vec3`→`vec2`、レイマーチ→直接評価という3点が、コード上で確認できる
  具体的な変更点のすべてである。
- 方針転換に伴って新たに見つかった課題（融合の輪郭が硬い・浅い角度の着地で扁平化しない等）は
  `doc/soap-issues.md`のS-16〜S-32として記録されている。
