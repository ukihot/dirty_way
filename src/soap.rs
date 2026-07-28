//! リアルタイム・ハンドソープ表現（doc/soap-model.md、特に第27〜28,31節）。
//!
//! `bubble.rs`
//! はゲームロジック（Avian3Dによる当たり判定・ダメージ・足止め）に加えて、
//! GPU側スロットの割当（`FoamSlotAllocator`）と「どのBubbleがどのスロットか」という
//! 対応（`FoamGpuBinding`コンポーネント）も持つ。
//! この`FoamGpuBinding`が付いている Bubbleエンティティを毎フレーム観測し、
//! GPU常駐のFoam Instance Poolを
//! コンピュートシェーダーで「着弾扁平化→広がり」というレオロジー（見た目の変形）
//! だけ更新し、Metaballレイマーチで「ぬちゃっと広がる泡」として描画するのがこのファイル。
//!
//! 重力・地面接触の物理そのものはAvian3D（`bubble.rs`）が一元的に解く。
//! 以前はここで 重力を独自に積分しており、`main.
//! rs`のGravityリソースと無関係な別の重力定数を 抱えてしまっていた（doc/
//! soap-issues.md S-09）。CPUは毎フレーム全粒子を転送しない （doc第2.1〜2.
//! 2節）——ただし「毎フレーム転送しない」のはPoolの全スロットの話であり、
//! 実際に生きているBubbleエンティティ（せいぜい数個）の位置・
//! 速度を毎フレーム送るのは 許容範囲内である（doc第28.2節）。
//!
//! GPU側の1スロットは「泡粒子」ではなく「1個のFoam Aggregateの見た目の状態」を
//! 保持するので、`GpuParticle`/
//! `Particle`ではなく`FoamInstance`と呼ぶ（doc第31節）。

use avian3d::prelude::*;
use bevy::core_pipeline::core_3d::{CORE_3D_DEPTH_FORMAT, Transparent3d, TransparentSortingInfo3d};
use bevy::ecs::system::SystemParamItem;
use bevy::ecs::system::lifetimeless::SRes;
use bevy::prelude::*;
use bevy::render::render_phase::{
    AddRenderCommand, DrawFunctions, PhaseItem, PhaseItemExtraIndex, RenderCommand,
    RenderCommandResult, SetItemPipeline, TrackedRenderPass, ViewSortedRenderPhases,
};
use bevy::render::render_resource::encase::StorageBuffer as EncaseBuffer;
use bevy::render::render_resource::{binding_types, *};
use bevy::render::renderer::{
    RenderContext, RenderDevice, RenderGraph, RenderGraphSystems, RenderQueue,
};
use bevy::render::sync_world::MainEntity;
use bevy::render::view::{ExtractedView, ViewTarget};
use bevy::render::{Extract, ExtractSchedule, Render, RenderApp, RenderStartup, RenderSystems};

use crate::bubble::{Bubble, FoamGpuBinding};
use crate::consts::FOAM_INSTANCE_POOL_SIZE;
use crate::quality::{FoamQuality, FoamQualitySetting};

pub struct SoapPlugin;

impl Plugin for SoapPlugin {
    fn build(&self, app: &mut App) {
        let compute_handle = {
            let mut shaders = app.world_mut().resource_mut::<Assets<Shader>>();
            shaders.add(Shader::from_wgsl(
                include_str!("shaders/soap_compute.wgsl"),
                "soap_compute.wgsl",
            ))
        };
        let render_handle = {
            let mut shaders = app.world_mut().resource_mut::<Assets<Shader>>();
            shaders.add(Shader::from_wgsl(
                include_str!("shaders/soap_render.wgsl"),
                "soap_render.wgsl",
            ))
        };
        app.insert_resource(SoapShaderHandles { compute: compute_handle, render: render_handle });
    }

