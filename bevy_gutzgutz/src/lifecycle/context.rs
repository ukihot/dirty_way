use bevy::ecs::schedule::ScheduleLabel;
use bevy::prelude::*;

/// ゲームの実行コンテキストの大分類。個々のゲームがどんな具体的な`States`型
/// （Title/Playing/Pause、あるいはPlaying/GameOverのような2値でも）を持っていても、
/// 必ずこの2つのどちらかに分類できる、という前提に立つ。
///
/// gutzgutzはこの列挙型そのものは提供するが、「ゲームのどのStateがどちらに
/// あたるか」は一切知らない・決めない。それを教えるのが[`GutzLifecycleState`]。
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum GutzExecutionContext {
    /// ゲームプレイが進行している。敵の移動・入力受付・スコア加算などの
    /// Gameplay系システムはこの間だけ動く想定。
    InGame,
    /// タイトル画面・メニュー・リザルト画面など、ゲームプレイそのものは
    /// 進行していない。
    OutGame,
}

/// ゲーム固有の`States`型が、自分の各バリアントを[`GutzExecutionContext`]の
/// どちらに分類するかを教えるためのトレイト。
///
/// gutzgutzはInGame/OutGameを強制的なenumとして押し付けない
/// （Bevy/Avian3D本体を丸ごとラップしないのと同じ思想）。ゲーム側は
/// 自分の`States`型にこれを実装するだけでよい。
///
/// ```ignore
/// #[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default, States)]
/// enum GameState {
///     #[default]
///     Playing,
///     GameOver,
/// }
///
/// impl GutzLifecycleState for GameState {
///     fn execution_context(&self) -> GutzExecutionContext {
///         match self {
///             GameState::Playing => GutzExecutionContext::InGame,
///             GameState::GameOver => GutzExecutionContext::OutGame,
///         }
///     }
/// }
/// ```
pub trait GutzLifecycleState: States {
    fn execution_context(&self) -> GutzExecutionContext;
}

/// 現在のStateが`InGame`かどうかを見る実行条件。
///
/// ```ignore
/// app.add_systems(Update, move_enemies.run_if(in_game::<GameState>()));
/// ```
pub fn in_game<S: GutzLifecycleState>() -> impl FnMut(Option<Res<State<S>>>) -> bool + Clone {
    in_context::<S>(GutzExecutionContext::InGame)
}

/// 現在のStateが`OutGame`かどうかを見る実行条件。
pub fn out_game<S: GutzLifecycleState>() -> impl FnMut(Option<Res<State<S>>>) -> bool + Clone {
    in_context::<S>(GutzExecutionContext::OutGame)
}

/// 任意の[`GutzExecutionContext`]と一致するかどうかを見る実行条件。
/// `in_game`/`out_game`はこれの薄いラッパー。
pub fn in_context<S: GutzLifecycleState>(
    context: GutzExecutionContext,
) -> impl FnMut(Option<Res<State<S>>>) -> bool + Clone {
    move |state: Option<Res<State<S>>>| {
        state.is_some_and(|state| state.get().execution_context() == context)
    }
}

/// [`GutzExecutionContext`]が変化した瞬間にだけ発行される、`S`型に依存しない
/// （＝ジェネリックでない）メッセージ。ゲームのState型を知らない汎用的な
/// 購読者が「実行コンテキストが変わった」ことにだけ反応できるようにする。
#[derive(Message, Clone, Copy, Debug)]
pub struct GutzExecutionContextChanged {
    pub previous: Option<GutzExecutionContext>,
    pub current: GutzExecutionContext,
}

/// Bevyの`OnEnter<S>`/`OnExit<S>`は「厳密に一致する1つのState値」にしか
/// フックできない。複数の具体的なStateが同じ`GutzExecutionContext`に属する
/// 場合（例：Title・GameOverが両方OutGame）に「OutGameになった瞬間」へ
/// フックしたいことがあるため、コンテキスト単位のOnEnter/OnExit相当の
/// スケジュールを別途用意する。ゲーム側は通常のOnEnter/OnExitと同じ要領で
/// `app.add_systems(OnEnterContext(GutzExecutionContext::InGame), ...)`のように使う。
#[derive(ScheduleLabel, Clone, Debug, PartialEq, Eq, Hash)]
pub struct OnEnterContext(pub GutzExecutionContext);

#[derive(ScheduleLabel, Clone, Debug, PartialEq, Eq, Hash)]
pub struct OnExitContext(pub GutzExecutionContext);

/// `StateTransitionEvent<S>`を監視し、実際に`GutzExecutionContext`が
/// 変化したフレームでだけ`OnExitContext`/`OnEnterContext`スケジュールと
/// `GutzExecutionContextChanged`を発行する。同じコンテキスト内での
/// State遷移（例：Title→Pause、両方OutGame）では何も発行しない。
pub(crate) fn detect_context_transitions<S: GutzLifecycleState>(
    mut transitions: MessageReader<StateTransitionEvent<S>>,
    mut previous: Local<Option<GutzExecutionContext>>,
    mut changed: MessageWriter<GutzExecutionContextChanged>,
    mut commands: Commands,
) {
    for transition in transitions.read() {
        let Some(entered) = &transition.entered else { continue };
        let current = entered.execution_context();
        if *previous == Some(current) {
            continue;
        }

        if let Some(previous_context) = *previous {
            commands.run_schedule(OnExitContext(previous_context));
        }
        commands.run_schedule(OnEnterContext(current));
        changed.write(GutzExecutionContextChanged { previous: *previous, current });
        *previous = Some(current);
    }
}
