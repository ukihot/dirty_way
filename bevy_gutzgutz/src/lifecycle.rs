//! ゲームが起動してから終了するまでの「ライフサイクル」を共通化する
//! `GutzLifeCyclePlugin`。
//!
//! gutzgutzは具体的なゲームStateを規定しない。ゲーム固有の`States`型
//! （`Title`/`Playing`/`Pause`のような3値でも、`Playing`/`GameOver`のような
//! 2値でもよい）はゲーム側が定義し、[`GutzLifecycleState`]を実装して各
//! バリアントをInGame/OutGameへ分類することだけをgutzgutzに教える。
//! gutzgutzが持つのは「状態そのもの」ではなく「状態を扱うための仕組み」——
//! 実行条件・コンテキスト単位のOnEnter/OnExit・Pause/Resume・遷移通知・
//! （devtools有効時の）State Debugger——だけである。

mod context;
mod pause;

use core::marker::PhantomData;

use bevy::prelude::*;
pub use context::{
    GutzExecutionContext, GutzExecutionContextChanged, GutzLifecycleState, OnEnterContext,
    OnExitContext, in_context, in_game, out_game,
};
pub use pause::{GutzPaused, not_paused, paused};

/// ゲーム固有のState型`S`を1つ受け取り、そのライフサイクル管理一式を配線する
/// プラグイン。ゲーム側は`S`に[`GutzLifecycleState`]を実装した上で、
/// `app.add_plugins(GutzLifeCyclePlugin::<GameState>::default())`のように
/// 追加する。`S`自体の`init_state`はゲーム側の責務のまま
/// （gutzgutzはBevy本体を丸ごとラップしない）。
pub struct GutzLifeCyclePlugin<S: GutzLifecycleState>(PhantomData<fn() -> S>);

impl<S: GutzLifecycleState> Default for GutzLifeCyclePlugin<S> {
    fn default() -> Self {
        Self(PhantomData)
    }
}

impl<S: GutzLifecycleState> Plugin for GutzLifeCyclePlugin<S> {
    fn build(&self, app: &mut App) {
        app.init_resource::<GutzPaused>()
            .add_message::<GutzExecutionContextChanged>()
            // ゲームがどちらのコンテキストにも1つもシステムを登録しなかった
            // 場合でも`Commands::run_schedule`が警告を出すだけで済むよう、
            // 4つのスケジュールを空でも必ず存在させておく。
            .init_schedule(OnEnterContext(GutzExecutionContext::InGame))
            .init_schedule(OnEnterContext(GutzExecutionContext::OutGame))
            .init_schedule(OnExitContext(GutzExecutionContext::InGame))
            .init_schedule(OnExitContext(GutzExecutionContext::OutGame))
            .add_systems(
                Update,
                (context::detect_context_transitions::<S>, pause::apply_pause_to_time),
            );

        #[cfg(feature = "devtools")]
        {
            // `devtools` featureが有効でも`GutzDevtoolsPlugin`自体が
            // 追加されているとは限らないため、`GutzDebugStats`は
            // ここでも（重複してもよい）初期化しておく。
            app.init_resource::<crate::devtools::GutzDebugStats>()
                .add_systems(Update, update_lifecycle_debug_stats::<S>);
        }
    }
}

/// State Debugger：現在のState/実行コンテキスト/Pause状態を`GutzDebugStats`
/// （devtoolsオーバーレイの汎用チャンネル）へ毎フレーム`set`する。
#[cfg(feature = "devtools")]
fn update_lifecycle_debug_stats<S: GutzLifecycleState>(
    state: Option<Res<State<S>>>,
    paused: Res<GutzPaused>,
    mut stats: ResMut<crate::devtools::GutzDebugStats>,
) {
    if let Some(state) = state {
        let context = match state.get().execution_context() {
            GutzExecutionContext::InGame => "InGame",
            GutzExecutionContext::OutGame => "OutGame",
        };
        stats.set("State", format!("{:?}", state.get()));
        stats.set("Context", context);
    }
    stats.set("Paused", paused.0);
}
