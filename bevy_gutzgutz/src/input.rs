//! `GutzInputPlugin` —「Action」と「Device」を分離する入力抽象化。
//!
//! やってはいけないのは`keyboard.pressed(KeyCode::KeyW)`のような生の入力を
//! そのまま薄くラップすること。それはBevyのAPIを隠しているだけで、gutzgutz
//! の価値が薄い。GutzInputの核心は、
//!
//! - プレイヤーが何をしたいか（[`GutzAction`]）と、何のデバイスで操作したか
//!   （[`GutzInputSource`]）を分離する
//! - デバイスではなくActionを抽象化する
//! - [`crate::lifecycle`]によってActionの有効範囲（実行コンテキスト）を制御する
//!
//! という3点に尽きる。ゲーム側は自分のAction型（enum）を定義するだけで、
//! `GutzInputMap`でDeviceとの対応を、`GutzActionState`でその場の状態を
//! 得られる。

mod config;
mod map;
mod source;

use core::marker::PhantomData;

use bevy::prelude::*;
pub use config::{load_into, load_into_from_file};
pub use map::{GutzAction, GutzActionState, GutzInputMap};
pub use source::GutzInputSource;

use crate::lifecycle::GutzLifecycleState;

/// Action型`A`とゲームのState型`S`を受け取り、`GutzInputMap<A>`/
/// `GutzActionState<A>`を毎フレーム更新するプラグイン。
///
/// ```ignore
/// app.add_plugins(GutzInputPlugin::<PlayerAction, GameState>::default());
/// ```
///
/// バインディングは起動後に`ResMut<GutzInputMap<PlayerAction>>`へ直接
/// `.bind(...)`するか、[`load_into`]でTOMLから読み込む。
pub struct GutzInputPlugin<A: GutzAction, S: GutzLifecycleState>(PhantomData<fn() -> (A, S)>);

impl<A: GutzAction, S: GutzLifecycleState> Default for GutzInputPlugin<A, S> {
    fn default() -> Self {
        Self(PhantomData)
    }
}

impl<A: GutzAction, S: GutzLifecycleState> Plugin for GutzInputPlugin<A, S> {
    fn build(&self, app: &mut App) {
        app.init_resource::<GutzInputMap<A>>()
            .init_resource::<GutzActionState<A>>()
            .add_systems(Update, map::update_action_state::<A, S>);
    }
}
