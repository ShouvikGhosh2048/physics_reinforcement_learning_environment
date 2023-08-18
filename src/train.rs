use crate::{
    algorithm::{Agent, Algorithm, TrainingDetails},
    common::{
        AppState, Environment, World, WorldObject, BEVY_TO_PHYSICS_SCALE, PLAYER_DEPTH,
        PLAYER_RADIUS,
    },
};

use bevy::{prelude::*, sprite::MaterialMesh2dBundle};
use bevy_egui::{egui, EguiContexts};
use crossbeam::channel::bounded;
use rapier2d::prelude::*;

pub fn add_train_systems<
    AgentType: Agent,
    Message: Send + Sync + 'static,
    TrainingDetailsType: TrainingDetails<AgentType, Message>,
    AlgorithmType: Algorithm<AgentType, Message, TrainingDetailsType>,
>(
    app: &mut App,
) {
    let ui_state: UiState<AgentType, TrainingDetailsType, AlgorithmType> = UiState::default();
    app.add_systems(
        (
            ui_system::<AgentType, Message, TrainingDetailsType, AlgorithmType>,
            update_visualization::<AgentType, Message, TrainingDetailsType, AlgorithmType>,
        )
            .in_set(OnUpdate(AppState::Train)),
    )
    .add_system(
        cleanup_train::<AgentType, Message, TrainingDetailsType, AlgorithmType>
            .in_schedule(OnExit(AppState::Train)),
    )
    .insert_resource(ui_state);
}

fn ui_system<
    AgentType: Agent,
    Message: Send + Sync + 'static,
    TrainingDetailsType: TrainingDetails<AgentType, Message>,
    AlgorithmType: Algorithm<AgentType, Message, TrainingDetailsType>,
>(
    mut contexts: EguiContexts,
    mut ui_state: ResMut<UiState<AgentType, TrainingDetailsType, AlgorithmType>>,
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
            if let Some(agent_receiver) = &mut ui_state.agent_receiver {
                agent_receiver.receive_messages();
            }

            match &ui_state.view {
                View::Select => {
                    if ui.button("Back to editor").clicked() {
                        next_state.set(AppState::Editor);
                    }

                    ui.add_space(10.0);

                    ui_state.agent.selection_ui(ui);

                    if ui.button("Train").clicked() {
                        ui_state.view = View::Train;
                        let (sender, receiver) = bounded(1000);
                        let world = world.clone();
                        let algorithm = ui_state.agent.clone();
                        std::thread::spawn(move || algorithm.train(world, sender));
                        ui_state.agent_receiver =
                            Some(ui_state.agent.training_details_receiver(receiver));
                    }
                }
                View::Train => {
                    let UiState {
                        view,
                        agent_receiver,
                        ..
                    } = &mut *ui_state;
                    if ui.button("Back to select").clicked() {
                        *view = View::Select;
                        *agent_receiver = None;
                    }

                    ui.add_space(10.0);

                    if let Some(receiver) = agent_receiver {
                        if let Some(agent) = receiver.details_ui(ui) {
                            *view = setup_visualization(
                                &world,
                                agent,
                                &mut commands,
                                &mut meshes,
                                &mut materials,
                            );
                        }
                    }
                }
                View::Visualize { agent, environment } => {
                    let mut back_to_train = false;
                    if ui.button("Go back to training").clicked() {
                        back_to_train = true;
                    }
                    ui.add_space(10.0);
                    if let Some(distance) = environment.distance_to_goals() {
                        ui.label(format!("Distance to goals: {:.3}", distance));
                    }
                    if environment.won() {
                        ui.add_space(10.0);
                        ui.label("Won");
                    }
                    ui.add_space(10.0);
                    agent.details_ui(ui, environment);
                    if back_to_train {
                        cleanup_visulazation(&mut commands, &visualization_objects);
                        ui_state.view = View::Train;
                    }
                }
            }
        });
}

fn update_visualization<
    AgentType: Agent,
    Message: Send + Sync + 'static,
    TrainingDetailsType: TrainingDetails<AgentType, Message>,
    AlgorithmType: Algorithm<AgentType, Message, TrainingDetailsType>,
>(
    mut ui_state: ResMut<UiState<AgentType, TrainingDetailsType, AlgorithmType>>,
    mut rigid_bodies: Query<(&mut Transform, &RigidBodyId)>,
    mut camera: Query<&mut Transform, (With<Camera>, Without<RigidBodyId>)>,
) {
    if let View::Visualize { environment, agent } = &mut ui_state.view {
        let player_move = agent.get_move(environment);
        environment.step(player_move);

        for (mut transform, RigidBodyId(rigid_body_handle)) in rigid_bodies.iter_mut() {
            let rigid_body = &environment.rigid_body_set()[*rigid_body_handle];
            transform.translation.x = rigid_body.translation().x / BEVY_TO_PHYSICS_SCALE;
            transform.translation.y = rigid_body.translation().y / BEVY_TO_PHYSICS_SCALE;
            transform.rotation = Quat::from_rotation_z(rigid_body.rotation().angle());
        }

        let player_translation =
            environment.rigid_body_set()[environment.player_handle()].translation();
        let mut camera_transform = camera.iter_mut().next().unwrap();
        camera_transform.translation.x = player_translation.x / BEVY_TO_PHYSICS_SCALE;
        camera_transform.translation.y = player_translation.y / BEVY_TO_PHYSICS_SCALE;
    }
}

fn cleanup_train<
    AgentType: Agent,
    Message: Send + Sync + 'static,
    TrainingDetailsType: TrainingDetails<AgentType, Message>,
    AlgorithmType: Algorithm<AgentType, Message, TrainingDetailsType>,
>(
    mut ui_state: ResMut<UiState<AgentType, TrainingDetailsType, AlgorithmType>>,
    mut commands: Commands,
    visualization_objects: Query<Entity, With<VisualizationObject>>,
) {
    *ui_state = UiState::default();
    for entity in visualization_objects.iter() {
        commands.entity(entity).despawn();
    }
}

fn setup_visualization<AgentType: Agent>(
    world: &Res<World>,
    agent: &AgentType,
    commands: &mut Commands,
    meshes: &mut ResMut<Assets<Mesh>>,
    materials: &mut ResMut<Assets<ColorMaterial>>,
) -> View<AgentType> {
    let mut environment = Environment::new(world.player_position);

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
        transform: Transform::from_translation(Vec3::new(
            world.player_position[0],
            world.player_position[1],
            0.0,
        )),
        ..default()
    });
    player.insert(VisualizationObject);
    player.insert(Player);
    player.insert(RigidBodyId(environment.player_handle()));

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
struct UiState<Agent, TrainingDetails, Algorithm> {
    agent: Algorithm,
    view: View<Agent>,
    agent_receiver: Option<TrainingDetails>,
}

impl<Agent, TrainingDetails, Algorithm: Default> Default
    for UiState<Agent, TrainingDetails, Algorithm>
{
    fn default() -> Self {
        UiState {
            agent: Algorithm::default(),
            view: View::default(),
            agent_receiver: None,
        }
    }
}

#[derive(Default)]
enum View<Agent> {
    #[default]
    Select,
    Train,
    Visualize {
        agent: Agent,
        environment: Box<Environment>,
    },
}

#[derive(Component)]
struct VisualizationObject;

#[derive(Component)]
struct RigidBodyId(RigidBodyHandle);

#[derive(Component)]
struct Player;
