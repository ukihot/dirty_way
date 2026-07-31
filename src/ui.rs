use bevy::prelude::*;
use bevy_gutzgutz::devtools::GutzDebugStats;
use bevy_gutzgutz::lifecycle::{GutzPaused, in_game};
use bevy_gutzgutz::ui::{
    GutzModalPanelStyle, GutzUiScreenClosed, GutzUiScreenOpened, GutzUiStack, spawn_modal_panel,
};

use crate::bubble::FoamGpuBinding;
use crate::quality::FoamQualitySetting;
use crate::state::{GameState, Health, SaveData, Score};

/// dirty_wayの「今開いているUI画面」（`GutzUiPlugin`用）。スタックの出し入れ
/// （`open_*_screen`/`close_*_screen`）とスタック変化に応じた実際の見た目の
/// スポーン/despawn（`spawn_screen_ui`/`despawn_screen_ui`）を分離しておく
/// ことで、画面が増えても構造はそのまま拡張できる（doc：「UIを操作可能な
/// 状態機械として扱う」）。
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum UiScreen {
    Title,
    /// `GameState`の遷移ではなく`GutzPaused`の変化で出し入れする
    /// （ポーズはGameStateと直交する別軸——`sync_paused_screen`参照）。
    Paused,
    GameOver,
}

#[derive(Component)]
struct ScoreText;
#[derive(Component)]
struct HealthText;
/// Title/Paused/GameOverのどの画面ルートにも共通して付ける印。1画面しか
/// 同時に開かない（`GutzUiStack`は常にLIFOで1枚だけアクティブ）前提で、
/// despawn側は「どの画面が閉じたか」を区別せず、開いているルートを全部
/// 畳めばよい。
#[derive(Component)]
struct ScreenUi;

pub struct HudPlugin;

impl Plugin for HudPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, setup_hud)
            .add_systems(
                Update,
                (update_score_text, update_health_text).run_if(in_game::<GameState>()),
            )
            // Foam数/Qualityというゲーム固有のベンチマーク値（doc/soap-issues.md
            // S-11a）は、GutzDevtoolsPlugin（F3）のオーバーレイにGutzDebugStats
            // 経由で載せる（FPS/Frameはgutzgutz自身が標準エントリとしてsetする）。
            // GameOver中も見えていた方が都合が良いので、Playing限定にしない。
            .add_systems(Update, update_gutz_debug_stats)
            // GameStateの遷移・GutzPausedの変化はUiScreenスタックの出し入れ
            // だけを行い、実際のスポーン/despawnはGutzUiScreenOpened/Closed
            // を購読して行う。
            .add_systems(OnEnter(GameState::Title), open_title_screen)
            .add_systems(OnEnter(GameState::GameOver), open_game_over_screen)
            .add_systems(OnEnter(GameState::Playing), close_outgame_screen)
            .add_systems(Update, sync_paused_screen.run_if(in_game::<GameState>()))
            .add_systems(Update, (spawn_screen_ui, despawn_screen_ui));
    }
}

fn setup_hud(mut commands: Commands) {
    // 左上：スコア／体力
    commands
        .spawn(Node {
            width: Val::Percent(100.0),
            height: Val::Percent(100.0),
            flex_direction: FlexDirection::Column,
            padding: UiRect::all(Val::Px(16.0)),
            row_gap: Val::Px(6.0),
            ..default()
        })
        .with_children(|root| {
            root.spawn((
                ScoreText,
                Text::new("Score: 0"),
                TextFont { font_size: FontSize::Px(28.0), ..default() },
                TextColor(Color::WHITE),
            ));
            root.spawn((
                HealthText,
                Text::new("HP: 5"),
                TextFont { font_size: FontSize::Px(24.0), ..default() },
                TextColor(Color::srgb(1.0, 0.6, 0.7)),
            ));
        });
    // 課題S-19（2026-07-29）：チャージ→離すで1発、という仕組みをやめ、本物の
    // ハンドソープ容器のポンプのように「押している間ノズルが沈み込みながら
    // 連続噴射し、沈みきったら止まる」方式にしたため、残量ゲージのUIは
    // 不要になった——ノズル自体の高さ（player.rs::update_player_visual）が
    // そのまま残量表示を兼ねる。
}

fn update_score_text(score: Res<Score>, mut query: Query<&mut Text, With<ScoreText>>) {
    if !score.is_changed() {
        return;
    }
    if let Ok(mut text) = query.single_mut() {
        text.0 = format!("Score: {}", score.0);
    }
}

fn update_health_text(health: Res<Health>, mut query: Query<&mut Text, With<HealthText>>) {
    if !health.is_changed() {
        return;
    }
    if let Ok(mut text) = query.single_mut() {
        text.0 = format!("HP: {}", health.0.max(0));
    }
}

fn update_gutz_debug_stats(
    foam_aggregates: Query<(), With<FoamGpuBinding>>,
    quality: Res<FoamQualitySetting>,
    mut stats: ResMut<GutzDebugStats>,
) {
    stats.set("Foam", foam_aggregates.iter().count());
    stats.set("Quality", quality.0.label());
}

fn open_title_screen(mut stack: ResMut<GutzUiStack<UiScreen>>) {
    stack.push(UiScreen::Title);
}

