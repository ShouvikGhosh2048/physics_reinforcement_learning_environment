use bevy::prelude::*;
use serde::{Deserialize, Serialize};

pub const PLAYER_DEPTH: f32 = 20.0;
pub const PLAYER_RADIUS: f32 = 20.0;

#[derive(Debug, Hash, Eq, PartialEq, Clone, Copy, Default, States)]
pub enum AppState {
    #[default]
    Editor,
    Game,
}

#[derive(Serialize, Deserialize, Resource, Debug)]
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
#[derive(Serialize, Deserialize, Debug)]
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
