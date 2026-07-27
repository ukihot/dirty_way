# リアルタイム・ハンドソープ表現

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