fn open_game_over_screen(mut stack: ResMut<GutzUiStack<UiScreen>>) {
    stack.push(UiScreen::GameOver);
}

/// PlayingへのOnEnterはTitleからでもGameOverからでも起こりうるが、
/// どちらの場合も「直前まで開いていたOutGame画面を閉じる」という
/// 意味では同じ処理でよい（`GutzUiStack`は中身を区別せず一番上を畳むだけ）。
fn close_outgame_screen(mut stack: ResMut<GutzUiStack<UiScreen>>) {
    stack.pop();
}

/// `GutzPaused`（`GutzLifeCyclePlugin`が管理）の変化を見て、Paused画面を
/// スタックへ出し入れするだけの薄い同期システム。ポーズはGameStateの
/// 遷移では起きない（Playingのまま`GutzPaused`だけが変わる）ため、
/// OnEnter/OnExitではなくここで能動的に監視する。
fn sync_paused_screen(paused: Res<GutzPaused>, mut stack: ResMut<GutzUiStack<UiScreen>>) {
    if !paused.is_changed() {
        return;
    }
    if paused.0 {
        stack.push(UiScreen::Paused);
    } else {
        stack.pop();
    }
}

fn spawn_screen_ui(
    mut commands: Commands,
    mut opened: MessageReader<GutzUiScreenOpened<UiScreen>>,
    score: Res<Score>,
    save_data: Res<SaveData>,
) {
    for GutzUiScreenOpened(screen) in opened.read() {
        match screen {
            UiScreen::Title => spawn_title_ui(&mut commands, &save_data),
            UiScreen::Paused => spawn_paused_ui(&mut commands),
            UiScreen::GameOver => spawn_game_over_ui(&mut commands, &score, &save_data),
        }
    }
}

fn despawn_screen_ui(
    mut commands: Commands,
    mut closed: MessageReader<GutzUiScreenClosed<UiScreen>>,
    query: Query<Entity, With<ScreenUi>>,
) {
    // 1画面しか同時に開かない前提なので、閉じたのがどの画面かは問わず
    // 開いているScreenUiを畳めばよい。
    if closed.read().count() == 0 {
        return;
    }
    for entity in &query {
        commands.entity(entity).despawn();
    }
}

fn spawn_title_ui(commands: &mut Commands, save_data: &SaveData) {
    spawn_modal_panel(
        commands,
        GutzModalPanelStyle { background: Color::srgb(0.05, 0.05, 0.08), ..default() },
    )
    .insert(ScreenUi)
    .with_children(|root| {
        root.spawn((
            Text::new("D WAY"),
            TextFont { font_size: FontSize::Px(64.0), ..default() },
            TextColor(Color::srgb(0.3, 0.85, 1.0)),
        ));
        root.spawn((
            Text::new(format!("High Score: {}", save_data.high_score)),
            TextFont { font_size: FontSize::Px(20.0), ..default() },
            TextColor(Color::srgb(1.0, 0.85, 0.4)),
        ));
        root.spawn((
            Text::new("Press R to Start"),
            TextFont { font_size: FontSize::Px(20.0), ..default() },
            TextColor(Color::srgb(0.8, 0.8, 0.8)),
        ));
    });
}

fn spawn_paused_ui(commands: &mut Commands) {
    spawn_modal_panel(commands, GutzModalPanelStyle::default()).insert(ScreenUi).with_children(
        |root| {
            root.spawn((
                Text::new("PAUSED"),
                TextFont { font_size: FontSize::Px(48.0), ..default() },
                TextColor(Color::WHITE),
            ));
            root.spawn((
                Text::new("Press Escape to Resume"),
                TextFont { font_size: FontSize::Px(20.0), ..default() },
                TextColor(Color::srgb(0.8, 0.8, 0.8)),
            ));
            root.spawn((
                Text::new("Press Q to Quit to Title"),
                TextFont { font_size: FontSize::Px(20.0), ..default() },
                TextColor(Color::srgb(0.8, 0.8, 0.8)),
            ));
        },
    );
}

fn spawn_game_over_ui(commands: &mut Commands, score: &Score, save_data: &SaveData) {
    spawn_modal_panel(
        commands,
        GutzModalPanelStyle { background: Color::srgba(0.0, 0.0, 0.0, 0.55), ..default() },
    )
    .insert(ScreenUi)
    .with_children(|root| {
        root.spawn((
            Text::new("GAME OVER"),
            TextFont { font_size: FontSize::Px(56.0), ..default() },
            TextColor(Color::srgb(1.0, 0.4, 0.5)),
        ));
        root.spawn((
            Text::new(format!("Score: {}", score.0)),
            TextFont { font_size: FontSize::Px(28.0), ..default() },
            TextColor(Color::WHITE),
        ));
        root.spawn((
            Text::new(format!("High Score: {}", save_data.high_score)),
            TextFont { font_size: FontSize::Px(20.0), ..default() },
            TextColor(Color::srgb(1.0, 0.85, 0.4)),
        ));
        root.spawn((
            Text::new("Press R to Restart"),
            TextFont { font_size: FontSize::Px(20.0), ..default() },
            TextColor(Color::srgb(0.8, 0.8, 0.8)),
        ));
    });
}
