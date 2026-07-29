use bevy::prelude::*;
use bevy_gutzgutz::input::GutzActionState;
use bevy_gutzgutz::lifecycle::{GutzExecutionContext, GutzLifecycleState};
use bevy_gutzgutz::save::{GutzLoadRequest, GutzLoaded, GutzSaveRequest};

use crate::actions::PlayerAction;
use crate::bubble::{Bubble, FoamGpuBinding, FoamSlotAllocator};
use crate::consts::PLAYER_MAX_HEALTH;
use crate::enemy::{Enemy, EnemySpawnTimer};
use crate::player::Charge;

#[derive(Clone, Copy, Eq, PartialEq, Hash, Debug, Default, States)]
pub enum GameState {
    #[default]
    Playing,
    GameOver,
}

/// GutzLifeCyclePlugin向けの分類。
/// GameOverはリザルト表示中でゲームプレイが進行していないため、
/// タイトル画面と同じOutGame扱いにする。
impl GutzLifecycleState for GameState {
    fn execution_context(&self) -> GutzExecutionContext {
        match self {
            GameState::Playing => GutzExecutionContext::InGame,
            GameState::GameOver => GutzExecutionContext::OutGame,
        }
    }
}

/// `GutzSavePlugin`で永続化する、dirty_way側の唯一のセーブデータ。
/// gutzgutz自身はこの中身を一切解釈しない（doc：「セーブデータと実行中の
/// Worldを分離する」）——起動時に読み込んだ値をこのResourceへ反映するのも、
/// 保存したい値をここから読み出して送るのもdirty_way側の責務。
#[derive(Resource, Clone, Copy, Default, serde::Serialize, serde::Deserialize)]
pub struct SaveData {
    pub high_score: u32,
}

#[derive(Resource, Default)]
pub struct Score(pub u32);

#[derive(Resource)]
pub struct Health(pub i32);

impl Default for Health {
    fn default() -> Self {
        Health(PLAYER_MAX_HEALTH)
    }
}

pub struct GameStatePlugin;

impl Plugin for GameStatePlugin {
    fn build(&self, app: &mut App) {
        app.init_state::<GameState>()
            .init_resource::<Score>()
            .init_resource::<Health>()
            .init_resource::<SaveData>()
            .add_systems(Startup, request_load_save_data)
            .add_systems(Update, apply_loaded_save_data)
            .add_systems(OnEnter(GameState::Playing), reset_game)
            .add_systems(OnEnter(GameState::GameOver), save_high_score)
            .add_systems(Update, restart_input);
    }
}

/// 起動時に一度だけ`save.toml`の読み込みを要求する。ファイルが無い
/// （初回起動）場合は`GutzLoadFailed`が飛ぶだけで、`SaveData::default()`
/// （high_score: 0）のまま何も変わらない。
fn request_load_save_data(mut requests: MessageWriter<GutzLoadRequest<SaveData>>) {
    requests.write(GutzLoadRequest::default());
}

/// 読み込みが完了したら、自分のResourceへ反映するかどうかも含めて自分で
/// 決める（doc：「セーブデータと実行中のWorldを分離する」——gutzgutzは
/// 勝手にResourceへ書き戻さない）。
fn apply_loaded_save_data(
    mut loaded: MessageReader<GutzLoaded<SaveData>>,
    mut save_data: ResMut<SaveData>,
) {
    for GutzLoaded(data) in loaded.read() {
        *save_data = *data;
    }
}

/// ラウンド終了時にハイスコアを更新し、更新した場合だけディスクへ保存する。
fn save_high_score(
    score: Res<Score>,
    mut save_data: ResMut<SaveData>,
    mut requests: MessageWriter<GutzSaveRequest<SaveData>>,
) {
    if score.0 > save_data.high_score {
        save_data.high_score = score.0;
        requests.write(GutzSaveRequest(*save_data));
    }
}

/// Playing に入るたび（起動時＆リスタート時）に状態を初期化する。
fn reset_game(
    mut commands: Commands,
    mut score: ResMut<Score>,
    mut health: ResMut<Health>,
    mut charge: ResMut<Charge>,
    mut spawn_timer: ResMut<EnemySpawnTimer>,
    mut foam_allocator: ResMut<FoamSlotAllocator>,
    enemies: Query<Entity, With<Enemy>>,
    bubbles: Query<(Entity, Option<&FoamGpuBinding>), With<Bubble>>,
) {
    score.0 = 0;
    *health = Health::default();
    *charge = Charge::default();
    *spawn_timer = EnemySpawnTimer::default();

    for entity in &enemies {
        commands.entity(entity).despawn();
    }
    for (entity, binding) in &bubbles {
        // GPUスロットも解放する。世代カウンタ（FoamGpuBinding::generation）のおかげで、
        // 再利用されたスロットにGPU側の古い変形状態が残っていても新規スポーンとして
        // 正しく初期化し直される（doc/soap-issues.md S-10）。
        if let Some(binding) = binding {
            foam_allocator.release(binding);
        }
        commands.entity(entity).despawn();
    }
}

fn restart_input(
    actions: Res<GutzActionState<PlayerAction>>,
    mut next_state: ResMut<NextState<GameState>>,
) {
    // RestartはOutGame専用（input.toml + actions.rs）なので、Playing中は
    // ここに来ても常にfalseになる。
    if actions.just_pressed(PlayerAction::Restart) {
        next_state.set(GameState::Playing);
    }
}
