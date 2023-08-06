// Implementation of DQN.
// The implementation has been taken from:
// https://github.com/coreylowman/dfdx/blob/main/examples/rl-dqn.rs
// https://pytorch.org/tutorials/intermediate/reinforcement_q_learning.html

use crate::common::{Move, PhysicsEnvironment, World};
use super::Agent;

use std::collections::VecDeque;
use bevy_egui::egui::{Ui, DragValue};
use crossbeam::channel::Sender;
use rand::prelude::*;
use dfdx::{
    optim::Sgd,
    prelude::{huber_loss, DeviceBuildExt, Linear, Module, Optimizer, ReLU, ZeroGrads},
    shapes::{Rank1, Rank2},
    tensor::{AsArray, AutoDevice, Tensor, TensorFrom, Trace},
    tensor_ops::{Backward, MaxTo, Momentum, SelectTo, SgdConfig},
};

type QNetwork = ((Linear<4, 32>, ReLU), (Linear<32, 32>, ReLU), Linear<32, 8>);
type QNetworkModel = (
    (dfdx::prelude::modules::Linear<4, 32, f32, AutoDevice>, ReLU),
    (
        dfdx::prelude::modules::Linear<32, 32, f32, AutoDevice>,
        ReLU,
    ),
    dfdx::prelude::modules::Linear<32, 8, f32, AutoDevice>,
);

#[derive(Clone)]
pub struct DQNAgent {
    dqn: QNetworkModel,
    curr: (Move, usize),
    repeat_move: usize,
    dev: AutoDevice,
}

impl DQNAgent {
    pub fn get_move(&mut self, environment: &PhysicsEnvironment) -> Move {
        if self.curr.1 < self.repeat_move {
            self.curr.1 += 1;
            self.curr.0
        } else {
            let state = self.dev.tensor(environment.state().unwrap());
            let q_values = self.dqn.forward(state);
            let mut max_q_index = 0;
            for i in 1..8 {
                if q_values[[max_q_index]] < q_values[[i]] {
                    max_q_index = i;
                }
            }
            let player_move = Move {
                left: (max_q_index & 1) == 0,
                right: (max_q_index & 2) == 0,
                up: (max_q_index & 4) == 0,
            };
            self.curr = (player_move, 1);
            player_move
        }
    }
}

#[derive(PartialEq, Clone, Copy)]
pub struct DQNAlgorithm {
    repeat_move: usize,
}

impl Default for DQNAlgorithm {
    fn default() -> Self {
        DQNAlgorithm {
            repeat_move: 20,
        }
    }
}

impl DQNAlgorithm {
    pub fn algorithm_properties_ui(&mut self, ui: &mut Ui) {
        ui.label("Repeat move: ");
        ui.add(DragValue::new(&mut self.repeat_move).clamp_range(1..=100));
        ui.end_row();
    }

