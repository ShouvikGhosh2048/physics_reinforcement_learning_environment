use crate::common::{
    AppState, Move, PhysicsEnvironment, World, WorldObject, BEVY_TO_PHYSICS_SCALE, PLAYER_DEPTH,
    PLAYER_RADIUS,
};

use bevy::{prelude::*, sprite::MaterialMesh2dBundle};
use bevy_egui::{egui, EguiContexts};
use rapier2d::prelude::RigidBodyHandle;

pub fn add_game_systems(app: &mut App) {
    app.add_system(setup_game.in_schedule(OnEnter(AppState::Game)))
        .add_systems((game_ui_system, update_game).in_set(OnUpdate(AppState::Game)))
        .add_system(cleanup_game.in_schedule(OnExit(AppState::Game)));
}

fn setup_game(
    world: Res<World>,
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
) {
    let mut physics_environment = PhysicsEnvironment::new(world.player_position);

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
    player.insert(GameObject);
    player.insert(RigidBodyId(physics_environment.player_handle));

    for object_and_transform in world.objects.iter() {
        let object = &object_and_transform.object;
        let transform = object_and_transform.transform();
        let rigid_body_handle = physics_environment.add_object(object_and_transform);
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
                block.insert(GameObject);
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
                    .insert(GameObject);
            }
        }
    }

    commands.insert_resource(GameState {
        physics_environment,
        steps: 0,
    });
}

fn game_ui_system(
    mut next_state: ResMut<NextState<AppState>>,
    game_state: Res<GameState>,
    mut contexts: EguiContexts,
) {
    egui::Window::new("Game").show(contexts.ctx_mut(), |ui| {
        ui.horizontal(|ui| {
            if ui.button("Back to editor").clicked() {
                next_state.set(AppState::Editor);
            }
            ui.add_space(15.0);
            if ui.button("Reset").clicked() {
                next_state.set(AppState::Game);
            }
        });
        ui.add_space(5.0);
        ui.label(format!("Steps: {}", game_state.steps));
        if game_state.physics_environment.won {
            ui.add_space(5.0);
            ui.label("Won!");
        }
    });
}

fn update_game(
    input: Res<Input<KeyCode>>,
    mut game_state: ResMut<GameState>,
    mut rigid_bodies: Query<(&mut Transform, &RigidBodyId)>,
    mut camera: Query<&mut Transform, (With<Camera>, Without<RigidBodyId>)>,
) {
    let GameState {
        physics_environment,
        steps,
    } = &mut *game_state;

    let player_move = Move {
        left: input.pressed(KeyCode::A),
        right: input.pressed(KeyCode::D),
        up: input.pressed(KeyCode::W),
    };
    physics_environment.step(player_move);
    *steps += 1;

    for (mut transform, RigidBodyId(rigid_body_handle)) in rigid_bodies.iter_mut() {
        let rigid_body = &physics_environment.rigid_body_set[*rigid_body_handle];
        transform.translation.x = rigid_body.translation().x / BEVY_TO_PHYSICS_SCALE;
        transform.translation.y = rigid_body.translation().y / BEVY_TO_PHYSICS_SCALE;
        transform.rotation = Quat::from_rotation_z(rigid_body.rotation().angle());
    }

    let player_translation =
        physics_environment.rigid_body_set[physics_environment.player_handle].translation();
    let mut camera_transform = camera.iter_mut().next().unwrap();
    camera_transform.translation.x = player_translation.x / BEVY_TO_PHYSICS_SCALE;
    camera_transform.translation.y = player_translation.y / BEVY_TO_PHYSICS_SCALE;
}

fn cleanup_game(mut commands: Commands, game_objects: Query<Entity, With<GameObject>>) {
    for entity in game_objects.iter() {
        commands.entity(entity).despawn();
    }
}

#[derive(Resource)]
struct GameState {
    physics_environment: PhysicsEnvironment,
    steps: usize,
}

#[derive(Component)]
struct GameObject;

#[derive(Component)]
struct RigidBodyId(RigidBodyHandle);
