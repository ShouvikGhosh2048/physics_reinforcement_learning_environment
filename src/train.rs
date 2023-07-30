use std::cmp::Ordering;

use crate::common::{
    AppState, Move, PhysicsEnvironment, World, WorldObject, BEVY_TO_PHYSICS_SCALE, PLAYER_DEPTH,
    PLAYER_RADIUS,
};

use bevy::{prelude::*, sprite::MaterialMesh2dBundle};
use bevy_egui::{egui, EguiContexts};
use crossbeam::channel::{bounded, Receiver};
use rand::prelude::*;
use rapier2d::prelude::*;

pub fn add_train_systems(app: &mut App) {
    app.add_systems((ui_system, update_visualization).in_set(OnUpdate(AppState::Train)))
        .add_system(cleanup_train.in_schedule(OnExit(AppState::Train)))
        .insert_resource(UiState::default());
}

fn ui_system(
    mut contexts: EguiContexts,
    mut ui_state: ResMut<UiState>,
    mut next_state: ResMut<NextState<AppState>>,
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
    world: Res<World>,
    visualization_objects: Query<Entity, With<VisualizationObject>>,
) {
    egui::Window::new("Train agents")
        .scroll2([false, true])
        .show(contexts.ctx_mut(), |ui| {
            let UiState {
                agent_reciever,
                agents,
                ..
            } = &mut *ui_state;
            if let Some(agent_reciever) = agent_reciever {
                agents.extend(agent_reciever.try_iter().take(1000)); // Take atmost 1000 at a time.
            }

            match &ui_state.view {
                View::Select => {
                    if ui.button("Back to editor").clicked() {
                        next_state.set(AppState::Editor);
                    }

                    ui.add_space(10.0);

                    egui::Grid::new("Selection grid")
                        .spacing([25.0, 5.0])
                        .show(ui, |ui| {
                            ui.label("Number of steps: ");
                            ui.add(
                                egui::DragValue::new(&mut ui_state.number_of_steps)
                                    .clamp_range(1..=100000),
                            );
                            ui.end_row();

                            let agent_type = match ui_state.agent {
                                Algorithm::Genetic { .. } => "Genetic",
                            };
                            ui.label("Algorithm: ");
                            egui::ComboBox::from_id_source("Algorithm")
                                .selected_text(agent_type)
                                .show_ui(ui, |ui| {
                                    ui.selectable_value(
                                        &mut ui_state.agent,
                                        Algorithm::Genetic {
                                            number_of_agents: 1000,
                                            repeat_move: 20,
                                            mutation_rate: 0.1,
                                            keep_best: false,
                                        },
                                        "Genetic",
                                    );
                                });
                            ui.end_row();

                            ui.label("Algorithm properties:");
                            ui.end_row();
                            match &mut ui_state.agent {
                                Algorithm::Genetic {
                                    number_of_agents,
                                    repeat_move,
                                    mutation_rate,
                                    keep_best,
                                } => {
                                    ui.label("Number of agents: ");
                                    ui.add(
                                        egui::DragValue::new(number_of_agents)
                                            .clamp_range(10..=1000),
                                    );
                                    ui.end_row();
                                    ui.label("Repeat move: ");
                                    ui.add(egui::DragValue::new(repeat_move).clamp_range(1..=100));
                                    ui.end_row();
                                    ui.label("Mutation rate: ");
                                    ui.add(
                                        egui::DragValue::new(mutation_rate).clamp_range(0.0..=1.0),
                                    );
                                    ui.end_row();
                                    ui.label("Keep best from previous generation: ");
                                    ui.checkbox(keep_best, "");
                                    ui.end_row();
                                }
                            }

                            if ui.button("Train").clicked() {
                                ui_state.view = View::Train;
                                ui_state.agent_reciever = Some(spawn_training_thread(
                                    ui_state.number_of_steps,
                                    &ui_state.agent,
                                    &world,
                                ));
                            }
                            ui.end_row();
                        });
                }
                View::Train => {
                    let UiState {
                        agents,
                        view,
                        agent_reciever,
                        ..
                    } = &mut *ui_state;
                    if ui.button("Back to select").clicked() {
                        *view = View::Select;
                        *agent_reciever = None;
                        agents.clear();
                    }

                    ui.add_space(10.0);

                    for (score, agent) in agents.iter() {
                        ui.horizontal(|ui| {
                            ui.label(format!("Score {}", score));
                            if ui.button("Visualize agent").clicked() {
                                *view = setup_visualization(
                                    &world,
                                    agent,
                                    &mut commands,
                                    &mut meshes,
                                    &mut materials,
                                );
                            }
                        });
                    }
                }
                View::Visualize { environment, .. } => {
                    let mut back_to_train = false;
                    if ui.button("Go back to training").clicked() {
                        back_to_train = true;
                    }
                    ui.add_space(10.0);
                    if let Some(distance) = environment.distance_to_goals() {
                        ui.label(format!("Distance to goals: {:.3}", distance));
                    }
                    if environment.won {
                        ui.add_space(10.0);
                        ui.label("Won");
                    }
                    if back_to_train {
                        cleanup_visulazation(&mut commands, &visualization_objects);
                        ui_state.view = View::Train;
                    }
                }
            }
        });
}

