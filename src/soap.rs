//! リアルタイム・ハンドソープ表現（doc/soap-model.md）。
//!
//! `bubble.rs` はゲームロジック（Avian3Dによる当たり判定・ダメージ・足止め）だけを
//! 担当し、見た目は持たない。こちらはGPU常駐のParticle Poolをコンピュートシェーダーで
//! 弾道飛翔→着弾扁平化→広がりのステートマシンで更新し、Metaballレイマーチで
//! 「ぬちゃっと広がる液体」として描画する。CPUは毎フレーム全粒子を転送しない
//! （doc第2.1〜2.2節）。

use bevy::core_pipeline::core_3d::{Transparent3d, TransparentSortingInfo3d, CORE_3D_DEPTH_FORMAT};
use bevy::ecs::system::lifetimeless::SRes;
use bevy::ecs::system::SystemParamItem;
use bevy::prelude::*;
use bevy::render::render_phase::{
    AddRenderCommand, DrawFunctions, PhaseItem, PhaseItemExtraIndex, RenderCommand,
    RenderCommandResult, SetItemPipeline, TrackedRenderPass, ViewSortedRenderPhases,
};
use bevy::render::render_resource::encase::StorageBuffer as EncaseBuffer;
use bevy::render::render_resource::{binding_types, *};
use bevy::render::renderer::{RenderContext, RenderDevice, RenderGraph, RenderGraphSystems, RenderQueue};
use bevy::render::sync_world::MainEntity;
use bevy::render::view::{ExtractedView, ViewTarget};
use bevy::render::{Extract, ExtractSchedule, Render, RenderApp, RenderStartup, RenderSystems};
use rand::Rng;

/// Phase 1のGPU常駐Particle Poolの固定サイズ（doc第5節：100〜1024を想定）。
const PARTICLE_POOL_SIZE: u32 = 256;

/// Main World側から発射される、1回の「ハンドソープ押下」のリクエスト（doc第4,7節）。
#[derive(Message, Clone, Copy)]
pub struct SoapSpawnRequest {
    pub position: Vec3,
    pub direction: Vec3,
    pub pressure: f32,
    pub amount: u32,
}

pub struct SoapPlugin;

impl Plugin for SoapPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<SoapSpawnRequest>();

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
            .init_resource::<ExtractedSoapSpawnRequests>()
            .init_resource::<NextSlotCursor>()
            .init_resource::<SoapSpawnQueue>()
            .init_resource::<SoapSimParamsBuffer>()
            .init_resource::<SoapViewUniformBuffer>()
            .add_systems(ExtractSchedule, extract_soap_spawn_requests)
            .add_systems(RenderStartup, init_soap_render_resources)
            .add_systems(
                Render,
                (
                    prepare_soap_view_uniform.in_set(RenderSystems::PrepareResources),
                    prepare_soap_frame_data.in_set(RenderSystems::PrepareResources),
                    prepare_soap_bind_groups.in_set(RenderSystems::PrepareBindGroups),
                    queue_soap_metaballs.in_set(RenderSystems::Queue),
                ),
            )
            .add_systems(RenderGraph, simulate_soap_particles.in_set(RenderGraphSystems::Begin))
            .add_render_command::<Transparent3d, DrawSoapMetaballs>();
    }
}

#[derive(Resource, Clone)]
struct SoapShaderHandles {
    compute: Handle<Shader>,
    render: Handle<Shader>,
}

// ---------------------------------------------------------------------------
// GPU側データレイアウト（doc第25.1節と1:1対応。フィールド順を変えない）。
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, ShaderType, Default)]
struct GpuParticle {
    position: Vec3,
    velocity: Vec3,
    scale: Vec3,
    state: u32,
    lifetime: f32,
}

#[derive(Clone, Copy, ShaderType)]
struct GpuSpawnRequest {
    target_slot: u32,
    position: Vec3,
    velocity: Vec3,
}

#[derive(Clone, Copy, ShaderType, Default)]
struct SimParams {
    dt: f32,
    gravity: f32,
    table_height: f32,
    impact_factor: f32,
    damping: f32,
    max_spread: f32,
    spawn_count: u32,
}

#[derive(Clone, Copy, ShaderType, Default)]
struct SoapViewUniform {
    clip_from_world: Mat4,
    world_from_clip: Mat4,
    camera_world_position: Vec3,
}