    fn finish(&self, app: &mut App) {
        let shaders = app.world().resource::<SoapShaderHandles>().clone();
        let Some(render_app) = app.get_sub_app_mut(RenderApp) else { return };

        render_app
            .insert_resource(shaders)
            .init_resource::<ExtractedFoamAggregates>()
            .init_resource::<ExtractedFoamQuality>()
            .init_resource::<FoamDriveQueue>()
            .init_resource::<ActiveSlotBuffer>()
            .init_resource::<SoapSimParamsBuffer>()
            .init_resource::<SoapViewUniformBuffer>()
            .add_systems(ExtractSchedule, (extract_foam_aggregates, extract_foam_quality))
            .add_systems(RenderStartup, init_soap_render_resources)
            .add_systems(
                Render,
                (
                    prepare_soap_view_uniform.in_set(RenderSystems::PrepareResources),
                    prepare_foam_drive_queue.in_set(RenderSystems::PrepareResources),
                    prepare_soap_bind_groups.in_set(RenderSystems::PrepareBindGroups),
                    queue_soap_metaballs.in_set(RenderSystems::Queue),
                ),
            )
            .add_systems(RenderGraph, simulate_foam_instances.in_set(RenderGraphSystems::Begin))
            .add_render_command::<Transparent3d, DrawSoapMetaballs>();
    }
}

#[derive(Resource, Clone)]
struct SoapShaderHandles {
    compute: Handle<Shader>,
    render: Handle<Shader>,
}

// ---------------------------------------------------------------------------
// GPU側データレイアウト（doc第25.1,31節と1:1対応。フィールド順を変えない）。
// ---------------------------------------------------------------------------

/// GPU常駐のFoam Instance Pool 1スロット分。「泡粒子」ではなく「1個の
/// Foam Aggregateの見た目の変形状態」を表す（doc第31節）。
#[derive(Clone, Copy, ShaderType, Default)]
struct FoamInstance {
    position: Vec3,
    velocity: Vec3,
    scale: Vec3,
    state: u32,
    lifetime: f32,
    base_radius: f32,
    /// このスロットへの「今回の」割当を識別する世代番号。`FoamGpuBinding`参照。
    generation: u32,
}

/// 「1回きりのスポーン要求」ではなく「今このBubbleはここにいる」という
/// 毎フレームのDrive更新（doc第28.2節）。
#[derive(Clone, Copy, ShaderType)]
struct GpuDriveEntry {
    target_slot: u32,
    position: Vec3,
    velocity: Vec3,
    base_radius: f32,
    generation: u32,
}

#[derive(Clone, Copy, ShaderType, Default)]
struct SimParams {
    dt: f32,
    table_height: f32,
    impact_factor: f32,
    max_spread: f32,
    drive_count: u32,
}

#[derive(Clone, Copy, ShaderType, Default)]
struct SoapViewUniform {
    clip_from_world: Mat4,
    world_from_clip: Mat4,
    camera_world_position: Vec3,
    /// Foam Quality（doc/soap-issues.md S-11a）に応じたレイマーチのステップ数。
    raymarch_steps: u32,
    /// MicrostructureQuality::as_u32()
    /// のエンコードと対応（soap_render.wgsl参照）。
    microstructure_quality: u32,
    /// `active_slots`（下記）の実際に有効な要素数（課題S-12）。
    active_count: u32,
}

// ---------------------------------------------------------------------------
// Main World → Render World（Extract）：FoamGpuBindingを持つBubbleを毎フレーム
// 観測する。スロット割当そのものはMain World側（bubble.rs）が既に済ませて
// いるので、ここでは「今のスロット番号」を読み取るだけでよい（doc第31節）。
// ---------------------------------------------------------------------------

struct ExtractedAggregate {
    slot: u32,
    generation: u32,
    position: Vec3,
    velocity: Vec3,
    radius: f32,
}

#[derive(Resource, Default)]
struct ExtractedFoamAggregates(Vec<ExtractedAggregate>);

fn extract_foam_aggregates(
    mut extracted: ResMut<ExtractedFoamAggregates>,
    bubbles: Extract<Query<(&Transform, &LinearVelocity, &Bubble, &FoamGpuBinding)>>,
) {
    extracted.0.clear();
    for (transform, velocity, bubble, binding) in &bubbles {
        extracted.0.push(ExtractedAggregate {
            slot: binding.slot,
            generation: binding.generation,
            position: transform.translation,
            velocity: velocity.0,
            radius: bubble.radius,
        });
    }
}

/// 現在のFoam Quality設定（doc/soap-issues.md S-11a）。レンダリング精度
/// （レイマーチのステップ数・Microstructureの詳細度）はここから決まる。
/// 同時Aggregate数の上限はMain World側（`bubble.rs`の`FoamSlotAllocator`）で
/// 既に適用済みなので、Render World側で改めて絞り込む必要はない。
#[derive(Resource, Default, Clone, Copy)]
struct ExtractedFoamQuality(FoamQuality);