fn update_visualization(
    mut ui_state: ResMut<UiState>,
    mut rigid_bodies: Query<(&mut Transform, &RigidBodyId)>,
    mut camera: Query<&mut Transform, (With<Camera>, Without<RigidBodyId>)>,
) {
    if let View::Visualize { environment, agent } = &mut ui_state.view {
        let player_move = agent.get_move();
        environment.step(player_move);

        for (mut transform, RigidBodyId(rigid_body_handle)) in rigid_bodies.iter_mut() {
            let rigid_body = &environment.rigid_body_set[*rigid_body_handle];
            transform.translation.x = rigid_body.translation().x / BEVY_TO_PHYSICS_SCALE;
            transform.translation.y = rigid_body.translation().y / BEVY_TO_PHYSICS_SCALE;
            transform.rotation = Quat::from_rotation_z(rigid_body.rotation().angle());
        }

        let player_translation =
            environment.rigid_body_set[environment.player_handle.unwrap()].translation();
        let mut camera_transform = camera.iter_mut().next().unwrap();
        camera_transform.translation.x = player_translation.x / BEVY_TO_PHYSICS_SCALE;
        camera_transform.translation.y = player_translation.y / BEVY_TO_PHYSICS_SCALE;
    }
}

fn cleanup_train(
    mut ui_state: ResMut<UiState>,
    mut commands: Commands,
    visualization_objects: Query<Entity, With<VisualizationObject>>,
) {
    *ui_state = UiState::default();
    for entity in visualization_objects.iter() {
        commands.entity(entity).despawn();
    }
}

