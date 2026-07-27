use bevy::prelude::*;

use crate::bubble::Bubble;
use crate::consts::PLAYER_MAX_HEALTH;
use crate::enemy::{Enemy, EnemySpawnTimer};
use crate::player::Charge;

#[derive(Clone, Copy, Eq, PartialEq, Hash, Debug, Default, States)]
pub enum GameState {
    #[default]
    Playing,
    GameOver,
}

#[derive(Resource, Default)]
pub struct Score(pub u32);

#[derive(Resource)]
pub struct Health(pub i32);

impl Default for Health {
    fn default() -> Self {
        Health(PLAYER_MAX_HEALTH)
    }
}

pub struct GameStatePlugin;

impl Plugin for GameStatePlugin {
    fn build(&self, app: &mut App) {
        app.init_state::<GameState>()
            .init_resource::<Score>()
            .init_resource::<Health>()
            .add_systems(OnEnter(GameState::Playing), reset_game)
            .add_systems(Update, restart_input.run_if(in_state(GameState::GameOver)));
    }
}

/// Playing に入るたび（起動時＆リスタート時）に状態を初期化する。
fn reset_game(
    mut commands: Commands,
    mut score: ResMut<Score>,
    mut health: ResMut<Health>,
    mut charge: ResMut<Charge>,
    mut spawn_timer: ResMut<EnemySpawnTimer>,
    enemies: Query<Entity, With<Enemy>>,
    bubbles: Query<Entity, With<Bubble>>,
) {
    score.0 = 0;
    *health = Health::default();
    *charge = Charge::default();
    *spawn_timer = EnemySpawnTimer::default();

    for entity in &enemies {
        commands.entity(entity).despawn();
    }
    for entity in &bubbles {
        commands.entity(entity).despawn();
    }
}

fn restart_input(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut next_state: ResMut<NextState<GameState>>,
) {
    if keyboard.just_pressed(KeyCode::KeyR) {
        next_state.set(GameState::Playing);
    }
}