fn extract_foam_quality(
    mut extracted: ResMut<ExtractedFoamQuality>,
    quality: Extract<Res<FoamQualitySetting>>,
) {
    extracted.0 = quality.0;
}

// ---------------------------------------------------------------------------
// Render World常駐GPUリソース（RenderStartupで1回だけ作る。doc第21節）。
// ---------------------------------------------------------------------------

#[derive(Resource)]
struct SoapGpuResources {
    foam_instance_pool: Buffer,
    pool_capacity: u32,
    compute_layout: BindGroupLayoutDescriptor,
    render_layout: BindGroupLayoutDescriptor,
    compute_pipeline: CachedComputePipelineId,
    /// 実際のビュー出力フォーマットが分かるQueue段階まで遅延生成する（Phase 1:
    /// 単一カメラ前提）。
    render_pipeline: Option<CachedRenderPipelineId>,
    render_shader: Handle<Shader>,
}

fn init_soap_render_resources(
    mut commands: Commands,
    render_device: Res<RenderDevice>,
    pipeline_cache: Res<PipelineCache>,
    shaders: Res<SoapShaderHandles>,
) {
    // Foam Instance Pool: ゼロ初期化したGPU専有バッファ。以後CPUからは書き込まない
    // （encaseは正しいバイトサイズ・パディングを求めるためだけに使う）。
    let zeroed = vec![FoamInstance::default(); FOAM_INSTANCE_POOL_SIZE as usize];
    let mut init_bytes = EncaseBuffer::new(Vec::new());
    init_bytes.write(&zeroed).expect("failed to size soap foam instance pool");
    let foam_instance_pool = render_device.create_buffer_with_data(&BufferInitDescriptor {
        label: Some("soap_foam_instance_pool"),
        contents: init_bytes.as_ref(),
        usage: BufferUsages::STORAGE,
    });

    let compute_layout_entries = BindGroupLayoutEntries::sequential(
        ShaderStages::COMPUTE,
        (
            // @group(0) @binding(0) var<storage, read_write> foam_instances: array<FoamInstance>;
            binding_types::storage_buffer::<FoamInstance>(false),
            // @group(0) @binding(1) var<storage, read> drive_entries: array<DriveEntry>;
            binding_types::storage_buffer_read_only::<GpuDriveEntry>(false),
            // @group(0) @binding(2) var<uniform> sim_params: SimParams;
            binding_types::uniform_buffer::<SimParams>(false),
        ),
    );
    let compute_layout =
        BindGroupLayoutDescriptor::new("soap_compute_layout", &compute_layout_entries);

    let render_layout_entries = BindGroupLayoutEntries::sequential(
        ShaderStages::FRAGMENT,
        (
            // @group(0) @binding(0) var<storage, read> foam_instances: array<FoamInstance>;
            binding_types::storage_buffer_read_only::<FoamInstance>(false),
            // @group(0) @binding(1) var<uniform> view: SoapView;
            binding_types::uniform_buffer::<SoapViewUniform>(false),
            // @group(0) @binding(2) var<storage, read> active_slots: array<u32>;
            // 課題S-12：poolの512スロット全部ではなく、実際に生きているAggregateの
            // スロット番号だけを並べたリスト。レイマーチ側のループ回数を
            // 「常に512」から「今アクティブな数だけ」に減らすためのもの。
            binding_types::storage_buffer_read_only::<u32>(false),
        ),
    );
    let render_layout =
        BindGroupLayoutDescriptor::new("soap_render_layout", &render_layout_entries);

    let compute_pipeline = pipeline_cache.queue_compute_pipeline(ComputePipelineDescriptor {
        label: Some("soap_simulate_pipeline".into()),
        layout: vec![compute_layout.clone()],
        shader: shaders.compute.clone(),
        shader_defs: vec![],
        entry_point: Some("simulate".into()),
        zero_initialize_workgroup_memory: true,
        immediate_size: 0,
    });

    commands.insert_resource(SoapGpuResources {
        foam_instance_pool,
        pool_capacity: FOAM_INSTANCE_POOL_SIZE,
        compute_layout,
        render_layout,
        compute_pipeline,
        render_pipeline: None,
        render_shader: shaders.render.clone(),
    });
}

