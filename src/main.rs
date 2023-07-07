mod common;
mod editor;
mod game;
use common::{AppState, World};
use editor::add_editor_systems;
use game::add_game_systems;

use bevy::prelude::*;
use bevy_egui::EguiPlugin;
use bevy_rapier2d::prelude::*;

fn main() {
    let mut app = App::new();
    app.insert_resource(ClearColor(Color::WHITE))
        .init_resource::<World>()
        .add_state::<AppState>()
        .add_plugins(DefaultPlugins)
        .add_plugin(EguiPlugin)
        .add_plugin(RapierPhysicsPlugin::<NoUserData>::pixels_per_meter(100.0))
        .add_startup_system(setup_graphics);
    add_editor_systems(&mut app);
    add_game_systems(&mut app);
    app.run();
}

fn setup_graphics(mut commands: Commands) {
    commands.spawn(Camera2dBundle::default());
}
