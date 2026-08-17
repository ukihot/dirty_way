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

use avian2d::dynamics::solver::SolverConfig;
use avian2d::prelude::*;
use bevy::prelude::*;
use bevy_gutzgutz::atlas::GutzAtlasPlugin;
use bevy_gutzgutz::devtools::GutzDevtoolsPlugin;
use bevy_gutzgutz::session::GutzGameSessionPlugin;
use bevy_gutzgutz::steam::GutzSteamPlugin;

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
        .insert_resource(Gravity(Vec2::NEG_Y * 14.0))
        // 課題S-38（2026-07-30、実機フィードバック）：泡だまりが積み上がる際、
        // 泡と泡の間に隙間が残ったり、逆に接触した瞬間に上の泡が垂直方向へ
        // 異常な速度で弾き飛ばされて消えたりする不具合があった。原因は
        // Avianのデフォルト（`max_overlap_solve_speed = 4.0`）だと、重なりの
        // 解消が1フレームでかなり強く行われること。これを下げ、重なりを
        // 常に穏やかに・段階的に解消させることで、両方の症状を解消する
        // （bubble.rs::settle_landed_bubbles参照。以前試した「着地済みの
        // 泡はX方向をlockする」という対策は、Xがlockされた状態で重なりを
        // 解消しようとするとソルバーが補正をY方向だけに押し付けてしまい、
        // むしろ垂直方向への異常な弾き飛ばしを悪化させる原因だったため撤去した）。
        .insert_resource(SolverConfig { max_overlap_solve_speed: 1.0, ..default() })
        .add_systems(Startup, actions::setup_input_bindings)
        .add_plugins((
            state::GameStatePlugin,
            // セッションの共通基盤。ゲーム固有のState / Action / UI / SaveDataを
            // 宣言するだけで、lifecycle・input・UIスタック・保存要求を一方向に
            // 配線する。各型の意味、画面の見た目、実データへの反映はdirty_way側
            // に残す（キー割り当てはinput.toml、読めない場合はactions.rs）。
            GutzGameSessionPlugin::<
                state::GameState,
                actions::PlayerAction,
                ui::UiScreen,
                state::SaveData,
            >::standard_save_location("dev", "ukihot", "dirty_way", "save.toml"),
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
