use std::f32::consts::PI;

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
}

impl ObjectAndTransform {
    pub fn transform(&self) -> Transform {
        Transform {
            translation: Vec3::from_array(self.position),
            scale: Vec3::from_array(self.scale),
            ..Default::default()
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
        let transform = object_and_transform.transform();
        match object {
            WorldObject::Block { fixed } => {
                if *fixed {
                    let collider = ColliderBuilder::cuboid(
                        0.5 * transform.scale.x.abs() * BEVY_TO_PHYSICS_SCALE,
                        0.5 * transform.scale.y.abs() * BEVY_TO_PHYSICS_SCALE,
                    )
                    .translation(vector![
                        transform.translation.x * BEVY_TO_PHYSICS_SCALE,
                        transform.translation.y * BEVY_TO_PHYSICS_SCALE
                    ])
                    .build();
                    self.collider_set.insert(collider);
                    None
                } else {
                    let rigid_body = RigidBodyBuilder::dynamic().translation(vector![
                        transform.translation.x * BEVY_TO_PHYSICS_SCALE,
                        transform.translation.y * BEVY_TO_PHYSICS_SCALE
                    ]);
                    let rigid_body_handle = self.rigid_body_set.insert(rigid_body);
                    let collider = ColliderBuilder::cuboid(
                        0.5 * transform.scale.x.abs() * BEVY_TO_PHYSICS_SCALE,
                        0.5 * transform.scale.y.abs() * BEVY_TO_PHYSICS_SCALE,
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
                        transform.translation.x * BEVY_TO_PHYSICS_SCALE,
                        transform.translation.y * BEVY_TO_PHYSICS_SCALE
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
                    x: transform.translation.x * BEVY_TO_PHYSICS_SCALE,
                    y: transform.translation.y * BEVY_TO_PHYSICS_SCALE,
                    width: transform.scale.x.abs() * BEVY_TO_PHYSICS_SCALE,
                    height: transform.scale.y.abs() * BEVY_TO_PHYSICS_SCALE,
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

            self.goals
                .iter()
                .map(|goal| {
                    let distance_x =
                        ((player_translation.x - goal.x).abs() - goal.width / 2.0).max(0.0);
                    let distance_y =
                        ((player_translation.y - goal.y).abs() - goal.height / 2.0).max(0.0);
                    (distance_x.powi(2) + distance_y.powi(2)).sqrt() / BEVY_TO_PHYSICS_SCALE
                })
                .reduce(f32::min)
        } else {
            None
        }
    }

    pub fn step(&mut self, player_move: Move) {
        if let Some(player_handle) = self.player_handle {
            let player_translation = self.rigid_body_set[player_handle].translation();

            let mut on_ground = false;
            let number_of_points = 21;
            for i in 0..number_of_points {
                let fraction = i as f32 / (number_of_points - 1) as f32;
                let arc_angle = (1.0 - fraction) * PI / 4.0 + fraction * 3.0 * PI / 4.0;
                let point_position = point![
                    player_translation.x - PLAYER_RADIUS * BEVY_TO_PHYSICS_SCALE * arc_angle.cos(),
                    player_translation.y
                        - PLAYER_DEPTH * BEVY_TO_PHYSICS_SCALE / 2.0
                        - PLAYER_RADIUS * BEVY_TO_PHYSICS_SCALE * arc_angle.sin()
                ];
                let ray_dir = vector![0.0, -1.0];
                let ray = rapier2d::prelude::Ray::new(point_position, ray_dir);
                let max_toi = 0.5 * BEVY_TO_PHYSICS_SCALE;
                let solid = true;
                let filter = QueryFilter::new().exclude_rigid_body(player_handle);
                if self
                    .query_pipeline
                    .cast_ray(
                        &self.rigid_body_set,
                        &self.collider_set,
                        &ray,
                        max_toi,
                        solid,
                        filter,
                    )
                    .is_some()
                {
                    on_ground = true;
                };
            }

            if on_ground {
                let player = &mut self.rigid_body_set[player_handle];

                let mut impulse = vector![0.0, 0.0];
                if player_move.left {
                    impulse.x -= 0.003;
                }
                if player_move.right {
                    impulse.x += 0.003;
                }
                if player_move.up {
                    impulse.y += 0.07;
                }
                player.apply_impulse(impulse, true);
            }
        }

        self.physics_pipeline.step(
            &vector![0.0, -1.0],
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

        if let Some(player_handle) = self.player_handle {
            if !self.won {
                let player_translation = self.rigid_body_set[player_handle].translation();
                for goal in &self.goals {
                    if (player_translation.x - goal.x).abs() < goal.width / 2.0
                        && (player_translation.y - goal.y).abs() < goal.height / 2.0
                    {
                        self.won = true;
                        break;
                    }
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
}

#[derive(Default, Clone, Copy)]
pub struct Move {
    pub left: bool,
    pub right: bool,
    pub up: bool,
}
