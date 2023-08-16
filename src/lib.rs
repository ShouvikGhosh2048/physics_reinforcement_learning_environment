#![allow(clippy::too_many_arguments, clippy::type_complexity)]

mod algorithm;
mod common;
mod editor;
mod game;
mod train;
use common::AppState;
use editor::add_editor_systems;
use game::add_game_systems;
use train::add_train_systems;

use bevy::prelude::*;
use bevy_egui::EguiPlugin;

pub use self::algorithm::Agent;
pub use self::algorithm::Algorithm;
pub use self::algorithm::TrainingDetails;
pub use self::common::Move;
pub use self::common::ObjectAndTransform;
pub use self::common::PhysicsEnvironment;
pub use self::common::World;
pub use self::common::WorldObject;

pub fn run<
    AgentType: Agent,
    Message: Send + Sync + 'static,
    TrainingDetailsType: TrainingDetails<AgentType, Message>,
    AlgorithmType: Algorithm<AgentType, Message, TrainingDetailsType>,
>() {
    let mut app = App::new();
    app.insert_resource(ClearColor(Color::WHITE))
        .init_resource::<World>()
        .add_state::<AppState>()
        .add_plugins(DefaultPlugins)
        .add_plugin(EguiPlugin)
        .add_startup_system(setup_graphics);
    add_editor_systems(&mut app);
    add_game_systems(&mut app);
    add_train_systems::<AgentType, Message, TrainingDetailsType, AlgorithmType>(&mut app);
    app.run();
}

fn setup_graphics(mut commands: Commands) {
    commands.spawn(Camera2dBundle::default());
}
