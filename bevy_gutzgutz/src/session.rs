//! `GutzGameSessionPlugin` — ゲームセッションを構成する共通機能の合成入口。
//!
//! `lifecycle`、`input`、`ui`、`save`は独立にも利用できるが、通常のゲームでは
//! 「タイトルからプレイへ進み、入力・メニュー・永続化を扱う」という一続きの
//! セッションを構成する。このプラグインはその定型配線だけをまとめる。
//!
//! 具体的なState、Action、画面、保存データの意味は一切決めない。ゲーム側は
//! それぞれの型を定義し、Stateの初期化・入力バインド・画面の見た目・保存値の
//! Worldへの反映を担当する。必要な機能だけを個別に追加したい場合は、下位の
//! `GutzLifeCyclePlugin`等を直接使ってよい。

use core::marker::PhantomData;
use std::path::Path;

use bevy::prelude::*;

use crate::input::{GutzAction, GutzInputPlugin};
use crate::lifecycle::{GutzLifeCyclePlugin, GutzLifecycleState};
use crate::save::{GutzSaveData, GutzSavePlugin};
use crate::ui::{GutzUiPlugin, GutzUiScreen};

/// 通常のゲームセッションを構成する合成プラグイン。
///
/// ```ignore
/// app.add_plugins(GutzGameSessionPlugin::<GameState, PlayerAction, UiScreen, SaveData>::
///     standard_save_location("dev", "example", "my_game", "save.toml"),
/// );
/// ```
///
/// 上の一行は、以下の4プラグインを追加するのと等価である。
///
/// - [`GutzLifeCyclePlugin`] — 実行コンテキストとPause
/// - [`GutzInputPlugin`] — Action入力とコンテキスト制限
/// - [`GutzUiPlugin`] — UI画面スタック
/// - [`GutzSavePlugin`] — TOMLでの明示的な保存・読み込み
///
/// 依存方向は一方向である。inputはlifecycleのコンテキストを読むが、UIと
/// saveはゲーム側が発行する状態・メッセージを受け取るだけで、他のコア機能の
/// 意味を知る必要がない。
pub struct GutzGameSessionPlugin<
    S: GutzLifecycleState,
    A: GutzAction,
    U: GutzUiScreen,
    D: GutzSaveData,
> {
    save: GutzSavePlugin<D>,
    _marker: PhantomData<fn() -> (S, A, U)>,
}

impl<S: GutzLifecycleState, A: GutzAction, U: GutzUiScreen, D: GutzSaveData>
    GutzGameSessionPlugin<S, A, U, D>
{
    /// 保存プラグインを明示指定してセッションを組み立てる。
    /// テスト用の任意パスなど、OS標準の保存先以外を使う場合に利用する。
    pub fn new(save: GutzSavePlugin<D>) -> Self {
        Self { save, _marker: PhantomData }
    }

    /// OS標準の保存場所を使う通常のゲーム向けの構築方法。
    pub fn standard_save_location(
        qualifier: &str,
        organization: &str,
        application: &str,
        file_name: impl AsRef<Path>,
    ) -> Self {
        Self::new(GutzSavePlugin::standard_location(
            qualifier,
            organization,
            application,
            file_name,
        ))
    }

    /// 任意パスを使うテスト・特殊環境向けの構築方法。
    pub fn save_at(path: impl Into<std::path::PathBuf>) -> Self {
        Self::new(GutzSavePlugin::new(path))
    }
}

impl<S: GutzLifecycleState, A: GutzAction, U: GutzUiScreen, D: GutzSaveData> Plugin
    for GutzGameSessionPlugin<S, A, U, D>
{
    fn build(&self, app: &mut App) {
        app.add_plugins((
            GutzLifeCyclePlugin::<S>::default(),
            GutzInputPlugin::<A, S>::default(),
            GutzUiPlugin::<U>::default(),
            self.save.clone(),
        ));
    }
}

#[cfg(test)]
mod tests {
    use bevy::state::app::StatesPlugin;

    use super::*;
    use crate::input::{GutzActionState, GutzInputMap};
    use crate::lifecycle::GutzExecutionContext;
    use crate::save::{GutzLoadRequest, GutzSaveRequest};
    use crate::ui::{GutzUiScreenClosed, GutzUiScreenOpened, GutzUiStack};

    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, States)]
    enum TestState {
        #[default]
        Playing,
    }

    impl GutzLifecycleState for TestState {
        fn execution_context(&self) -> GutzExecutionContext {
            GutzExecutionContext::InGame
        }
    }

    #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
    enum TestAction {
        Confirm,
    }

    #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
    enum TestScreen {
        Pause,
    }

    #[derive(serde::Serialize, serde::Deserialize)]
    struct TestSaveData;

    #[test]
    fn wires_the_complete_session_contract() {
        let mut app = App::new();
        app.add_plugins(StatesPlugin).init_state::<TestState>().add_plugins(
            GutzGameSessionPlugin::<TestState, TestAction, TestScreen, TestSaveData>::save_at(
                "unused-in-this-test.toml",
            ),
        );

        let _ = (TestAction::Confirm, TestScreen::Pause);

        assert!(app.world().contains_resource::<GutzInputMap<TestAction>>());
        assert!(app.world().contains_resource::<GutzActionState<TestAction>>());
        assert!(app.world().contains_resource::<GutzUiStack<TestScreen>>());
        assert!(app.world().contains_resource::<crate::lifecycle::GutzPaused>());
        assert!(app.world().contains_resource::<Messages<GutzUiScreenOpened<TestScreen>>>());
        assert!(app.world().contains_resource::<Messages<GutzUiScreenClosed<TestScreen>>>());
        assert!(app.world().contains_resource::<Messages<GutzSaveRequest<TestSaveData>>>());
        assert!(app.world().contains_resource::<Messages<GutzLoadRequest<TestSaveData>>>());
    }
}
