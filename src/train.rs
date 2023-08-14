mod agent;

use self::agent::{genetic::GeneticAlgorithm, spawn_training_thread, Agent, Algorithm};
use crate::common::{
    AppState, PhysicsEnvironment, World, WorldObject, BEVY_TO_PHYSICS_SCALE, PLAYER_DEPTH,
    PLAYER_RADIUS,
};

use bevy::{prelude::*, sprite::MaterialMesh2dBundle};
use bevy_egui::{egui, EguiContexts};
use crossbeam::channel::Receiver;
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
                                Algorithm::Genetic(_) => "Genetic",
                            };
                            ui.label("Algorithm: ");
                            egui::ComboBox::from_id_source("Algorithm")
                                .selected_text(agent_type)
                                .show_ui(ui, |ui| {
                                    ui.selectable_value(
                                        &mut ui_state.agent,
                                        Algorithm::Genetic(GeneticAlgorithm::default()),
                                        "Genetic",
                                    );
                                });
                            ui.end_row();

                            ui.label("Algorithm properties:");
                            ui.end_row();

                            ui_state.agent.algorithm_properties_ui(ui);

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

#[derive(Component)]
struct VisualizationObject;

#[derive(Component)]
struct RigidBodyId(RigidBodyHandle);

#[derive(Component)]
struct Player;