// ---------------------------------------------------------------------------
// 毎フレームのPrepare。スロット割当はMain World側（bubble.rs）が既に済ませて
// いるので、ここはExtractした状態をそのままGPUへ転送するだけ（doc第31節）。
// ---------------------------------------------------------------------------

#[derive(Resource, Default)]
struct FoamDriveQueue(StorageBuffer<Vec<GpuDriveEntry>>);

/// 課題S-12：レイマーチ側（フラグメントシェーダー）が「今アクティブな
/// スロット番号」だけを辿れるようにするための一覧。これが無いと、
/// `scene_density`/距離計算がpoolの512スロット全部を毎回舐めることになり、
/// アクティブなAggregateが2個程度でも致命的に重くなる（低性能GPUで実測
/// FPS=1という壊滅的な結果が出た）。
#[derive(Resource, Default)]
struct ActiveSlotBuffer(StorageBuffer<Vec<u32>>);

#[derive(Resource, Default)]
struct SoapSimParamsBuffer(UniformBuffer<SimParams>);

#[derive(Resource, Default)]
struct SoapViewUniformBuffer(UniformBuffer<SoapViewUniform>);

fn prepare_soap_view_uniform(
    mut buffer: ResMut<SoapViewUniformBuffer>,
    render_device: Res<RenderDevice>,
    render_queue: Res<RenderQueue>,
    // With<ViewTarget>: シーンにはシャドウマップ用の DirectionalLight があり、
    // それぞれ追加の ExtractedView（影用カメラ）を持つ。フィルタなしで
    // Query<&ExtractedView> の先頭を拾うと、メインカメラではなくシャドウの
    // Viewを掴んでしまい、レイの原点・方向が完全に破綻する（第23節の
    // 「Phase 1: カメラ1台固定」はViewTargetを持つ実際の描画カメラに限定する）。
    views: Query<&ExtractedView, With<ViewTarget>>,
    quality: Res<ExtractedFoamQuality>,
    extracted: Res<ExtractedFoamAggregates>,
) {
    let profile = quality.0.profile();
    // Phase 1: カメラ1台固定という前提（doc第23節と同じ簡略化）。
    if let Some(view) = views.iter().next() {
        let clip_from_world = view
            .clip_from_world
            .unwrap_or_else(|| view.clip_from_view * view.world_from_view.to_matrix().inverse());
        buffer.0.set(SoapViewUniform {
            clip_from_world,
            world_from_clip: clip_from_world.inverse(),
            camera_world_position: view.world_from_view.translation(),
            raymarch_steps: profile.raymarch_steps,
            microstructure_quality: profile.microstructure_quality.as_u32(),
            active_count: extracted.0.len() as u32,
        });
    }
    buffer.0.write_buffer(&render_device, &render_queue);
}

fn prepare_foam_drive_queue(
    extracted: Res<ExtractedFoamAggregates>,
    mut drive_queue: ResMut<FoamDriveQueue>,
    mut active_slots: ResMut<ActiveSlotBuffer>,
    mut sim_params: ResMut<SoapSimParamsBuffer>,
    render_device: Res<RenderDevice>,
    render_queue: Res<RenderQueue>,
    time: Res<Time>,
) {
    drive_queue.0.get_mut().clear();
    active_slots.0.get_mut().clear();

    for aggregate in &extracted.0 {
        drive_queue.0.get_mut().push(GpuDriveEntry {
            target_slot: aggregate.slot,
            position: aggregate.position,
            velocity: aggregate.velocity,
            base_radius: aggregate.radius,
            generation: aggregate.generation,
        });
        // 課題S-12：レンダー側がpool全体ではなく、この一覧だけを辿れるようにする。
        active_slots.0.get_mut().push(aggregate.slot);
    }

    if drive_queue.0.get().is_empty() {
        // ゼロサイズのストレージバッファを作らないためのダミーエントリ
        // （target_slotは実在しないインデックスなので、compute
        // shader側では何にもマッチしない）。
        drive_queue.0.get_mut().push(GpuDriveEntry {
            target_slot: u32::MAX,
            position: Vec3::ZERO,
            velocity: Vec3::ZERO,
            base_radius: 0.0,
            generation: 0,
        });
    }
    if active_slots.0.get().is_empty() {
        // ゼロサイズのストレージバッファを作らないためのダミー。
        // active_count(SoapViewUniform)が0なので、レンダー側で読まれることはない。
        active_slots.0.get_mut().push(0);
    }

    let drive_count = drive_queue.0.get().len() as u32;
    drive_queue.0.write_buffer(&render_device, &render_queue);
    active_slots.0.write_buffer(&render_device, &render_queue);

    sim_params.0.set(SimParams {
        dt: time.delta_secs(),
        table_height: 0.0,
        // 課題S-15：0.12だと着弾速度(概ね6〜10)に対してspread=1.7〜2.2程度
        // にしかならず、遠くから見ると「ちょっと潰れた球」にしか見えず
        // 扁平化が伝わらなかった。もっとはっきり潰れて見えるよう強める。
        impact_factor: 0.28,
        // 課題S-01：自然な到達範囲より十分高くして、着弾速度差がそのまま
        // 最終形状差として残るようにする。
        max_spread: 3.4,
        drive_count,
    });
    sim_params.0.write_buffer(&render_device, &render_queue);
}

