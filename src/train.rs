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
    egui::Window::new("Train agents").show(contexts.ctx_mut(), |ui| {
        let UiState {
            agent_reciever,
            agents,
            ..
        } = &mut *ui_state;
        if let Some(agent_reciever) = agent_reciever {
            agents.extend(agent_reciever.try_iter().take(1000)); // Take atmost 1000 at a time.
        }

        if ui.button("Back to editor").clicked() {
            next_state.set(AppState::Editor);
        }

        match &ui_state.view {
            View::Select => {
                ui.horizontal(|ui| {
                    ui.label("Number of steps: ");
                    ui.add(
                        egui::DragValue::new(&mut ui_state.number_of_steps).clamp_range(1..=100000),
                    );
                });

                let agent_type = match ui_state.agent {
                    Algorithm::Genetic { .. } => "Genetic",
                };
                ui.horizontal(|ui| {
                    ui.label("Algorithm: ");
                    egui::ComboBox::from_id_source("Algorithm")
                        .selected_text(agent_type)
                        .show_ui(ui, |ui| {
                            ui.selectable_value(
                                &mut ui_state.agent,
                                Algorithm::Genetic {
                                    number_of_agents: 10,
                                },
                                "Genetic",
                            );
                        });
                });

                ui.label("Algorithm properties");
                match &mut ui_state.agent {
                    Algorithm::Genetic { number_of_agents } => {
                        ui.horizontal(|ui| {
                            ui.label("Number of agents: ");
                            ui.add(egui::DragValue::new(number_of_agents).clamp_range(10..=1000));
                        });
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

                egui::ScrollArea::vertical().show(ui, |ui| {
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
                });
            }
            View::Visualize { environment, .. } => {
                let mut back_to_train = false;
                if ui.button("Go back to training").clicked() {
                    back_to_train = true;
                }
                ui.label(format!("Won: {}", environment.won));
                if let Some(distance) = environment.distance_to_goals() {
                    ui.label(format!("Distance to goals: {}", distance));
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
            Algorithm::Genetic { number_of_agents } => {
                let mut generation = vec![];
                for _ in 0..number_of_agents {
                    let mut agent = vec![];
                    for _ in 0..number_of_steps {
                        agent.push(Move {
                            left: rng.gen(),
                            right: rng.gen(),
                            up: rng.gen(),
                        });
                    }

                    let mut environment = PhysicsEnvironment::from_world(&world);
                    let mut score = f32::INFINITY;
                    for player_move in agent.iter() {
                        environment.step(*player_move);
                        score = score.min(environment.distance_to_goals().unwrap());

                        if environment.won {
                            break;
                        }
                    }
                    generation.push((score, agent));
                }

                loop {
                    generation.sort_by(|(score1, _), (score2, _)| {
                        if score1 < score2 {
                            Ordering::Less
                        } else if score1 > score2 {
                            Ordering::Greater
                        } else {
                            Ordering::Equal
                        }
                    });
                    if sender
                        .send((
                            generation[0].0,
                            Agent::GeneticAgent {
                                moves: generation[0].1.clone(),
                                curr: 0,
                            },
                        ))
                        .is_err()
                    {
                        return;
                    }

                    generation.truncate(number_of_agents / 10);
                    for i in 0..number_of_agents / 10 {
                        for _ in 0..6 {
                            let mut agent = generation[i].1.clone();
                            for player_move in agent.iter_mut() {
                                if rng.gen::<f64>() < 0.1 {
                                    player_move.left = rng.gen();
                                }
                                if rng.gen::<f64>() < 0.1 {
                                    player_move.right = rng.gen();
                                }
                                if rng.gen::<f64>() < 0.1 {
                                    player_move.up = rng.gen();
                                }
                            }

                            let mut environment = PhysicsEnvironment::from_world(&world);
                            let mut score = f32::INFINITY;
                            for player_move in agent.iter() {
                                environment.step(*player_move);
                                score = score.min(environment.distance_to_goals().unwrap());

                                if environment.won {
                                    break;
                                }
                            }
                            generation.push((score, agent));
                        }
                    }

                    while generation.len() < number_of_agents {
                        let mut agent = vec![];
                        for _ in 0..number_of_steps {
                            agent.push(Move {
                                left: rng.gen(),
                                right: rng.gen(),
                                up: rng.gen(),
                            });
                        }

                        let mut environment = PhysicsEnvironment::from_world(&world);
                        let mut score = f32::INFINITY;
                        for player_move in agent.iter() {
                            environment.step(*player_move);
                            score = score.min(environment.distance_to_goals().unwrap());

                            if environment.won {
                                break;
                            }
                        }
                        generation.push((score, agent));
                    }
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
            WorldObject::Block { .. } => {
                let mut block = commands.spawn(MaterialMesh2dBundle {
                    mesh: meshes
                        .add(Mesh::from(bevy::prelude::shape::Quad::new(Vec2::ONE)))
                        .into(),
                    material: materials.add(ColorMaterial::from(Color::BLACK)),
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
        environment,
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
        environment: PhysicsEnvironment,
    },
}

#[derive(PartialEq, Clone)]
enum Algorithm {
    Genetic { number_of_agents: usize },
}

impl Default for Algorithm {
    fn default() -> Self {
        Algorithm::Genetic {
            number_of_agents: 1000,
        }
    }
}

#[derive(Clone)]
enum Agent {
    GeneticAgent { moves: Vec<Move>, curr: usize },
}

impl Agent {
    fn get_move(&mut self) -> Move {
        match self {
            Agent::GeneticAgent { moves, curr } => {
                if *curr < moves.len() {
                    *curr += 1;
                    moves[*curr - 1]
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
