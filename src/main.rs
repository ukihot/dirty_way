mod bubble;
mod consts;
mod enemy;
mod player;
mod scene;
mod state;
mod ui;

use avian3d::prelude::*;
use bevy::prelude::*;

fn main() {
    App::new()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window { title: "D WAY".into(), ..default() }),
            ..default()
        }))
        .add_plugins(PhysicsPlugins::default())
        // 小さめのアリーナに合わせて重力を強めにし、泡の山なり弾道を短めにする。
        .insert_resource(Gravity(Vec3::NEG_Y * 14.0))
        .add_plugins((
            state::GameStatePlugin,
            scene::ScenePlugin,
            player::PlayerPlugin,
            bubble::BubblePlugin,
            enemy::EnemyPlugin,
            ui::HudPlugin,
        ))
        .run();
}