// ---------------------------------------------------------------------------
// Bind Group（doc第21節：ComputeとRenderで別レイアウト）。
// ---------------------------------------------------------------------------

#[derive(Resource)]
struct SoapBindGroups {
    compute: BindGroup,
    render: BindGroup,
}

fn prepare_soap_bind_groups(
    mut commands: Commands,
    render_device: Res<RenderDevice>,
    pipeline_cache: Res<PipelineCache>,
    gpu: Res<SoapGpuResources>,
    drive_queue: Res<FoamDriveQueue>,
    active_slots: Res<ActiveSlotBuffer>,
    sim_params: Res<SoapSimParamsBuffer>,
    view_uniform: Res<SoapViewUniformBuffer>,
) {
    let (Some(drive_binding), Some(active_slots_binding), Some(sim_binding), Some(view_binding)) = (
        drive_queue.0.binding(),
        active_slots.0.binding(),
        sim_params.0.binding(),
        view_uniform.0.binding(),
    ) else {
        return;
    };

    let compute_layout = pipeline_cache.get_bind_group_layout(&gpu.compute_layout);
    let compute = render_device.create_bind_group(
        Some("soap_compute_bind_group"),
        &compute_layout,
        &BindGroupEntries::sequential((
            gpu.foam_instance_pool.as_entire_binding(),
            drive_binding,
            sim_binding,
        )),
    );

    let render_layout = pipeline_cache.get_bind_group_layout(&gpu.render_layout);
    let render = render_device.create_bind_group(
        Some("soap_render_bind_group"),
        &render_layout,
        &BindGroupEntries::sequential((
            gpu.foam_instance_pool.as_entire_binding(),
            view_binding,
            active_slots_binding,
        )),
    );

    commands.insert_resource(SoapBindGroups { compute, render });
}

// ---------------------------------------------------------------------------
// Compute Dispatch（doc第23節：RenderGraphSystems::Begin、
// ビュー非依存で1フレーム1回）。
// ---------------------------------------------------------------------------

fn simulate_foam_instances(
    mut render_context: RenderContext,
    pipeline_cache: Res<PipelineCache>,
    gpu: Res<SoapGpuResources>,
    bind_groups: Option<Res<SoapBindGroups>>,
    extracted: Res<ExtractedFoamAggregates>,
) {
    // 課題S-05：今生きているFoam
    // Aggregateが1つも無ければDispatch自体をスキップする。
    if extracted.0.is_empty() {
        return;
    }
    let Some(bind_groups) = bind_groups else { return };
    let Some(pipeline) = pipeline_cache.get_compute_pipeline(gpu.compute_pipeline) else { return };

    let encoder = render_context.command_encoder();
    let mut pass = encoder.begin_compute_pass(&ComputePassDescriptor {
        label: Some("soap_simulate"),
        timestamp_writes: None,
    });
    pass.set_pipeline(pipeline);
    pass.set_bind_group(0, &bind_groups.compute, &[]);
    pass.dispatch_workgroups(gpu.pool_capacity.div_ceil(64), 1, 1);
}

// ---------------------------------------------------------------------------
// Metaball描画パス（doc第24節：Transparent3dフェーズへの参加）。
// ---------------------------------------------------------------------------

