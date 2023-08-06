use std::cmp::Ordering;

use bevy::prelude::*;
use rapier2d::prelude::*;
use serde::{Deserialize, Serialize};

pub const PLAYER_DEPTH: f32 = 20.0;
pub const PLAYER_RADIUS: f32 = 20.0;
pub const BEVY_TO_PHYSICS_SCALE: f32 = 0.25 / (2.0 * PLAYER_RADIUS);

#[derive(Debug, Hash, Eq, PartialEq, Clone, Copy, Default, States)]
pub enum AppState {
    #[default]
    Editor,
    Game,
    Train,
}

#[derive(Serialize, Deserialize, Resource, Debug, Clone)]
pub struct World {
    pub objects: Vec<ObjectAndTransform>,
}

impl Default for World {
    fn default() -> Self {
        Self {
            objects: vec![ObjectAndTransform {
                object: WorldObject::Player,
                position: [0.0; 3],
                scale: [1.0; 3],
                rotation: 0.0,
            }],
        }
    }
}

// We don't store the transform as Bevy's Transform as it doesn't implement Serialize.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ObjectAndTransform {
    pub object: WorldObject,
    pub position: [f32; 3],
    pub scale: [f32; 3],
    pub rotation: f32,
}

impl ObjectAndTransform {
    pub fn transform(&self) -> Transform {
        Transform {
            translation: Vec3::from_array(self.position),
            scale: Vec3::from_array(self.scale),
            rotation: Quat::from_rotation_z(self.rotation),
        }
    }
}

// We separate the transform and object as we want separate Bevy components.
#[derive(Serialize, Deserialize, Component, Clone, Debug)]
pub enum WorldObject {
    Player,
    Block { fixed: bool },
    Goal,
}

pub struct PhysicsEnvironment {
    pub integration_parameters: IntegrationParameters,
    pub physics_pipeline: PhysicsPipeline,
    pub island_manager: IslandManager,
    pub broad_phase: BroadPhase,
    pub narrow_phase: NarrowPhase,
    pub impulse_joint_set: ImpulseJointSet,
    pub multibody_joint_set: MultibodyJointSet,
    pub ccd_solver: CCDSolver,
    pub rigid_body_set: RigidBodySet,
    pub collider_set: ColliderSet,
    pub query_pipeline: QueryPipeline,
    pub player_handle: Option<RigidBodyHandle>,
    pub goals: Vec<GoalDimensions>,
    pub won: bool,
}

impl PhysicsEnvironment {
    pub fn new() -> PhysicsEnvironment {
        PhysicsEnvironment {
            integration_parameters: IntegrationParameters::default(),
            physics_pipeline: PhysicsPipeline::new(),
            island_manager: IslandManager::new(),
            broad_phase: BroadPhase::new(),
            narrow_phase: NarrowPhase::new(),
            impulse_joint_set: ImpulseJointSet::new(),
            multibody_joint_set: MultibodyJointSet::new(),
            ccd_solver: CCDSolver::new(),
            rigid_body_set: RigidBodySet::new(),
            collider_set: ColliderSet::new(),
            query_pipeline: QueryPipeline::new(),
            player_handle: None,
            goals: vec![],
            won: false,
        }
    }

