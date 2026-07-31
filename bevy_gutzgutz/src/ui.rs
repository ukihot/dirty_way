//! `GutzUiPlugin` — 「UIを作る」のではなく「UIのライフサイクル」を作る。
//!
//! HealthBar・Score・Ammoのようなゲーム固有のHUD部品をここへ大量に持ち込む
//! ことはしない。それはゲーム固有のドメインだから。GutzUiPluginの本質は、
//! ゲーム全体に共通するUIの**構造・遷移・操作モデル**を提供すること——
//! UIの「見た目」よりも、「UIを操作可能な状態機械として扱うための基盤」を
//! 提供する。
//!
//! 具体的には、画面（Title/PauseMenu/Settings/GameOverのような「今どのUI
//! 画面が開いているか」の単位）をスタックとして管理する[`GutzUiStack`]と、
//! スタックの一番上（＝今アクティブな画面）が変わった瞬間に発行される
//! [`GutzUiScreenOpened`]/[`GutzUiScreenClosed`]メッセージだけを持つ。実際に
//! どんな見た目のUIをスポーン/despawnするかはゲーム側がこれらのメッセージを
//! 購読して決める。

use core::fmt::Debug;
use core::marker::PhantomData;

use bevy::prelude::*;

/// ゲーム側が定義する「今どのUI画面が開いているか」を表す型が満たすべき
/// 制約をまとめたマーカートレイト。[`crate::lifecycle::GutzLifecycleState`]
/// と同様、振る舞いの実装は不要。
///
/// ```ignore
/// #[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
/// enum UiScreen {
///     GameOver,
/// }
/// impl GutzUiScreen for UiScreen {}
/// ```
pub trait GutzUiScreen: Copy + Eq + core::hash::Hash + Debug + Send + Sync + 'static {}

impl<T: Copy + Eq + core::hash::Hash + Debug + Send + Sync + 'static> GutzUiScreen for T {}

/// 「今開いているUI画面」のスタック。一番上（[`Self::top`]）だけがアクティブ
/// という前提を置く（例：PauseMenuの上にSettingsを開くと、Settingsが閉じる
/// までPauseMenu自体は裏で保持されたまま操作対象から外れる）。
#[derive(Resource)]
pub struct GutzUiStack<T: GutzUiScreen> {
    stack: Vec<T>,
}

impl<T: GutzUiScreen> Default for GutzUiStack<T> {
    fn default() -> Self {
        Self { stack: Vec::new() }
    }
}

impl<T: GutzUiScreen> GutzUiStack<T> {
    pub fn push(&mut self, screen: T) {
        self.stack.push(screen);
    }

    pub fn pop(&mut self) -> Option<T> {
        self.stack.pop()
    }

    /// 一番上の画面を`screen`に差し替える（PauseMenu→Settingsのような
    /// 「積む」のではなく「切り替える」遷移用）。
    pub fn replace_top(&mut self, screen: T) {
        self.stack.pop();
        self.stack.push(screen);
    }

    pub fn clear(&mut self) {
        self.stack.clear();
    }

    pub fn top(&self) -> Option<T> {
        self.stack.last().copied()
    }

    pub fn is_open(&self, screen: T) -> bool {
        self.stack.contains(&screen)
    }
}

/// スタックの一番上に`screen`が来た（＝アクティブになった）瞬間に発行される。
#[derive(Message, Clone, Copy, Debug)]
pub struct GutzUiScreenOpened<T: GutzUiScreen>(pub T);

/// スタックの一番上から`screen`が外れた瞬間に発行される。
#[derive(Message, Clone, Copy, Debug)]
pub struct GutzUiScreenClosed<T: GutzUiScreen>(pub T);

fn detect_screen_changes<T: GutzUiScreen>(
    stack: Res<GutzUiStack<T>>,
    mut previous_top: Local<Option<T>>,
    mut opened: MessageWriter<GutzUiScreenOpened<T>>,
    mut closed: MessageWriter<GutzUiScreenClosed<T>>,
) {
    if !stack.is_changed() {
        return;
    }
    let current_top = stack.top();
    if current_top == *previous_top {
        return;
    }
    if let Some(previous) = *previous_top {
        closed.write(GutzUiScreenClosed(previous));
    }
    if let Some(current) = current_top {
        opened.write(GutzUiScreenOpened(current));
    }
    *previous_top = current_top;
}

/// 画面型`T`を1つ受け取り、そのスタック管理一式を配線するプラグイン。
///
/// ```ignore
/// app.add_plugins(GutzUiPlugin::<UiScreen>::default());
/// // ゲーム側
/// fn spawn_on_open(mut opened: MessageReader<GutzUiScreenOpened<UiScreen>>, ...) {
///     for GutzUiScreenOpened(screen) in opened.read() {
///         if *screen == UiScreen::GameOver { /* GameOver画面をスポーン */ }
///     }
/// }
/// ```
pub struct GutzUiPlugin<T: GutzUiScreen>(PhantomData<fn() -> T>);

impl<T: GutzUiScreen> Default for GutzUiPlugin<T> {
    fn default() -> Self {
        Self(PhantomData)
    }
}

impl<T: GutzUiScreen> Plugin for GutzUiPlugin<T> {
    fn build(&self, app: &mut App) {
        app.init_resource::<GutzUiStack<T>>()
            .add_message::<GutzUiScreenOpened<T>>()
            .add_message::<GutzUiScreenClosed<T>>()
            .add_systems(Update, detect_screen_changes::<T>);
    }
}

/// 全画面・中央寄せ・縦積みの「モーダルダイアログの枠」ビルダー。
///
/// Title/Pause/GameOverのような画面は、どのゲームでもほぼ同じ骨組み
/// （画面全体を覆うNode＋背景色のオーバーレイ＋中央に縦積みのテキスト行）を
/// 持つ。中身（Text等）はゲーム側が`.with_children`で足す——ここが持つのは
/// あくまで共通のNode/BackgroundColor設定という「部品」まで
/// （doc/gutzgutz-requirements.md「UI」節：ゲージ・ダイアログ等の**部品**は
/// スコープ内、ゲーム固有のHUD構成そのものはスコープ外）。
#[derive(Clone, Copy, Debug)]
pub struct GutzModalPanelStyle {
    pub background: Color,
    pub row_gap: Val,
}

impl Default for GutzModalPanelStyle {
    fn default() -> Self {
        Self { background: Color::srgba(0.0, 0.0, 0.0, 0.6), row_gap: Val::Px(12.0) }
    }
}

/// 全画面・中央寄せ・縦積みのパネル「枠」を1エンティティspawnして返す。
/// 呼び出し側は`.with_children(...)`で中身を足し、必要なら`.insert(...)`で
/// 画面を区別するマーカーComponentを追加する。
///
/// ```ignore
/// spawn_modal_panel(&mut commands, GutzModalPanelStyle::default())
///     .insert(ScreenUi)
///     .with_children(|root| {
///         root.spawn((Text::new("PAUSED"), ...));
///     });
/// ```
pub fn spawn_modal_panel<'a>(
    commands: &'a mut Commands,
    style: GutzModalPanelStyle,
) -> EntityCommands<'a> {
    commands.spawn((
        Node {
            width: Val::Percent(100.0),
            height: Val::Percent(100.0),
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            flex_direction: FlexDirection::Column,
            row_gap: style.row_gap,
            ..default()
        },
        BackgroundColor(style.background),
    ))
}
