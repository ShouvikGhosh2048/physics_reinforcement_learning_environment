pub mod dqn;
pub mod genetic;

use self::{
    dqn::{DQNAgent, DQNAlgorithm},
    genetic::{GeneticAgent, GeneticAlgorithm},
};
use crate::common::{Move, PhysicsEnvironment, World};

use bevy_egui::egui::Ui;
use crossbeam::channel::{bounded, Receiver};

pub fn spawn_training_thread(
    number_of_steps: usize,
    algorithm: &Algorithm,
    world: &World,
) -> Receiver<(f32, Agent)> {
    let (sender, reciever) = bounded(100);
    let world = (*world).clone();
    let algorithm = *algorithm;
    std::thread::spawn(move || match algorithm {
        Algorithm::Genetic(algorithm) => {
            algorithm.train(world, number_of_steps, sender);
        }
        Algorithm::Dqn(algorithm) => {
            algorithm.train(world, number_of_steps, sender);
        }
    });
    reciever
}

#[derive(PartialEq, Clone, Copy)]
pub enum Algorithm {
    Genetic(GeneticAlgorithm),
    Dqn(DQNAlgorithm),
}

impl Default for Algorithm {
    fn default() -> Self {
        Algorithm::Genetic(GeneticAlgorithm::default())
    }
}

impl Algorithm {
    pub fn algorithm_properties_ui(&mut self, ui: &mut Ui) {
        match self {
            Algorithm::Genetic(algorithm) => {
                algorithm.algorithm_properties_ui(ui);
            }
            Algorithm::Dqn(algorithm) => algorithm.algorithm_properties_ui(ui),
        }
    }
}

#[derive(Clone)]
pub enum Agent {
    Genetic(GeneticAgent),
    Dqn(DQNAgent),
}

impl Agent {
    pub fn get_move(&mut self, environment: &PhysicsEnvironment) -> Move {
        match self {
            Agent::Genetic(agent) => agent.get_move(),
            Agent::Dqn(agent) => agent.get_move(environment),
        }
    }
}