    pub fn add_object(
        &mut self,
        object_and_transform: &ObjectAndTransform,
    ) -> Option<RigidBodyHandle> {
        let object = &object_and_transform.object;
        match object {
            WorldObject::Block { fixed } => {
                if *fixed {
                    let collider = ColliderBuilder::cuboid(
                        0.5 * object_and_transform.scale[0].abs() * BEVY_TO_PHYSICS_SCALE,
                        0.5 * object_and_transform.scale[1].abs() * BEVY_TO_PHYSICS_SCALE,
                    )
                    .translation(vector![
                        object_and_transform.position[0] * BEVY_TO_PHYSICS_SCALE,
                        object_and_transform.position[1] * BEVY_TO_PHYSICS_SCALE
                    ])
                    .rotation(object_and_transform.rotation)
                    .build();
                    self.collider_set.insert(collider);
                    None
                } else {
                    let rigid_body = RigidBodyBuilder::dynamic()
                        .translation(vector![
                            object_and_transform.position[0] * BEVY_TO_PHYSICS_SCALE,
                            object_and_transform.position[1] * BEVY_TO_PHYSICS_SCALE
                        ])
                        .rotation(object_and_transform.rotation);
                    let rigid_body_handle = self.rigid_body_set.insert(rigid_body);
                    let collider = ColliderBuilder::cuboid(
                        0.5 * object_and_transform.scale[0].abs() * BEVY_TO_PHYSICS_SCALE,
                        0.5 * object_and_transform.scale[1].abs() * BEVY_TO_PHYSICS_SCALE,
                    )
                    .build();
                    self.collider_set.insert_with_parent(
                        collider,
                        rigid_body_handle,
                        &mut self.rigid_body_set,
                    );
                    Some(rigid_body_handle)
                }
            }
            WorldObject::Player => {
                let rigid_body = RigidBodyBuilder::dynamic()
                    .lock_rotations()
                    .translation(vector![
                        object_and_transform.position[0] * BEVY_TO_PHYSICS_SCALE,
                        object_and_transform.position[1] * BEVY_TO_PHYSICS_SCALE
                    ]);
                let rigid_body_handle = self.rigid_body_set.insert(rigid_body);
                let collider = ColliderBuilder::capsule_y(
                    0.5 * PLAYER_DEPTH * BEVY_TO_PHYSICS_SCALE,
                    PLAYER_RADIUS * BEVY_TO_PHYSICS_SCALE,
                )
                .build();
                self.collider_set.insert_with_parent(
                    collider,
                    rigid_body_handle,
                    &mut self.rigid_body_set,
                );

                self.player_handle = Some(rigid_body_handle);
                Some(rigid_body_handle)
            }
            WorldObject::Goal => {
                self.goals.push(GoalDimensions {
                    x: object_and_transform.position[0] * BEVY_TO_PHYSICS_SCALE,
                    y: object_and_transform.position[1] * BEVY_TO_PHYSICS_SCALE,
                    width: object_and_transform.scale[0].abs() * BEVY_TO_PHYSICS_SCALE,
                    height: object_and_transform.scale[1].abs() * BEVY_TO_PHYSICS_SCALE,
                    rotation: object_and_transform.rotation,
                });
                None
            }
        }
    }

    pub fn from_world(world: &World) -> PhysicsEnvironment {
        let mut environment = PhysicsEnvironment::new();

        for object_and_transform in world.objects.iter() {
            environment.add_object(object_and_transform);
        }

        environment
    }

    pub fn distance_to_goals(&self) -> Option<f32> {
        if let Some(player_handle) = self.player_handle {
            let player_translation = self.rigid_body_set[player_handle].translation();
            let player_translation = Vec2::new(player_translation.x, player_translation.y);

            self.goals
                .iter()
                .map(|goal| {
                    let goal_translation = Vec2::new(goal.x, goal.y);
                    let x_axis = (Quat::from_rotation_z(goal.rotation) * Vec3::X).truncate();
                    let y_axis = (Quat::from_rotation_z(goal.rotation) * Vec3::Y).truncate();

                    let distance_x = ((player_translation - goal_translation).dot(x_axis).abs()
                        - goal.width / 2.0)
                        .max(0.0);
                    let distance_y = ((player_translation - goal_translation).dot(y_axis).abs()
                        - goal.height / 2.0)
                        .max(0.0);
                    (distance_x.powi(2) + distance_y.powi(2)).sqrt() / BEVY_TO_PHYSICS_SCALE
                })
                .reduce(f32::min)
        } else {
            None
        }
    }

    pub fn state(&self) -> Option<[f32; 4]> {
        if let Some(player_handle) = self.player_handle {
            let player_translation = self.rigid_body_set[player_handle].translation();
            let player_velocity = self.rigid_body_set[player_handle].linvel();
            Some([
                player_translation.x,
                player_translation.y,
                player_velocity.x,
                player_velocity.y,
            ])
        } else {
            None
        }
    }

