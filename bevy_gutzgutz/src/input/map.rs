use bevy::prelude::*;
use std::collections::{HashMap, HashSet};

use crate::lifecycle::{GutzExecutionContext, GutzLifecycleState};

use super::source::GutzInputSource;

/// ゲーム側が定義する「プレイヤーが何をしたいか」を表すAction型が満たすべき
/// 制約をまとめたマーカートレイト。振る舞いの実装は不要（値として使える型
/// なら自動的に満たされる）。
///
/// GutzInputの核心は、`KeyCode`/`MouseButton`/`GamepadButton`を薄くラップ
/// するのではなく、「Action（プレイヤーの意図）」と「Device（操作手段）」を
/// 分離すること。ゲーム側はこのAction型だけを見てシステムを書き、実際に
/// どのキー・ボタンが割り当てられているかは`GutzInputMap`が仲介する。
///
/// ```ignore
/// #[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
/// enum PlayerAction {
///     RotateLeft,
///     RotateRight,
///     Charge,
///     Restart,
/// }
/// impl GutzAction for PlayerAction {}
/// ```
pub trait GutzAction: Copy + Eq + core::hash::Hash + Send + Sync + 'static {}

impl<T: Copy + Eq + core::hash::Hash + Send + Sync + 'static> GutzAction for T {}

/// Action→Deviceのバインディングと、Actionごとの実行コンテキスト制限を持つ
/// 設定リソース。ゲーム起動時に組み立てるか、[`super::config::GutzInputConfig`]
/// 経由でTOMLから読み込む。
#[derive(Resource)]
pub struct GutzInputMap<A: GutzAction> {
    bindings: HashMap<A, Vec<GutzInputSource>>,
    contexts: HashMap<A, GutzExecutionContext>,
}

impl<A: GutzAction> Default for GutzInputMap<A> {
    fn default() -> Self {
        Self { bindings: HashMap::default(), contexts: HashMap::default() }
    }
}

impl<A: GutzAction> GutzInputMap<A> {
    /// `action`に`source`を追加でバインドする（同じActionに複数のDeviceを
    /// 割り当てられる。例：Confirmにキーボードとゲームパッドの両方）。
    pub fn bind(&mut self, action: A, source: GutzInputSource) -> &mut Self {
        self.bindings.entry(action).or_default().push(source);
        self
    }

    /// `action`が有効なのは`context`の間だけ、という制限を追加する
    /// （doc：「LifecycleによってActionの有効範囲を制御する」）。設定しない
    /// Actionはどの実行コンテキストでも常に評価される。
    pub fn restrict_to(&mut self, action: A, context: GutzExecutionContext) -> &mut Self {
        self.contexts.insert(action, context);
        self
    }

    fn allowed_in(&self, action: &A, current: Option<GutzExecutionContext>) -> bool {
        match self.contexts.get(action) {
            Some(required) => Some(*required) == current,
            None => true,
        }
    }
}

/// 毎フレーム計算される、Action単位の読み取り専用の入力状態。
/// `KeyCode`等の生の値は一切公開しない——ゲーム側はActionの名前だけを見る。
#[derive(Resource)]
pub struct GutzActionState<A: GutzAction> {
    pressed: HashSet<A>,
    just_pressed: HashSet<A>,
    just_released: HashSet<A>,
}

impl<A: GutzAction> Default for GutzActionState<A> {
    fn default() -> Self {
        Self { pressed: HashSet::default(), just_pressed: HashSet::default(), just_released: HashSet::default() }
    }
}

impl<A: GutzAction> GutzActionState<A> {
    pub fn pressed(&self, action: A) -> bool {
        self.pressed.contains(&action)
    }

    pub fn just_pressed(&self, action: A) -> bool {
        self.just_pressed.contains(&action)
    }

    pub fn just_released(&self, action: A) -> bool {
        self.just_released.contains(&action)
    }
}

pub(crate) fn update_action_state<A: GutzAction, S: GutzLifecycleState>(
    map: Res<GutzInputMap<A>>,
    mut state: ResMut<GutzActionState<A>>,
    keyboard: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    gamepads: Query<&Gamepad>,
    current_state: Option<Res<State<S>>>,
) {
    state.pressed.clear();
    state.just_pressed.clear();
    state.just_released.clear();

    let context = current_state.map(|s| s.get().execution_context());

    for (action, sources) in map.bindings.iter() {
        if !map.allowed_in(action, context) {
            continue;
        }
        for source in sources {
            if source.pressed(&keyboard, &mouse, &gamepads) {
                state.pressed.insert(*action);
            }
            if source.just_pressed(&keyboard, &mouse, &gamepads) {
                state.just_pressed.insert(*action);
            }
            if source.just_released(&keyboard, &mouse, &gamepads) {
                state.just_released.insert(*action);
            }
        }
    }
}