// ---------------------------------------------------------------------------
// Main World → Render World（Extract）
// ---------------------------------------------------------------------------

#[derive(Resource, Default)]
struct ExtractedSoapSpawnRequests(Vec<SoapSpawnRequest>);

fn extract_soap_spawn_requests(
    mut extracted: ResMut<ExtractedSoapSpawnRequests>,
    mut messages: Extract<MessageReader<SoapSpawnRequest>>,
) {
    extracted.0.extend(messages.read().copied());
}

// ---------------------------------------------------------------------------
// Render World常駐GPUリソース（RenderStartupで1回だけ作る。doc第21節）。
// ---------------------------------------------------------------------------

#[derive(Resource)]
struct SoapGpuResources {
    particle_pool: Buffer,
    pool_capacity: u32,
    compute_layout: BindGroupLayoutDescriptor,
    render_layout: BindGroupLayoutDescriptor,
    compute_pipeline: CachedComputePipelineId,
    /// 実際のビュー出力フォーマットが分かるQueue段階まで遅延生成する（Phase 1: 単一カメラ前提）。
    render_pipeline: Option<CachedRenderPipelineId>,
    render_shader: Handle<Shader>,
}

fn init_soap_render_resources(
    mut commands: Commands,
    render_device: Res<RenderDevice>,
    pipeline_cache: Res<PipelineCache>,
    shaders: Res<SoapShaderHandles>,
) {
    // Particle Pool: ゼロ初期化したGPU専有バッファ。以後CPUからは書き込まない
    // （encaseは正しいバイトサイズ・パディングを求めるためだけに使う）。
    let zeroed = vec![GpuParticle::default(); PARTICLE_POOL_SIZE as usize];
    let mut init_bytes = EncaseBuffer::new(Vec::new());
    init_bytes.write(&zeroed).expect("failed to size soap particle pool");
    let particle_pool = render_device.create_buffer_with_data(&BufferInitDescriptor {
        label: Some("soap_particle_pool"),
        contents: init_bytes.as_ref(),
        usage: BufferUsages::STORAGE,
    });

    let compute_layout_entries = BindGroupLayoutEntries::sequential(
        ShaderStages::COMPUTE,
        (
            // @group(0) @binding(0) var<storage, read_write> particles: array<Particle>;
            binding_types::storage_buffer::<GpuParticle>(false),
            // @group(0) @binding(1) var<storage, read> spawn_requests: array<SpawnRequestGpu>;
            binding_types::storage_buffer_read_only::<GpuSpawnRequest>(false),
            // @group(0) @binding(2) var<uniform> sim_params: SimParams;
            binding_types::uniform_buffer::<SimParams>(false),
        ),
    );
    let compute_layout = BindGroupLayoutDescriptor::new("soap_compute_layout", &compute_layout_entries);

    let render_layout_entries = BindGroupLayoutEntries::sequential(
        ShaderStages::FRAGMENT,
        (
            // @group(0) @binding(0) var<storage, read> particles: array<Particle>;
            binding_types::storage_buffer_read_only::<GpuParticle>(false),
            // @group(0) @binding(1) var<uniform> view: SoapView;
            binding_types::uniform_buffer::<SoapViewUniform>(false),
        ),
    );
    let render_layout = BindGroupLayoutDescriptor::new("soap_render_layout", &render_layout_entries);

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
        particle_pool,
        pool_capacity: PARTICLE_POOL_SIZE,
        compute_layout,
        render_layout,
        compute_pipeline,
        render_pipeline: None,
        render_shader: shaders.render.clone(),
    });
}

// ---------------------------------------------------------------------------
// 毎フレームのPrepare（doc第22節：Extract→Prepare経路、リングカーソル方式）。
// ---------------------------------------------------------------------------

#[derive(Resource, Default)]
struct NextSlotCursor(u32);