    pub fn step(&mut self, player_move: Move) {
        if let Some(player_handle) = self.player_handle {
            let player_translation = self.rigid_body_set[player_handle].translation();
            let player_lower_center = vector![
                player_translation.x,
                player_translation.y - PLAYER_DEPTH * BEVY_TO_PHYSICS_SCALE / 2.0
            ];

            let mut player_floor_contacts = vec![];
            let player_collider = self.rigid_body_set[player_handle].colliders()[0];
            for contact_pair in self.narrow_phase.contacts_with(player_collider) {
                let contact_collider = if contact_pair.collider1 != player_collider {
                    contact_pair.collider1
                } else {
                    contact_pair.collider2
                };
                let rigid_body = self.collider_set[contact_collider].parent();
                if contact_pair.has_any_active_contact {
                    for manifold in &contact_pair.manifolds {
                        for solver_contact in &manifold.data.solver_contacts {
                            let player_floor_contact = (solver_contact.point - player_lower_center)
                                / (PLAYER_RADIUS * BEVY_TO_PHYSICS_SCALE);
                            if player_floor_contact.y < -0.707 {
                                player_floor_contacts.push((solver_contact.point, rigid_body));
                            }
                        }
                    }
                }
            }

            let on_ground = !player_floor_contacts.is_empty();

            if on_ground {
                let mut player_impulse = vector![0.0, 0.0];

                if player_move.left {
                    let (point, rigid_body) = player_floor_contacts
                        .iter()
                        .min_by(|(point1, _), (point2, _)| {
                            if point1.x < point2.x {
                                Ordering::Less
                            } else if point1.x > point2.x {
                                Ordering::Greater
                            } else {
                                Ordering::Equal
                            }
                        })
                        .unwrap();

                    let mut normal = *point - player_lower_center;
                    normal /= (normal.x.powi(2) + normal.y.powi(2)).sqrt();
                    let impulse = vector![0.003 * normal.y, -0.003 * normal.x]; // Rotate normal

                    if let Some(rigid_body) = rigid_body {
                        self.rigid_body_set[*rigid_body]
                            .apply_impulse_at_point(-impulse, *point, true);
                    }
                    player_impulse += impulse;
                }

                if player_move.right {
                    let (point, rigid_body) = player_floor_contacts
                        .iter()
                        .max_by(|(point1, _), (point2, _)| {
                            if point1.x < point2.x {
                                Ordering::Less
                            } else if point1.x > point2.x {
                                Ordering::Greater
                            } else {
                                Ordering::Equal
                            }
                        })
                        .unwrap();

                    let mut normal = *point - player_lower_center;
                    normal /= (normal.x.powi(2) + normal.y.powi(2)).sqrt();
                    let impulse = vector![-0.003 * normal.y, 0.003 * normal.x]; // Rotate normal

                    if let Some(rigid_body) = rigid_body {
                        self.rigid_body_set[*rigid_body]
                            .apply_impulse_at_point(-impulse, *point, true);
                    }
                    player_impulse += impulse;
                }

                if player_move.up {
                    for (point, rigid_body) in &player_floor_contacts {
                        let mut normal = *point - player_lower_center;
                        normal /= (normal.x.powi(2) + normal.y.powi(2)).sqrt();
                        let impulse = vector![-0.1 * normal.x, -0.1 * normal.y]
                            / player_floor_contacts.len() as f32;

                        if let Some(rigid_body) = rigid_body {
                            self.rigid_body_set[*rigid_body]
                                .apply_impulse_at_point(-impulse, *point, true);
                        }
                        player_impulse += impulse;
                    }
                }

                self.rigid_body_set[player_handle].apply_impulse(player_impulse, true);
            }
        }

        self.physics_pipeline.step(
            &vector![0.0, -2.0],
            &self.integration_parameters,
            &mut self.island_manager,
            &mut self.broad_phase,
            &mut self.narrow_phase,
            &mut self.rigid_body_set,
            &mut self.collider_set,
            &mut self.impulse_joint_set,
            &mut self.multibody_joint_set,
            &mut self.ccd_solver,
            None,
            &(),
            &(),
        );
        self.query_pipeline
            .update(&self.rigid_body_set, &self.collider_set);

        if !self.won {
            if let Some(distance) = self.distance_to_goals() {
                if distance < 1e-7 {
                    self.won = true;
                }
            }
        }
    }
}

pub struct GoalDimensions {
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    rotation: f32,
}

#[derive(Default, Clone, Copy, Debug)]
pub struct Move {
    pub left: bool,
    pub right: bool,
    pub up: bool,
}