fn queue_soap_metaballs(
    draw_functions: Res<DrawFunctions<Transparent3d>>,
    pipeline_cache: Res<PipelineCache>,
    mut gpu: ResMut<SoapGpuResources>,
    mut phases: ResMut<ViewSortedRenderPhases<Transparent3d>>,
    views: Query<(Entity, &ExtractedView, &ViewTarget)>,
    extracted: Res<ExtractedFoamAggregates>,
) {
    // 課題S-05：今生きているFoam Aggregateが1つも無ければ、画面全体に対する
    // レイマーチ自体を丸ごとスキップする。
    if extracted.0.is_empty() {
        return;
    }

    let render_pipeline = match gpu.render_pipeline {
        Some(id) => id,
        None => {
            let Some((_, _, view_target)) = views.iter().next() else { return };
            let format = view_target.main_texture_format();
            let id = pipeline_cache.queue_render_pipeline(RenderPipelineDescriptor {
                label: Some("soap_metaball_pipeline".into()),
                layout: vec![gpu.render_layout.clone()],
                vertex: VertexState {
                    shader: gpu.render_shader.clone(),
                    shader_defs: vec![],
                    entry_point: Some("vertex".into()),
                    buffers: vec![],
                },
                fragment: Some(FragmentState {
                    shader: gpu.render_shader.clone(),
                    shader_defs: vec![],
                    entry_point: Some("fragment".into()),
                    targets: vec![Some(ColorTargetState {
                        format,
                        blend: Some(BlendState::ALPHA_BLENDING),
                        write_mask: ColorWrites::ALL,
                    })],
                }),
                primitive: PrimitiveState::default(),
                // Transparent3dのレンダーパスはDepth32Floatの深度アタッチメントを
                // 持っており、パイプライン側もフォーマットを合わせないと
                // "Incompatible depth-stencil attachment format" で即クラッシュする
                // （depth_stencil: None は不可）。フォーマットは合わせつつ、
                // Always/書き込みなしにして実質的に深度テストを無効化する
                // （詳細はshader冒頭のコメント）。
                depth_stencil: Some(DepthStencilState {
                    format: CORE_3D_DEPTH_FORMAT,
                    depth_write_enabled: Some(false),
                    depth_compare: Some(CompareFunction::Always),
                    stencil: StencilState::default(),
                    bias: DepthBiasState::default(),
                }),
                multisample: MultisampleState::default(),
                immediate_size: 0,
                zero_initialize_workgroup_memory: false,
            });
            gpu.render_pipeline = Some(id);
            id
        }
    };

    let draw_function = draw_functions.read().get_id::<DrawSoapMetaballs>().unwrap();

    for (view_entity, view, _) in &views {
        let Some(phase) = phases.get_mut(&view.retained_view_entity) else { continue };
        phase.add_transient(Transparent3d {
            sorting_info: TransparentSortingInfo3d::AlwaysOnTop,
            distance: 0.0,
            pipeline: render_pipeline,
            entity: (view_entity, MainEntity::from(view_entity)),
            draw_function,
            batch_range: 0..1,
            extra_index: PhaseItemExtraIndex::None,
            indexed: false,
        });
    }
}

struct SetSoapBindGroup;

impl<P: PhaseItem> RenderCommand<P> for SetSoapBindGroup {
    type Param = SRes<SoapBindGroups>;
    type ViewQuery = ();
    type ItemQuery = ();

    fn render<'w>(
        _item: &P,
        _view: (),
        _entity: Option<()>,
        bind_groups: SystemParamItem<'w, '_, Self::Param>,
        pass: &mut TrackedRenderPass<'w>,
    ) -> RenderCommandResult {
        pass.set_bind_group(0, &bind_groups.into_inner().render, &[]);
        RenderCommandResult::Success
    }
}

struct DrawFullscreenTriangle;

impl<P: PhaseItem> RenderCommand<P> for DrawFullscreenTriangle {
    type Param = ();
    type ViewQuery = ();
    type ItemQuery = ();

    fn render<'w>(
        _item: &P,
        _view: (),
        _entity: Option<()>,
        _param: SystemParamItem<'w, '_, Self::Param>,
        pass: &mut TrackedRenderPass<'w>,
    ) -> RenderCommandResult {
        pass.draw(0..3, 0..1);
        RenderCommandResult::Success
    }
}

type DrawSoapMetaballs = (SetItemPipeline, SetSoapBindGroup, DrawFullscreenTriangle);
