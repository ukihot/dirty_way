use bevy::prelude::*;
use bevy_gutzgutz::devtools::GutzDebugStats;
use bevy_gutzgutz::lifecycle::in_game;
use bevy_gutzgutz::ui::{GutzUiScreenClosed, GutzUiScreenOpened, GutzUiStack};

use crate::bubble::FoamGpuBinding;
use crate::consts::CHARGE_MAX_TIME;
use crate::player::Charge;
use crate::quality::FoamQualitySetting;
use crate::state::{GameState, Health, SaveData, Score};

/// dirty_wayの「今開いているUI画面」（`GutzUiPlugin`用）。今はGameOverの
/// リザルト表示だけだが、見た目のスポーン/despawnとスタックの出し入れを
/// 分離しておくことで、将来PauseMenu等が増えても構造はそのまま拡張できる。
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum UiScreen {
    GameOver,
}

#[derive(Component)]
struct ScoreText;
#[derive(Component)]
struct HealthText;
#[derive(Component)]
struct ChargeBarFill;
#[derive(Component)]
struct GameOverUi;

pub struct HudPlugin;

impl Plugin for HudPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, setup_hud)
            .add_systems(
                Update,
                (update_score_text, update_health_text, update_charge_bar)
                    .run_if(in_game::<GameState>()),
            )
            // Foam数/Qualityというゲーム固有のベンチマーク値（doc/soap-issues.md
            // S-11a）は、GutzDevtoolsPlugin（F3）のオーバーレイにGutzDebugStats
            // 経由で載せる（FPS/Frameはgutzgutz自身が標準エントリとしてsetする）。
            // GameOver中も見えていた方が都合が良いので、Playing限定にしない。
            .add_systems(Update, update_gutz_debug_stats)
            // GameStateの遷移はUiScreenスタックの出し入れだけを行い、実際の
            // スポーン/despawnはGutzUiScreenOpened/Closedを購読して行う
            // （doc：「UIを操作可能な状態機械として扱う」）。
            .add_systems(OnEnter(GameState::GameOver), open_game_over_screen)
            .add_systems(OnEnter(GameState::Playing), close_game_over_screen)
            .add_systems(Update, (spawn_game_over_ui, despawn_game_over_ui));
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

    // 下部中央：チャージゲージ
    commands
        .spawn(Node {
            width: Val::Percent(100.0),
            height: Val::Percent(100.0),
            flex_direction: FlexDirection::Column,
            justify_content: JustifyContent::FlexEnd,
            align_items: AlignItems::Center,
            padding: UiRect::bottom(Val::Px(24.0)),
            ..default()
        })
        .with_children(|root| {
            root.spawn((
                Node {
                    width: Val::Px(240.0),
                    height: Val::Px(18.0),
                    border: UiRect::all(Val::Px(2.0)),
                    ..default()
                },
                BackgroundColor(Color::srgba(0.1, 0.1, 0.1, 0.6)),
                BorderColor::all(Color::WHITE),
            ))
            .with_children(|track| {
                track.spawn((
                    ChargeBarFill,
                    Node { width: Val::Percent(0.0), height: Val::Percent(100.0), ..default() },
                    BackgroundColor(Color::srgb(0.3, 0.85, 1.0)),
                ));
            });
        });
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

fn update_charge_bar(charge: Res<Charge>, mut query: Query<&mut Node, With<ChargeBarFill>>) {
    let fraction = (charge.time / CHARGE_MAX_TIME).clamp(0.0, 1.0) * 100.0;
    if let Ok(mut node) = query.single_mut() {
        node.width = Val::Percent(if charge.charging { fraction } else { 0.0 });
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

fn open_game_over_screen(mut stack: ResMut<GutzUiStack<UiScreen>>) {
    stack.push(UiScreen::GameOver);
}

fn close_game_over_screen(mut stack: ResMut<GutzUiStack<UiScreen>>) {
    stack.pop();
}

fn spawn_game_over_ui(
    mut commands: Commands,
    mut opened: MessageReader<GutzUiScreenOpened<UiScreen>>,
    score: Res<Score>,
    save_data: Res<SaveData>,
) {
    for GutzUiScreenOpened(screen) in opened.read() {
        if *screen != UiScreen::GameOver {
            continue;
        }
        commands
            .spawn((
                GameOverUi,
                Node {
                    width: Val::Percent(100.0),
                    height: Val::Percent(100.0),
                    justify_content: JustifyContent::Center,
                    align_items: AlignItems::Center,
                    flex_direction: FlexDirection::Column,
                    row_gap: Val::Px(12.0),
                    ..default()
                },
                BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.55)),
            ))
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
}

fn despawn_game_over_ui(
    mut commands: Commands,
    mut closed: MessageReader<GutzUiScreenClosed<UiScreen>>,
    query: Query<Entity, With<GameOverUi>>,
) {
    for GutzUiScreenClosed(screen) in closed.read() {
        if *screen != UiScreen::GameOver {
            continue;
        }
        for entity in &query {
            commands.entity(entity).despawn();
        }
    }
}
