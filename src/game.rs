use crate::common::{AppState, World, WorldObject, PLAYER_DEPTH, PLAYER_RADIUS};

use std::f32::consts::PI;

use bevy::{prelude::*, sprite::MaterialMesh2dBundle};
use bevy_egui::{egui, EguiContexts};
use bevy_rapier2d::prelude::*;

pub fn add_game_systems(app: &mut App) {
    app.add_system(setup_game.in_schedule(OnEnter(AppState::Game)))
        .add_systems(
            (game_ui_system, movement, camera_on_player, player_won)
                .in_set(OnUpdate(AppState::Game)),
        )
        .add_system(cleanup_game.in_schedule(OnExit(AppState::Game)));
}

#[derive(Component)]
struct Player;

#[derive(Component)]
struct Goal;

#[derive(Component)]
struct GameObject;

#[derive(Resource)]
struct Won(bool);

fn setup_game(
    world: Res<World>,
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
) {
    commands.insert_resource(Won(false));

    for object_and_transform in world.objects.iter() {
        let object = &object_and_transform.object;
        let transform = object_and_transform.transform();
        match object {
            WorldObject::Block { fixed } => {
                let mut block = commands.spawn(MaterialMesh2dBundle {
                    mesh: meshes.add(Mesh::from(shape::Quad::new(Vec2::ONE))).into(),
                    material: materials.add(ColorMaterial::from(Color::BLACK)),
                    transform,
                    ..default()
                });
                block.insert(GameObject);
                block.insert(Collider::cuboid(1.0 / 2.0, 1.0 / 2.0));
                if !fixed {
                    block.insert(RigidBody::Dynamic);
                }
            }
            WorldObject::Goal => {
                commands
                    .spawn(MaterialMesh2dBundle {
                        mesh: meshes.add(Mesh::from(shape::Quad::new(Vec2::ONE))).into(),
                        material: materials
                            .add(ColorMaterial::from(Color::rgba(0.0, 1.0, 0.0, 0.5))),
                        transform,
                        ..default()
                    })
                    .insert(Goal)
                    .insert(GameObject);
            }
            WorldObject::Player => {
                let capsule = shape::Capsule {
                    radius: PLAYER_RADIUS,
                    rings: 5,
                    depth: PLAYER_DEPTH,
                    latitudes: 10,
                    longitudes: 10,
                    uv_profile: shape::CapsuleUvProfile::Uniform,
                };
                commands
                    .spawn(RigidBody::Dynamic)
                    .insert(Collider::capsule_y(PLAYER_DEPTH / 2.0, PLAYER_RADIUS))
                    .insert(MaterialMesh2dBundle {
                        mesh: meshes.add(capsule.into()).into(),
                        material: materials.add(ColorMaterial::from(Color::GRAY)),
                        transform,
                        ..default()
                    })
                    .insert(ExternalImpulse {
                        impulse: Vec2::new(0.0, 0.0),
                        torque_impulse: 0.0,
                    })
                    .insert(LockedAxes::ROTATION_LOCKED)
                    .insert(Player)
                    .insert(GameObject);
            }
        }
    }
}

fn game_ui_system(
    mut next_state: ResMut<NextState<AppState>>,
    mut contexts: EguiContexts,
    won: Res<Won>,
) {
    egui::Window::new("Game").show(contexts.ctx_mut(), |ui| {
        if ui.button("Back to editor").clicked() {
            next_state.set(AppState::Editor);
        }
        if ui.button("Reset").clicked() {
            next_state.set(AppState::Game);
        }
        if won.0 {
            ui.label("Won!");
        }
    });
}

fn movement(
    input: Res<Input<KeyCode>>,
    mut player: Query<(Entity, &Transform, &mut ExternalImpulse), With<Player>>,
    rapier_context: Res<RapierContext>,
) {
    let (entity, transform, mut external_impulse) = player.iter_mut().next().unwrap(); // Exactly one player should exist.

    // We take points on the bottom semicircle
    // (equidistant to each other and the endpoints of the semicircle's diameter)
    // and cast rays downwards to check for ground.
    let mut on_ground = false;
    let number_of_points = 11;
    for i in 1..=number_of_points {
        let arc_angle = (i as f32 / (number_of_points + 1) as f32) * PI;
        let point_position = Vec2::new(
            transform.translation.x,
            transform.translation.y - PLAYER_DEPTH / 2.0,
        ) - PLAYER_RADIUS * Vec2::new(arc_angle.cos(), arc_angle.sin());
        let ray_dir = Vec2::NEG_Y;
        let max_toi = 2.0;
        let solid = true;
        let filter = QueryFilter::new().exclude_rigid_body(entity);
        if rapier_context
            .cast_ray(point_position, ray_dir, max_toi, solid, filter)
            .is_some()
        {
            on_ground = true;
            break;
        }
    }

    if !on_ground {
        return;
    }

    let mut impulse = Vec2::ZERO;

    if input.pressed(KeyCode::A) {
        impulse.x -= 0.5;
    }
    if input.pressed(KeyCode::D) {
        impulse.x += 0.5;
    }
    if input.pressed(KeyCode::W) {
        impulse.y += 5.0;
    }

    external_impulse.impulse = impulse;
}

fn camera_on_player(
    mut camera: Query<&mut Transform, With<Camera>>,
    player: Query<&Transform, (With<Player>, Without<Camera>)>,
) {
    let player_transform = player.iter().next().unwrap(); // Exactly one player should exist.
    let mut camera_transform = camera.iter_mut().next().unwrap(); // Exactly one camera should exist.
    camera_transform.translation.x = player_transform.translation.x;
    camera_transform.translation.y = player_transform.translation.y;
}

fn player_won(
    player: Query<&Transform, With<Player>>,
    goals: Query<&Transform, (With<Goal>, Without<Player>)>,
    mut won: ResMut<Won>,
) {
    let player_transform = player.iter().next().unwrap(); // Exactly one player should exist.
    for goal in goals.iter() {
        let player_offset = player_transform.translation.truncate() - goal.translation.truncate();
        if player_offset.x.abs() < goal.scale.x.abs() / 2.0
            && player_offset.y.abs() < goal.scale.y.abs() / 2.0
        {
            *won = Won(true);
        }
    }
}

fn cleanup_game(mut commands: Commands, game_objects: Query<Entity, With<GameObject>>) {
    for entity in game_objects.iter() {
        commands.entity(entity).despawn();
    }
}