    pub fn train(&self, world: World, number_of_steps: usize, sender: Sender<(f32, Agent)>) {
        let mut rng = thread_rng();
        
        let dev = AutoDevice::default();
        let mut q_net = dev.build_module::<QNetwork, f32>();
        let mut target_q_net = q_net.clone();

        let mut grads = q_net.alloc_grads();

        let mut sgd = Sgd::new(
            &q_net,
            SgdConfig {
                lr: 1e-1,
                momentum: Some(Momentum::Nesterov(0.9)),
                weight_decay: None,
            },
        );

        let mut state_actions = VecDeque::new();

        for game in 0_usize.. {
            if game % 1000 == 0 {
                let mut agent = DQNAgent {
                    dqn: q_net.clone(),
                    curr: (Move::default(), self.repeat_move),
                    repeat_move: self.repeat_move,
                    dev: AutoDevice::default()
                };
                let mut environment = PhysicsEnvironment::from_world(&world);
                let mut score = f32::INFINITY;
                for _ in 0..number_of_steps {
                    let player_move = agent.get_move(&environment);
                    environment.step(player_move);
                    score = score.min(environment.distance_to_goals().unwrap());
                    if environment.won {
                        break;
                    }
                }

                let agent = Agent::DQN(DQNAgent {
                    dqn: q_net.clone(),
                    curr: (Move::default(), self.repeat_move),
                    repeat_move: self.repeat_move,
                    dev: AutoDevice::default()
                });
                if sender.send((score, agent)).is_err() {
                    return;
                }
            }

            let mut environment = PhysicsEnvironment::from_world(&world);
            for _ in 0..number_of_steps/self.repeat_move {
                let state = dev.tensor(environment.state().unwrap());
                let q_values = q_net.forward(state.clone());

                let mut max_q_index = 0;
                for i in 1..8 {
                    if q_values[[max_q_index]] < q_values[[i]] {
                        max_q_index = i;
                    }
                }
                let action_index = if rng.gen::<f32>() < (-(game as f32) / 10000.0).exp() {
                    rng.gen::<usize>() % 8
                } else {
                    max_q_index
                };

                let previous_score = environment.distance_to_goals().unwrap();
                for _ in 0..self.repeat_move {
                    environment.step(Move {
                        left: (action_index & 1) == 0,
                        right: (action_index & 2) == 0,
                        up: (action_index & 4) == 0,
                    });
                }
                let reward = previous_score - environment.distance_to_goals().unwrap();

                let next_state = dev.tensor(environment.state().unwrap());
                state_actions.push_back((
                    state.array(),
                    action_index,
                    reward,
                    next_state.array(),
                ));
                if state_actions.len() == 10000 {
                    state_actions.pop_front();
                }

                const BATCH_SIZE: usize = 1000;
                if state_actions.len() < BATCH_SIZE {
                    continue;
                }
                let batch = state_actions.iter().choose_multiple(&mut rng, BATCH_SIZE);
                let states = batch
                    .iter()
                    .flat_map(|(state, _, _, _)| state.iter().map(|x| *x))
                    .collect::<Vec<_>>();
                let states: Tensor<Rank2<BATCH_SIZE, 4>, _, _> = dev.tensor(states);
                let next_states = batch
                    .iter()
                    .flat_map(|(_, _, _, next_state)| next_state.iter().map(|x| *x))
                    .collect::<Vec<_>>();
                let next_states: Tensor<Rank2<BATCH_SIZE, 4>, _, _> =
                    dev.tensor(next_states);
                let rewards = batch
                    .iter()
                    .map(|(_, _, reward, _)| *reward)
                    .collect::<Vec<_>>();
                let rewards: Tensor<Rank1<BATCH_SIZE>, _, _> = dev.tensor(rewards);
                let actions = batch
                    .iter()
                    .map(|(_, action, _, _)| *action)
                    .collect::<Vec<_>>();
                let actions: Tensor<Rank1<BATCH_SIZE>, _, _> = dev.tensor(actions);

                let q_values = q_net.forward(states.trace(grads));
                let action_qs = q_values.select(actions.clone());

                let next_q_values = target_q_net.forward(next_states.clone());
                let max_next_q = next_q_values.max::<Rank1<BATCH_SIZE>, _>();
                let target_q = max_next_q * 0.99 + rewards.clone();

                let loss = huber_loss(action_qs, target_q, 1.0);

                grads = loss.backward();

                sgd.update(&mut q_net, &grads).expect("Unused params");
                q_net.zero_grads(&mut grads);

                target_q_net.0.0.weight = target_q_net.0.0.weight * 0.99 + q_net.0.0.weight.clone() * 0.01;
                target_q_net.0.0.bias = target_q_net.0.0.bias * 0.99 + q_net.0.0.bias.clone() * 0.01;
                target_q_net.1.0.weight = target_q_net.1.0.weight * 0.99 + q_net.1.0.weight.clone() * 0.01;
                target_q_net.1.0.bias = target_q_net.1.0.bias * 0.99 + q_net.1.0.bias.clone() * 0.01;
                target_q_net.2.weight = target_q_net.2.weight * 0.99 + q_net.2.weight.clone() * 0.01;
                target_q_net.2.bias = target_q_net.2.bias * 0.99 + q_net.2.bias.clone() * 0.01;
            }
        }
    }
}