fn spawn_training_thread(
    number_of_steps: usize,
    algorithm: &Algorithm,
    world: &World,
) -> Receiver<(f32, Agent)> {
    let (sender, reciever) = bounded(100);
    let world = (*world).clone();
    let algorithm = (*algorithm).clone();
    std::thread::spawn(move || {
        let mut rng = thread_rng();

        match algorithm {
            Algorithm::Genetic {
                number_of_agents,
                repeat_move,
                mutation_rate,
                keep_best,
            } => {
                let agent_score = |agent: &Vec<Move>| {
                    let mut environment = PhysicsEnvironment::from_world(&world);
                    let mut score = f32::INFINITY;
                    for player_move in agent.iter() {
                        for _ in 0..repeat_move {
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
                    for _ in 0..number_of_steps % repeat_move {
                        environment.step(Move::default());
                        score = score.min(environment.distance_to_goals().unwrap());

                        if environment.won {
                            break;
                        }
                    }
                    score
                };

                let mut generation = vec![];
                for _ in 0..number_of_agents {
                    let mut agent = vec![];
                    for _ in 0..number_of_steps / repeat_move {
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
                            Agent::GeneticAgent {
                                moves: min_agent.1.clone(),
                                curr: 0,
                                repeat_move,
                            },
                        ))
                        .is_err()
                    {
                        return;
                    }

                    let mut new_generation = if keep_best {
                        vec![min_agent.clone()]
                    } else {
                        vec![]
                    };
                    let additional_agents = number_of_agents - new_generation.len();

                    for _ in 0..additional_agents {
                        let mut parents = generation
                            .choose_multiple_weighted(&mut rng, 2, |(score, _)| {
                                max_score + 1.0 - score
                            })
                            .unwrap();
                        let parent1 = &parents.next().unwrap().1;
                        let parent2 = &parents.next().unwrap().1;

                        let mut agent = vec![];
                        for i in 0..number_of_steps / repeat_move {
                            if rng.gen() {
                                agent.push(parent1[i]);
                            } else {
                                agent.push(parent2[i]);
                            }
                        }
                        for player_move in agent.iter_mut() {
                            if rng.gen::<f32>() < mutation_rate {
                                player_move.left = rng.gen();
                            }
                            if rng.gen::<f32>() < mutation_rate {
                                player_move.right = rng.gen();
                            }
                            if rng.gen::<f32>() < mutation_rate {
                                player_move.up = rng.gen();
                            }
                        }
                        new_generation.push((agent_score(&agent), agent));
                    }
                    generation = new_generation;
                }
            }
        }
    });
    reciever
}

fn setup_visualization(
    world: &Res<World>,
    agent: &Agent,
    commands: &mut Commands,
    meshes: &mut ResMut<Assets<Mesh>>,
    materials: &mut ResMut<Assets<ColorMaterial>>,
) -> View {
    let mut environment = PhysicsEnvironment::new();

    for object_and_transform in world.objects.iter() {
        let object = &object_and_transform.object;
        let transform = object_and_transform.transform();
        let rigid_body_handle = environment.add_object(object_and_transform);
        match object {
            WorldObject::Block { fixed } => {
                let color = if *fixed {
                    Color::BLACK
                } else {
                    Color::DARK_GRAY
                };
                let mut block = commands.spawn(MaterialMesh2dBundle {
                    mesh: meshes
                        .add(Mesh::from(bevy::prelude::shape::Quad::new(Vec2::ONE)))
                        .into(),
                    material: materials.add(ColorMaterial::from(color)),
                    transform,
                    ..default()
                });
                block.insert(VisualizationObject);
                if let Some(rigid_body_handle) = rigid_body_handle {
                    block.insert(RigidBodyId(rigid_body_handle));
                }
            }
            WorldObject::Player => {
                let capsule = bevy::prelude::shape::Capsule {
                    radius: PLAYER_RADIUS,
                    rings: 5,
                    depth: PLAYER_DEPTH,
                    latitudes: 10,
                    longitudes: 10,
                    uv_profile: bevy::prelude::shape::CapsuleUvProfile::Uniform,
                };
                let mut player = commands.spawn(MaterialMesh2dBundle {
                    mesh: meshes.add(capsule.into()).into(),
                    material: materials.add(ColorMaterial::from(Color::GRAY)),
                    transform,
                    ..default()
                });
                player.insert(VisualizationObject);
                player.insert(Player);
                if let Some(rigid_body_handle) = rigid_body_handle {
                    player.insert(RigidBodyId(rigid_body_handle));
                }
            }
            WorldObject::Goal => {
                commands
                    .spawn(MaterialMesh2dBundle {
                        mesh: meshes
                            .add(Mesh::from(bevy::prelude::shape::Quad::new(Vec2::ONE)))
                            .into(),
                        material: materials
                            .add(ColorMaterial::from(Color::rgba(0.0, 1.0, 0.0, 0.5))),
                        transform,
                        ..default()
                    })
                    .insert(VisualizationObject);
            }
        }
    }

    View::Visualize {
        agent: agent.clone(),
        environment: Box::new(environment),
    }
}

fn cleanup_visulazation(
    commands: &mut Commands,
    visualization_objects: &Query<Entity, With<VisualizationObject>>,
) {
    for entity in visualization_objects.iter() {
        commands.entity(entity).despawn();
    }
}

#[derive(Resource)]
struct UiState {
    number_of_steps: usize,
    agent: Algorithm,
    view: View,
    agents: Vec<(f32, Agent)>,
    agent_reciever: Option<Receiver<(f32, Agent)>>,
}

impl Default for UiState {
    fn default() -> Self {
        UiState {
            number_of_steps: 1000,
            agent: Algorithm::default(),
            view: View::default(),
            agents: vec![],
            agent_reciever: None,
        }
    }
}

#[derive(Default)]
enum View {
    #[default]
    Select,
    Train,
    Visualize {
        agent: Agent,
        environment: Box<PhysicsEnvironment>,
    },
}

#[derive(PartialEq, Clone)]
enum Algorithm {
    Genetic {
        number_of_agents: usize,
        repeat_move: usize,
        mutation_rate: f32,
        keep_best: bool,
    },
}

impl Default for Algorithm {
    fn default() -> Self {
        Algorithm::Genetic {
            number_of_agents: 1000,
            repeat_move: 20,
            mutation_rate: 0.1,
            keep_best: false,
        }
    }
}

#[derive(Clone)]
enum Agent {
    GeneticAgent {
        moves: Vec<Move>,
        curr: usize,
        repeat_move: usize,
    },
}

impl Agent {
    fn get_move(&mut self) -> Move {
        match self {
            Agent::GeneticAgent {
                moves,
                curr,
                repeat_move,
            } => {
                if *curr / *repeat_move < moves.len() {
                    let player_move = moves[*curr / *repeat_move];
                    *curr += 1;
                    player_move
                } else {
                    Move::default()
                }
            }
        }
    }
}

#[derive(Component)]
struct VisualizationObject;

#[derive(Component)]
struct RigidBodyId(RigidBodyHandle);

#[derive(Component)]
struct Player;
