//! This crate provides a physics based environment and level editor
//! for training reinforcement learning agents.
//!
//! # Using the crate
//! Here is an example using the crate.
//! ```no_run
//! // We define our agent.
//! use physics_reinforcement_learning_environment::{
//!     Move, World, Environment,
//!     egui::{self, Ui}, Sender, Receiver,
//!     Agent, TrainingDetails, Algorithm, run
//! };
//! #[derive(Clone)]
//! pub struct SingleMoveAgent {
//!     player_move: Move,
//! }
//!
//! // We implement the Agent trait for our agent.
//! impl Agent for SingleMoveAgent {
//!     fn get_move(&mut self, _environment: &Environment) -> Move {
//!         self.player_move
//!     }
//!
//!     // Show the agent details UI. Uses egui for the UI.
//!     fn details_ui(&self, ui: &mut Ui, environment: &Environment) {
//!         ui.label(format!("Move: {:?}", self.player_move));
//!     }
//! }
//!
//! // The training takes places on a seperate thread.
//! // We define the message we send from the training thread to
//! // the visualization thread.
//! // Here we send over the agent and the minimum distance during training.
//! type SingleMoveMessage = (SingleMoveAgent, f32);
//!
//! // We define the struct which keeps track of the training details.
//! struct SingleMoveTrainingDetails {
//!     agents: Vec<(SingleMoveAgent, f32)>,
//!     receiver: Receiver<SingleMoveMessage>,
//! }
//!
//! // We implement the TrainingDetails trait for the struct.
//! impl TrainingDetails<SingleMoveAgent, SingleMoveMessage> for SingleMoveTrainingDetails {
//!     // Receive messages from the training thread.
//!     fn receive_messages(&mut self) {
//!         self.agents.extend(self.receiver.try_iter().take(1000));
//!     }
//!
//!     // Show the training details UI. Uses egui for the UI.
//!     // This method returns an Option<&SingleMoveAgent>
//!     // - if an agent is returned, the app will change to
//!     // visualize the agent.
//!     fn details_ui(&mut self, ui: &mut Ui) -> Option<&SingleMoveAgent> {
//!         let mut selected_agent = None;
//!         for (agent, score) in self.agents.iter() {
//!             ui.horizontal(|ui| {
//!                 ui.label(format!("Score {score}"));
//!                 if ui.button("Visualise agent").clicked() {
//!                     selected_agent = Some(agent);
//!                 }
//!             });
//!         }
//!         selected_agent
//!     }
//! }
//!
//! // We define the struct representing the algorithm.
//! #[derive(PartialEq, Clone, Copy)]
//! pub struct SingleMoveAlgorithm {
//!     number_of_steps: usize,
//! }
//!
//! impl Default for SingleMoveAlgorithm {
//!     fn default() -> Self {
//!         SingleMoveAlgorithm {
//!             number_of_steps: 1000,
//!         }
//!     }
//! }
//!
//! // We implement the Algorithm trait for the struct.
//! impl Algorithm<SingleMoveAgent, SingleMoveMessage, SingleMoveTrainingDetails> for SingleMoveAlgorithm {
//!     // Note that the application can drop the receiver when it doesn't
//!     // want to receive any more messages.
//!     // The application doesn't stop the training thread - it's your responsibillity
//!     // to return if you detect that the receiver is dropped.
//!     fn train(&self, world: World, sender: Sender<SingleMoveMessage>) {
//!         for left in [false, true] {
//!             for right in [false, true] {
//!                 for up in [false, true] {
//!                     let player_move = Move {
//!                         left,
//!                         right,
//!                         up
//!                     };
//!
//!                     let (mut environment, _) = Environment::from_world(&world);
//!                     let mut score = f32::INFINITY;
//!                     for _ in 0..self.number_of_steps {
//!                         environment.step(player_move);
//!                         score = score.min(environment.distance_to_goals().unwrap());
//!                         
//!                         if environment.won() {
//!                             break;
//!                         }
//!                     }
//!
//!                     if sender
//!                         .send((
//!                             SingleMoveAgent { player_move },
//!                             score
//!                         ))
//!                         .is_err() {
//!                         // Can't send a message, so we return.
//!                         return;
//!                     }
//!                 }
//!             }
//!         }
//!     }
//!
//!     // UI for algorithm selection.
//!     fn selection_ui(&mut self, ui: &mut Ui) {
//!         ui.label("Number of steps: ");
//!         ui.add(egui::DragValue::new(&mut self.number_of_steps).clamp_range(1..=10000));    
//!     }
//!
//!     // Function which takes a Receiver receiving messages from
//!     // the training thread and returns the TrainingDetails.
//!     fn training_details_receiver(
//!         &self,
//!         receiver: Receiver<SingleMoveMessage>,
//!     ) -> SingleMoveTrainingDetails {
//!         SingleMoveTrainingDetails {
//!             agents: vec![],
//!             receiver,
//!         }
//!     }
//! }
//!
//! run::<SingleMoveAgent, SingleMoveMessage, SingleMoveTrainingDetails, SingleMoveAlgorithm>();
//! ```

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
pub use self::common::Environment;
pub use self::common::Move;
pub use self::common::ObjectAndTransform;
pub use self::common::World;
pub use self::common::WorldObject;
pub use bevy_egui::egui;
pub use crossbeam::channel::{Receiver, Sender};
pub use rapier2d;

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
