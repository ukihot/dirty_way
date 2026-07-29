mod actions;
mod bubble;
mod consts;
mod enemy;
mod player;
mod quality;
mod scene;
mod soap;
mod state;
mod ui;

use avian3d::prelude::*;
use bevy::prelude::*;
use bevy_gutzgutz::atlas::GutzAtlasPlugin;
use bevy_gutzgutz::devtools::GutzDevtoolsPlugin;
use bevy_gutzgutz::input::GutzInputPlugin;
use bevy_gutzgutz::lifecycle::GutzLifeCyclePlugin;
use bevy_gutzgutz::save::GutzSavePlugin;
use bevy_gutzgutz::steam::GutzSteamPlugin;
use bevy_gutzgutz::ui::GutzUiPlugin;

fn main() {
    App::new()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window { title: "D WAY".into(), ..default() }),
            ..default()
        }))
        // FrameTimeDiagnosticsPlugin・FPS/統計オーバーレイはGutzDevtoolsPlugin
        // （bevy_gutzgutz、F3でトグル）が内包する。
        .add_plugins(GutzDevtoolsPlugin)
        .add_plugins(PhysicsPlugins::default())
        // 小さめのアリーナに合わせて重力を強めにし、泡の山なり弾道を短めにする。
        .insert_resource(Gravity(Vec3::NEG_Y * 14.0))
        .add_systems(Startup, actions::setup_input_bindings)
        .add_plugins((
            state::GameStatePlugin,
            // GameState(Playing/GameOver)のInGame/OutGame分類・Pause/Resume・
            // 遷移通知はgutzgutzへ委ねる（gutzgutzは状態そのものを持たず、
            // 状態を扱うための仕組みだけ持つ）。
            GutzLifeCyclePlugin::<state::GameState>::default(),
            // キー入力はAction（RotateLeft/RotateRight/Charge/Restart）を
            // 経由する。実際のキー割り当ては input.toml（読めない場合は
            // actions.rsのハードコード値）。
            GutzInputPlugin::<actions::PlayerAction, state::GameState>::default(),
            // 「今開いているUI画面」のスタック（今はGameOverのみ）。
            GutzUiPlugin::<ui::UiScreen>::default(),
            // ハイスコアの永続化。保存先はOS標準の場所
            // （Windows: %APPDATA%\ukihot\dirty_way\data\、macOS:
            // ~/Library/Application Support/dev.ukihot.dirty_way/、
            // Linux: ~/.local/share/dirty_way/。bevy_gutzgutz/README.mdの
            // `save`節参照）。
            GutzSavePlugin::<state::SaveData>::standard_location(
                "dev",
                "ukihot",
                "dirty_way",
                "save.toml",
            ),
            // Steam連携。実際のApp IDが割り当てられるまではSpaceWar
            // （開発・検証用のテストApp ID）を使う。Steamクライアント未起動
            // でもグレースフルデグレードするので、開発機で常時有効にして良い
            // （bevy_gutzgutz/README.mdの`steam`節参照）。
            GutzSteamPlugin::new(GutzSteamPlugin::DEV_APP_ID),
            // assets/character/配下から build.rs が生成した
            // assets/generated/atlas/manifest.toml をStartupで読み込む
            // （bevy_gutzgutz/README.mdの`atlas`節参照）。
            GutzAtlasPlugin,
            scene::ScenePlugin,
            player::PlayerPlugin,
            bubble::BubblePlugin,
            quality::QualityPlugin,
            soap::SoapPlugin,
            enemy::EnemyPlugin,
            ui::HudPlugin,
        ))
        .run();
}
