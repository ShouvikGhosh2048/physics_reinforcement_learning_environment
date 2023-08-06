use super::Agent;
use crate::common::{Move, PhysicsEnvironment, World};

use bevy_egui::egui::{DragValue, Ui};
use crossbeam::channel::Sender;
use rand::prelude::*;
use std::cmp::Ordering;

#[derive(Clone)]
pub struct GeneticAgent {
    moves: Vec<Move>,
    curr: usize,
    repeat_move: usize,
}

impl GeneticAgent {
    pub fn get_move(&mut self) -> Move {
        if self.curr / self.repeat_move < self.moves.len() {
            let player_move = self.moves[self.curr / self.repeat_move];
            self.curr += 1;
            player_move
        } else {
            Move::default()
        }
    }
}

#[derive(PartialEq, Clone, Copy)]
pub struct GeneticAlgorithm {
    number_of_agents: usize,
    repeat_move: usize,
    mutation_rate: f32,
    keep_best: bool,
}

impl Default for GeneticAlgorithm {
    fn default() -> Self {
        GeneticAlgorithm {
            number_of_agents: 1000,
            repeat_move: 20,
            mutation_rate: 0.1,
            keep_best: false,
        }
    }
}

impl GeneticAlgorithm {
    pub fn algorithm_properties_ui(&mut self, ui: &mut Ui) {
        ui.label("Number of agents: ");
        ui.add(DragValue::new(&mut self.number_of_agents).clamp_range(10..=1000));
        ui.end_row();
        ui.label("Repeat move: ");
        ui.add(DragValue::new(&mut self.repeat_move).clamp_range(1..=100));
        ui.end_row();
        ui.label("Mutation rate: ");
        ui.add(DragValue::new(&mut self.mutation_rate).clamp_range(0.0..=1.0));
        ui.end_row();
        ui.label("Keep best from previous generation: ");
        ui.checkbox(&mut self.keep_best, "");
        ui.end_row();
    }

    pub fn train(&self, world: World, number_of_steps: usize, sender: Sender<(f32, Agent)>) {
        let mut rng = thread_rng();

        let agent_score = |agent: &Vec<Move>| {
            let mut environment = PhysicsEnvironment::from_world(&world);
            let mut score = f32::INFINITY;
            for player_move in agent.iter() {
                for _ in 0..self.repeat_move {
                    environment.step(*player_move);
                    score = score.min(environment.distance_to_goals().unwrap());

                    if environment.won {
                        break;
                    }
                }

                if environment.won {
                    break;
                }
            }
            for _ in 0..number_of_steps % self.repeat_move {
                environment.step(Move::default());
                score = score.min(environment.distance_to_goals().unwrap());

                if environment.won {
                    break;
                }
            }
            score
        };

        let mut generation = vec![];
        for _ in 0..self.number_of_agents {
            let mut agent = vec![];
            for _ in 0..number_of_steps / self.repeat_move {
                agent.push(Move {
                    left: rng.gen(),
                    right: rng.gen(),
                    up: rng.gen(),
                });
            }

            generation.push((agent_score(&agent), agent));
        }

        loop {
            let min_agent = generation
                .iter()
                .min_by(|(score1, _), (score2, _)| {
                    if score1 < score2 {
                        Ordering::Less
                    } else if score1 > score2 {
                        Ordering::Greater
                    } else {
                        Ordering::Equal
                    }
                })
                .unwrap();
            let max_score = generation
                .iter()
                .max_by(|(score1, _), (score2, _)| {
                    if score1 < score2 {
                        Ordering::Less
                    } else if score1 > score2 {
                        Ordering::Greater
                    } else {
                        Ordering::Equal
                    }
                })
                .unwrap()
                .0;
            if sender
                .send((
                    min_agent.0,
                    Agent::Genetic(GeneticAgent {
                        moves: min_agent.1.clone(),
                        curr: 0,
                        repeat_move: self.repeat_move,
                    }),
                ))
                .is_err()
            {
                return;
            }

            let mut new_generation = if self.keep_best {
                vec![min_agent.clone()]
            } else {
                vec![]
            };
            let additional_agents = self.number_of_agents - new_generation.len();

            for _ in 0..additional_agents {
                let mut parents = generation
                    .choose_multiple_weighted(&mut rng, 2, |(score, _)| max_score + 1.0 - score)
                    .unwrap();
                let parent1 = &parents.next().unwrap().1;
                let parent2 = &parents.next().unwrap().1;

                let mut agent = vec![];
                for i in 0..number_of_steps / self.repeat_move {
                    if rng.gen() {
                        agent.push(parent1[i]);
                    } else {
                        agent.push(parent2[i]);
                    }
                }
                for player_move in agent.iter_mut() {
                    if rng.gen::<f32>() < self.mutation_rate {
                        player_move.left = rng.gen();
                    }
                    if rng.gen::<f32>() < self.mutation_rate {
                        player_move.right = rng.gen();
                    }
                    if rng.gen::<f32>() < self.mutation_rate {
                        player_move.up = rng.gen();
                    }
                }
                new_generation.push((agent_score(&agent), agent));
            }
            generation = new_generation;
        }
    }
}