#[derive(Resource, Default)]
struct SoapSpawnQueue(StorageBuffer<Vec<GpuSpawnRequest>>);

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
) {
    // Phase 1: カメラ1台固定という前提（doc第23節と同じ簡略化）。
    if let Some(view) = views.iter().next() {
        let clip_from_world = view
            .clip_from_world
            .unwrap_or_else(|| view.clip_from_view * view.world_from_view.to_matrix().inverse());
        buffer.0.set(SoapViewUniform {
            clip_from_world,
            world_from_clip: clip_from_world.inverse(),
            camera_world_position: view.world_from_view.translation(),
        });
    }
    buffer.0.write_buffer(&render_device, &render_queue);
}

fn prepare_soap_frame_data(
    mut extracted: ResMut<ExtractedSoapSpawnRequests>,
    mut cursor: ResMut<NextSlotCursor>,
    mut spawn_queue: ResMut<SoapSpawnQueue>,
    mut sim_params: ResMut<SoapSimParamsBuffer>,
    gpu: Res<SoapGpuResources>,
    render_device: Res<RenderDevice>,
    render_queue: Res<RenderQueue>,
    time: Res<Time>,
) {
    spawn_queue.0.get_mut().clear();

    let mut rng = rand::thread_rng();
    for request in extracted.0.drain(..) {
        for _ in 0..request.amount {
            let slot = cursor.0;
            cursor.0 = (cursor.0 + 1) % gpu.pool_capacity;

            // 各粒子に小さなランダム性を与える（doc第7節）。全粒子が完全に同一だと
            // レーザーのような直線的な液体になってしまうため避ける。
            let offset = Vec3::new(
                rng.gen_range(-0.15f32..0.15),
                rng.gen_range(-0.05f32..0.05),
                rng.gen_range(-0.15f32..0.15),
            );
            let scatter = Vec3::new(
                rng.gen_range(-0.6f32..0.6),
                rng.gen_range(-0.3f32..0.3),
                rng.gen_range(-0.6f32..0.6),
            );

            spawn_queue.0.get_mut().push(GpuSpawnRequest {
                target_slot: slot,
                position: request.position + offset,
                velocity: request.direction * request.pressure + scatter,
            });
        }
    }

    if spawn_queue.0.get().is_empty() {
        // ゼロサイズのストレージバッファを作らないためのダミーエントリ
        // （target_slotは実在しないインデックスなので、compute shader側では何にもマッチしない）。
        spawn_queue.0.get_mut().push(GpuSpawnRequest {
            target_slot: u32::MAX,
            position: Vec3::ZERO,
            velocity: Vec3::ZERO,
        });
    }

    let spawn_count = spawn_queue.0.get().len() as u32;
    spawn_queue.0.write_buffer(&render_device, &render_queue);

    sim_params.0.set(SimParams {
        dt: time.delta_secs(),
        gravity: 14.0,
        table_height: 0.0,
        impact_factor: 0.12,
        damping: 0.85,
        max_spread: 2.2,
        spawn_count,
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
    spawn_queue: Res<SoapSpawnQueue>,
    sim_params: Res<SoapSimParamsBuffer>,
    view_uniform: Res<SoapViewUniformBuffer>,
) {
    let (Some(spawn_binding), Some(sim_binding), Some(view_binding)) =
        (spawn_queue.0.binding(), sim_params.0.binding(), view_uniform.0.binding())
    else {
        return;
    };

    let compute_layout = pipeline_cache.get_bind_group_layout(&gpu.compute_layout);
    let compute = render_device.create_bind_group(
        Some("soap_compute_bind_group"),
        &compute_layout,
        &BindGroupEntries::sequential((gpu.particle_pool.as_entire_binding(), spawn_binding, sim_binding)),
    );

    let render_layout = pipeline_cache.get_bind_group_layout(&gpu.render_layout);
    let render = render_device.create_bind_group(
        Some("soap_render_bind_group"),
        &render_layout,
        &BindGroupEntries::sequential((gpu.particle_pool.as_entire_binding(), view_binding)),
    );

    commands.insert_resource(SoapBindGroups { compute, render });
}

// ---------------------------------------------------------------------------
// Compute Dispatch（doc第23節：RenderGraphSystems::Begin、ビュー非依存で1フレーム1回）。
// ---------------------------------------------------------------------------

fn simulate_soap_particles(
    mut render_context: RenderContext,
    pipeline_cache: Res<PipelineCache>,
    gpu: Res<SoapGpuResources>,
    bind_groups: Option<Res<SoapBindGroups>>,
) {
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
) {
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
