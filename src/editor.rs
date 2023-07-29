use crate::common::{
    AppState, ObjectAndTransform, World, WorldObject, PLAYER_DEPTH, PLAYER_RADIUS,
};

use bevy::{input::mouse::MouseWheel, prelude::*, sprite::MaterialMesh2dBundle};
use bevy_egui::{
    egui::{self, DragValue},
    EguiContexts,
};
use std::{f32::consts::PI, fs};

const ANCHOR_RADIUS: f32 = 5.0;
const RING_OUTER_RADIUS: f32 = 100.0;
const RING_INNER_RADIUS: f32 = 90.0;

pub fn add_editor_systems(app: &mut App) {
    app.init_resource::<EditorUiState>()
        .add_system(setup_editor.in_schedule(OnEnter(AppState::Editor)))
        .add_system(editor_ui_system.in_set(OnUpdate(AppState::Editor)))
        .add_system(cleanup_editor.in_schedule(OnExit(AppState::Editor)));
}

#[derive(Component)]
enum TransformEditor {
    Anchor,
    Ring,
}

fn create_anchor(
    position: Vec3,
    camera_scale: f32,
    commands: &mut Commands,
    meshes: &mut ResMut<Assets<Mesh>>,
    materials: &mut ResMut<Assets<ColorMaterial>>,
) -> Entity {
    commands
        .spawn(MaterialMesh2dBundle {
            mesh: meshes.add(shape::Circle::new(ANCHOR_RADIUS).into()).into(),
            material: materials.add(ColorMaterial::from(Color::RED)),
            transform: Transform::from_translation(position).with_scale(Vec3::new(
                camera_scale,
                camera_scale,
                1.0,
            )),
            ..default()
        })
        .insert(TransformEditor::Anchor)
        .id()
}

fn create_ring(
    position: (f32, f32, f32),
    camera_scale: f32,
    commands: &mut Commands,
    meshes: &mut ResMut<Assets<Mesh>>,
    materials: &mut ResMut<Assets<ColorMaterial>>,
) -> Entity {
    commands
        .spawn(MaterialMesh2dBundle {
            mesh: meshes
                .add(
                    shape::Torus {
                        radius: (RING_OUTER_RADIUS + RING_INNER_RADIUS) / 2.0,
                        ring_radius: (RING_OUTER_RADIUS - RING_INNER_RADIUS) / 2.0,
                        subdivisions_segments: 50,
                        subdivisions_sides: 50,
                    }
                    .into(),
                )
                .into(),
            material: materials.add(ColorMaterial::from(Color::TEAL)),
            transform: Transform::from_xyz(position.0, position.1, position.2)
                .with_scale(Vec3::new(camera_scale, 1.0, camera_scale))
                .with_rotation(Quat::from_rotation_x(PI / 2.0)),
            ..default()
        })
        .insert(TransformEditor::Ring)
        .id()
}

impl WorldObject {
    fn can_drag(&self, transform: &Transform, pointer_position: Vec2) -> bool {
        match self {
            WorldObject::Player => {
                let translation = transform.translation.truncate();
                let center_offset = Vec2::new(0.0, PLAYER_DEPTH / 2.0);
                ((pointer_position - translation).x.abs() < PLAYER_RADIUS
                    && (pointer_position - translation).y.abs() < PLAYER_DEPTH / 2.0)
                    || (pointer_position - translation - center_offset).length() < PLAYER_RADIUS
                    || (pointer_position - translation + center_offset).length() < PLAYER_RADIUS
            }
            WorldObject::Block { .. } | WorldObject::Goal => {
                let translation = transform.translation.truncate();
                let size = transform.scale.truncate();
                let x_axis = (transform.rotation * Vec3::X).truncate();
                let y_axis = (transform.rotation * Vec3::Y).truncate();
                let x_dot = (pointer_position - translation).dot(x_axis);
                let y_dot = (pointer_position - translation).dot(y_axis);
                x_dot.abs() < size.x.abs() / 2.0 && y_dot.abs() < size.y.abs() / 2.0
            }
        }
    }

    fn create_entity(
        self,
        transform: Transform,
        commands: &mut Commands,
        meshes: &mut ResMut<Assets<Mesh>>,
        materials: &mut ResMut<Assets<ColorMaterial>>,
    ) -> Entity {
        match self {
            WorldObject::Block { fixed } => {
                let color = if fixed {
                    Color::BLACK
                } else {
                    Color::DARK_GRAY
                };
                commands
                    .spawn(self)
                    .insert(MaterialMesh2dBundle {
                        mesh: meshes.add(Mesh::from(shape::Quad::new(Vec2::ONE))).into(),
                        material: materials.add(ColorMaterial::from(color)),
                        transform,
                        ..default()
                    })
                    .id()
            }
            WorldObject::Player => commands
                .spawn(self)
                .insert(MaterialMesh2dBundle {
                    mesh: meshes
                        .add(Mesh::from(shape::Capsule {
                            radius: PLAYER_RADIUS,
                            rings: 20,
                            depth: PLAYER_DEPTH,
                            latitudes: 20,
                            longitudes: 20,
                            uv_profile: shape::CapsuleUvProfile::Uniform,
                        }))
                        .into(),
                    material: materials.add(ColorMaterial::from(Color::GRAY)),
                    transform,
                    ..default()
                })
                .id(),
            WorldObject::Goal => commands
                .spawn(self)
                .insert(MaterialMesh2dBundle {
                    mesh: meshes.add(Mesh::from(shape::Quad::new(Vec2::ONE))).into(),
                    material: materials.add(ColorMaterial::from(Color::rgba(0.0, 1.0, 0.0, 0.5))),
                    transform,
                    ..default()
                })
                .id(),
        }
    }
}

struct DragState {
    initial_pointer_offset: Vec2,
    initial_camera_translation: Vec2,
}

enum RectDrag {
    // The Vec2 and f32 store the initial value which will be changed by dragging.
    None(Vec2),
    Left(Vec2),
    Right(Vec2),
    Top(Vec2),
    Bottom(Vec2),
    Rotation(f32),
}

enum TransformEditors {
    Rect {
        left: Entity,
        right: Entity,
        top: Entity,
        bottom: Entity,
        rotation: Entity,
        dragging: RectDrag,
    },
    None {
        initial_translation: Vec2,
    },
}

impl TransformEditors {
    fn despawn_transform_editors(self, commands: &mut Commands) {
        match self {
            TransformEditors::Rect {
                left,
                right,
                top,
                bottom,
                rotation,
                ..
            } => {
                commands.entity(left).despawn();
                commands.entity(right).despawn();
                commands.entity(top).despawn();
                commands.entity(bottom).despawn();
                commands.entity(rotation).despawn();
            }
            TransformEditors::None { .. } => {}
        }
    }

    fn update_transform(
        &self,
        entity_transform: &Transform,
        transform_editors: &mut Query<
            (Entity, &mut Transform, &TransformEditor),
            (Without<WorldObject>, Without<Camera>),
        >,
    ) {
        match &self {
            TransformEditors::Rect {
                left,
                right,
                top,
                bottom,
                rotation,
                ..
            } => {
                let translation = entity_transform.translation.truncate();
                let size = entity_transform.scale.truncate();
                let x_axis = (entity_transform.rotation * Vec3::X).truncate();
                let y_axis = (entity_transform.rotation * Vec3::Y).truncate();
                let z_index = entity_transform.translation.z;
                let (_, mut rotation_transform, _) = transform_editors.get_mut(*rotation).unwrap();
                rotation_transform.translation = translation.extend(z_index + 1.0);
                let (_, mut left_transform, _) = transform_editors.get_mut(*left).unwrap();
                left_transform.translation =
                    (translation - x_axis * size.x / 2.0).extend(z_index + 2.0);
                let (_, mut right_transform, _) = transform_editors.get_mut(*right).unwrap();
                right_transform.translation =
                    (translation + x_axis * size.x / 2.0).extend(z_index + 2.0);
                let (_, mut top_transform, _) = transform_editors.get_mut(*top).unwrap();
                top_transform.translation =
                    (translation + y_axis * size.y / 2.0).extend(z_index + 2.0);
                let (_, mut bottom_transform, _) = transform_editors.get_mut(*bottom).unwrap();
                bottom_transform.translation =
                    (translation - y_axis * size.y / 2.0).extend(z_index + 2.0);
            }
            TransformEditors::None { .. } => {}
        }
    }
}

struct SelectedState {
    entity: Entity,
    transform_editors: TransformEditors,
    prev_z_index: f32,
}

impl SelectedState {
    fn can_drag(
        &self,
        pointer_position: Vec2,
        objects: &mut Query<(Entity, &mut WorldObject, &mut Transform)>,
        transform_editors: &mut Query<
            (Entity, &mut Transform, &TransformEditor),
            (Without<WorldObject>, Without<Camera>),
        >,
    ) -> bool {
        for (_, transform, transform_editor) in transform_editors {
            let distance_from_center =
                (transform.translation.truncate() - pointer_position).length();
            match transform_editor {
                TransformEditor::Anchor => {
                    if distance_from_center < ANCHOR_RADIUS * transform.scale.x {
                        return true;
                    }
                }
                TransformEditor::Ring => {
                    if RING_INNER_RADIUS * transform.scale.x < distance_from_center
                        && distance_from_center < RING_OUTER_RADIUS * transform.scale.x
                    {
                        return true;
                    }
                }
            }
        }
        let (_, object, transform) = objects.get(self.entity).unwrap();
        object.can_drag(transform, pointer_position)
    }

    fn clear_selection(
        self,
        objects: &mut Query<(Entity, &mut WorldObject, &mut Transform)>,
        commands: &mut Commands,
    ) {
        // TODO: Handle deletion of selected entity?
        let (_, _, mut transform) = objects.get_mut(self.entity).unwrap();
        transform.translation.z = self.prev_z_index;
        self.transform_editors.despawn_transform_editors(commands);
    }

    fn drag_start(
        &mut self,
        pointer_position: Vec2,
        camera_scale: f32,
        objects: &mut Query<(Entity, &mut WorldObject, &mut Transform)>,
        selected_by_drag: bool,
    ) {
        match &mut self.transform_editors {
            TransformEditors::Rect { dragging, .. } => {
                let (_, object, transform) = objects.get(self.entity).unwrap();

                let translation = transform.translation.truncate();
                let size = transform.scale.truncate();
                let x_axis = (transform.rotation * Vec3::X).truncate();
                let y_axis = (transform.rotation * Vec3::Y).truncate();

                *dragging = if selected_by_drag {
                    RectDrag::None(transform.translation.truncate())
                } else if (pointer_position - (translation - x_axis * size.x / 2.0)).length()
                    < ANCHOR_RADIUS * camera_scale
                {
                    RectDrag::Left(translation - x_axis * size.x / 2.0)
                } else if (pointer_position - (translation + x_axis * size.x / 2.0)).length()
                    < ANCHOR_RADIUS * camera_scale
                {
                    RectDrag::Right(translation + x_axis * size.x / 2.0)
                } else if (pointer_position - (translation + y_axis * size.y / 2.0)).length()
                    < ANCHOR_RADIUS * camera_scale
                {
                    RectDrag::Top(translation + y_axis * size.y / 2.0)
                } else if (pointer_position - (translation - y_axis * size.y / 2.0)).length()
                    < ANCHOR_RADIUS * camera_scale
                {
                    RectDrag::Bottom(translation - y_axis * size.y / 2.0)
                } else if RING_INNER_RADIUS * camera_scale
                    < (translation - pointer_position).length()
                    && (translation - pointer_position).length() < RING_OUTER_RADIUS * camera_scale
                {
                    RectDrag::Rotation(transform.rotation.to_euler(EulerRot::XYZ).2)
                } else if object.can_drag(transform, pointer_position) {
                    RectDrag::None(transform.translation.truncate())
                } else {
                    unreachable!("Should be draggable.")
                };
            }
            TransformEditors::None {
                initial_translation,
            } => {
                let (_, _, transform) = objects.get(self.entity).unwrap();
                *initial_translation = transform.translation.truncate();
            }
        }
    }

    fn drag(
        &mut self,
        objects: &mut Query<(Entity, &mut WorldObject, &mut Transform)>,
        transform_editors: &mut Query<
            (Entity, &mut Transform, &TransformEditor),
            (Without<WorldObject>, Without<Camera>),
        >,
        initial_pointer_position: Vec2,
        pointer_position: Vec2,
    ) {
        match &self.transform_editors {
            TransformEditors::Rect { dragging, .. } => {
                let (_, _, mut rect_transform) = objects.get_mut(self.entity).unwrap();

                let translation = rect_transform.translation.truncate();
                let size = rect_transform.scale.truncate();
                let x_axis = (rect_transform.rotation * Vec3::X).truncate();
                let y_axis = (rect_transform.rotation * Vec3::Y).truncate();

                match dragging {
                    RectDrag::None(initial_translation) => {
                        let new_position =
                            *initial_translation + (pointer_position - initial_pointer_position);
                        rect_transform.translation.x = new_position.x;
                        rect_transform.translation.y = new_position.y;
                    }
                    RectDrag::Rotation(initial_rotation) => {
                        let initial_offset_from_center =
                            initial_pointer_position - rect_transform.translation.truncate();
                        let offset_from_center =
                            pointer_position - rect_transform.translation.truncate();
                        let rotation_change = if offset_from_center.length() > 1e-7 {
                            initial_offset_from_center.angle_between(offset_from_center)
                        } else {
                            0.0
                        };
                        rect_transform.rotation =
                            Quat::from_rotation_z(initial_rotation + rotation_change);
                    }
                    RectDrag::Left(initial_translation) => {
                        let new_position =
                            *initial_translation + (pointer_position - initial_pointer_position);
                        let left_anchor_position =
                            translation + (new_position - translation).dot(x_axis) * x_axis;
                        let right_anchor_position = translation + x_axis * size.x / 2.0;
                        rect_transform.translation.x =
                            ((left_anchor_position + right_anchor_position) / 2.0).x;
                        rect_transform.translation.y =
                            ((left_anchor_position + right_anchor_position) / 2.0).y;
                        rect_transform.scale.x =
                            (right_anchor_position - left_anchor_position).dot(x_axis);
                    }
                    RectDrag::Right(initial_translation) => {
                        let new_position =
                            *initial_translation + (pointer_position - initial_pointer_position);
                        let left_anchor_position = translation - x_axis * size.x / 2.0;
                        let right_anchor_position =
                            translation + (new_position - translation).dot(x_axis) * x_axis;
                        rect_transform.translation.x =
                            ((left_anchor_position + right_anchor_position) / 2.0).x;
                        rect_transform.translation.y =
                            ((left_anchor_position + right_anchor_position) / 2.0).y;
                        rect_transform.scale.x =
                            (right_anchor_position - left_anchor_position).dot(x_axis);
                    }
                    RectDrag::Top(initial_translation) => {
                        let new_position =
                            *initial_translation + (pointer_position - initial_pointer_position);
                        let bottom_anchor_position = translation - y_axis * size.y / 2.0;
                        let top_anchor_position =
                            translation + (new_position - translation).dot(y_axis) * y_axis;
                        rect_transform.translation.x =
                            ((bottom_anchor_position + top_anchor_position) / 2.0).x;
                        rect_transform.translation.y =
                            ((bottom_anchor_position + top_anchor_position) / 2.0).y;
                        rect_transform.scale.y =
                            (top_anchor_position - bottom_anchor_position).dot(y_axis);
                    }
                    RectDrag::Bottom(initial_translation) => {
                        let new_position =
                            *initial_translation + (pointer_position - initial_pointer_position);
                        let bottom_anchor_position =
                            translation + (new_position - translation).dot(y_axis) * y_axis;
                        let top_anchor_position = translation + y_axis * size.y / 2.0;
                        rect_transform.translation.x =
                            ((bottom_anchor_position + top_anchor_position) / 2.0).x;
                        rect_transform.translation.y =
                            ((bottom_anchor_position + top_anchor_position) / 2.0).y;
                        rect_transform.scale.y =
                            (top_anchor_position - bottom_anchor_position).dot(y_axis);
                    }
                }

                self.transform_editors
                    .update_transform(&rect_transform, transform_editors);
            }
            TransformEditors::None {
                initial_translation,
            } => {
                let new_position =
                    *initial_translation + (pointer_position - initial_pointer_position);
                let (_, _, mut transform) = objects.get_mut(self.entity).unwrap();
                transform.translation.x = new_position.x;
                transform.translation.y = new_position.y;
            }
        }
    }
}

#[derive(Default, Resource)]
struct EditorUiState {
    drag: Option<DragState>,
    selected: Option<SelectedState>,
}

impl EditorUiState {
    fn clear_selection(
        &mut self,
        objects: &mut Query<(Entity, &mut WorldObject, &mut Transform)>,
        commands: &mut Commands,
    ) {
        if let Some(selected_state) = self.selected.take() {
            selected_state.clear_selection(objects, commands);
        }
    }

    fn create_and_select(
        &mut self,
        world_object: WorldObject,
        position: Vec2,
        camera_scale: f32,
        objects: &mut Query<(Entity, &mut WorldObject, &mut Transform)>,
        commands: &mut Commands,
        meshes: &mut ResMut<Assets<Mesh>>,
        materials: &mut ResMut<Assets<ColorMaterial>>,
    ) {
        self.clear_selection(objects, commands);

        let selection_z_index = objects
            .iter()
            .map(|(_, _, transform)| transform.translation.z)
            .reduce(f32::max)
            .unwrap()
            + 1.0; // We can unwrap as player will always be there.

        let transform = match world_object {
            WorldObject::Block { .. } | WorldObject::Goal => {
                Transform::from_xyz(position.x, position.y, selection_z_index)
                    .with_scale(Vec3::new(50.0, 50.0, 1.0))
            }
            WorldObject::Player => Transform::from_xyz(position.x, position.y, selection_z_index),
        };
        let entity = world_object
            .clone()
            .create_entity(transform, commands, meshes, materials);

        self.selected = Some(SelectedState {
            entity,
            transform_editors: self.create_transform_editors(
                &world_object,
                &transform,
                camera_scale,
                selection_z_index,
                commands,
                meshes,
                materials,
            ),
            prev_z_index: transform.translation.z,
        });
    }

    fn select<'a>(
        &'a mut self,
        entity: Entity,
        camera_scale: f32,
        objects: &mut Query<(Entity, &mut WorldObject, &mut Transform)>,
        commands: &mut Commands,
        meshes: &mut ResMut<Assets<Mesh>>,
        materials: &mut ResMut<Assets<ColorMaterial>>,
    ) -> &'a mut SelectedState {
        self.clear_selection(objects, commands);

        let selection_z_index = objects
            .iter()
            .map(|(_, _, transform)| transform.translation.z)
            .reduce(f32::max)
            .unwrap()
            + 1.0; // We can unwrap as player will always be there.

        let (_, world_object, mut transform) = objects.get_mut(entity).unwrap();

        self.selected = Some(SelectedState {
            entity,
            transform_editors: self.create_transform_editors(
                &world_object,
                &transform,
                camera_scale,
                selection_z_index,
                commands,
                meshes,
                materials,
            ),
            prev_z_index: transform.translation.z,
        });
        transform.translation.z = selection_z_index;
        self.selected.as_mut().unwrap()
    }

    fn create_transform_editors(
        &self,
        world_object: &WorldObject,
        transform: &Transform,
        camera_scale: f32,
        selection_z_index: f32,
        commands: &mut Commands,
        meshes: &mut ResMut<Assets<Mesh>>,
        materials: &mut ResMut<Assets<ColorMaterial>>,
    ) -> TransformEditors {
        match world_object {
            WorldObject::Block { .. } | WorldObject::Goal => {
                let translation = transform.translation.truncate();
                let size = transform.scale.truncate();
                let x_axis = (transform.rotation * Vec3::X).truncate();
                let y_axis = (transform.rotation * Vec3::Y).truncate();
                let rotation = create_ring(
                    (translation.x, translation.y, selection_z_index + 1.0),
                    camera_scale,
                    commands,
                    meshes,
                    materials,
                );
                let left = create_anchor(
                    (translation - x_axis * size.x / 2.0).extend(selection_z_index + 2.0),
                    camera_scale,
                    commands,
                    meshes,
                    materials,
                );
                let right = create_anchor(
                    (translation + x_axis * size.x / 2.0).extend(selection_z_index + 2.0),
                    camera_scale,
                    commands,
                    meshes,
                    materials,
                );
                let top = create_anchor(
                    (translation + y_axis * size.y / 2.0).extend(selection_z_index + 2.0),
                    camera_scale,
                    commands,
                    meshes,
                    materials,
                );
                let bottom = create_anchor(
                    (translation - y_axis * size.y / 2.0).extend(selection_z_index + 2.0),
                    camera_scale,
                    commands,
                    meshes,
                    materials,
                );
                TransformEditors::Rect {
                    left,
                    right,
                    top,
                    bottom,
                    rotation,
                    dragging: RectDrag::None(transform.translation.truncate()),
                }
            }
            WorldObject::Player => TransformEditors::None {
                initial_translation: transform.translation.truncate(),
            },
        }
    }

    fn drag_start(
        &mut self,
        pointer_position: Vec2,
        pointer_offset_from_center: Vec2,
        objects: &mut Query<(Entity, &mut WorldObject, &mut Transform)>,
        transform_editors: &mut Query<
            (Entity, &mut Transform, &TransformEditor),
            (Without<WorldObject>, Without<Camera>),
        >,
        camera_transform: &Transform,
        commands: &mut Commands,
        meshes: &mut ResMut<Assets<Mesh>>,
        materials: &mut ResMut<Assets<ColorMaterial>>,
    ) {
        // First check selected.
        if let Some(selected_state) = &mut self.selected {
            if selected_state.can_drag(pointer_position, objects, transform_editors) {
                selected_state.drag_start(
                    pointer_position,
                    camera_transform.scale.x,
                    objects,
                    false,
                );
                self.drag = Some(DragState {
                    initial_pointer_offset: pointer_offset_from_center,
                    initial_camera_translation: camera_transform.translation.truncate(),
                });
                return;
            } else {
                self.clear_selection(objects, commands);
            }
        }

        let mut drag_entity = None;
        let mut max_drag_z_index: Option<f32> = None;

        for (entity, object, transform) in objects.iter() {
            if let Some(max_drag_z_index) = max_drag_z_index {
                if transform.translation.z <= max_drag_z_index {
                    continue;
                }
            }

            if object.can_drag(transform, pointer_position) {
                max_drag_z_index = Some(transform.translation.z);
                drag_entity = Some(entity);
            }
        }

        if let Some(drag_entity) = drag_entity {
            let selected_state = self.select(
                drag_entity,
                camera_transform.scale.x,
                objects,
                commands,
                meshes,
                materials,
            );
            selected_state.drag_start(pointer_position, camera_transform.scale.x, objects, true);
            self.drag = Some(DragState {
                initial_pointer_offset: pointer_offset_from_center,
                initial_camera_translation: camera_transform.translation.truncate(),
            });
        } else {
            self.drag = Some(DragState {
                initial_pointer_offset: pointer_offset_from_center,
                initial_camera_translation: camera_transform.translation.truncate(),
            });
        }
    }

    fn on_drag(
        &mut self,
        pointer_offset_from_center: Vec2,
        objects: &mut Query<(Entity, &mut WorldObject, &mut Transform)>,
        transform_editors: &mut Query<
            (Entity, &mut Transform, &TransformEditor),
            (Without<WorldObject>, Without<Camera>),
        >,
        camera_transform: &mut Transform,
    ) {
        if let Some(DragState {
            initial_pointer_offset,
            initial_camera_translation,
        }) = self.drag
        {
            if let Some(selected_state) = &mut self.selected {
                selected_state.drag(
                    objects,
                    transform_editors,
                    initial_camera_translation + initial_pointer_offset,
                    initial_camera_translation + pointer_offset_from_center,
                );
            } else {
                // Camera will dragged in the opposite direction,
                // this makes it appear as if the world is dragged in the correct direction.
                let new_position = initial_camera_translation
                    - (pointer_offset_from_center - initial_pointer_offset);
                camera_transform.translation.x = new_position.x;
                camera_transform.translation.y = new_position.y;
            }
        }
    }

    fn drag_end(&mut self) {
        self.drag = None;
    }
}

fn setup_editor(
    mut commands: Commands,
    world: Res<World>,
    mut camera: Query<&mut Transform, With<Camera>>,
    mut ui_state: ResMut<EditorUiState>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
) {
    for object_and_transform in world.objects.iter() {
        object_and_transform.object.clone().create_entity(
            object_and_transform.transform(),
            &mut commands,
            &mut meshes,
            &mut materials,
        );
    }
    let mut camera_transform = camera.iter_mut().next().unwrap();
    camera_transform.translation.x = 0.0;
    camera_transform.translation.y = 0.0;
    *ui_state = EditorUiState::default();
}

fn cleanup_editor(
    mut commands: Commands,
    mut world: ResMut<World>,
    mut ui_state: ResMut<EditorUiState>,
    mut objects: Query<(Entity, &mut WorldObject, &mut Transform)>,
    transform_editors: Query<Entity, (With<TransformEditor>, Without<WorldObject>)>,
    mut camera: Query<&mut Transform, (With<Camera>, Without<WorldObject>)>,
) {
    ui_state.clear_selection(&mut objects, &mut commands);

    for transform_editor in transform_editors.iter() {
        commands.entity(transform_editor).despawn();
    }

    world.objects.clear();
    for (entity, object, transform) in objects.iter() {
        world.objects.push(ObjectAndTransform {
            object: object.clone(),
            position: transform.translation.to_array(),
            scale: transform.scale.to_array(),
            rotation: transform.rotation.to_euler(EulerRot::XYZ).2,
        });
        commands.entity(entity).despawn();
    }

    let mut camera_transform = camera.iter_mut().next().unwrap();
    camera_transform.scale.x = 1.0;
    camera_transform.scale.y = 1.0;
}

fn load_world(
    world: &ResMut<World>,
    commands: &mut Commands,
    objects: &Query<(Entity, &mut WorldObject, &mut Transform)>,
    transform_editors: &Query<
        (Entity, &mut Transform, &TransformEditor),
        (Without<WorldObject>, Without<Camera>),
    >,
    camera: &mut Transform,
    ui_state: &mut ResMut<EditorUiState>,
    meshes: &mut ResMut<Assets<Mesh>>,
    materials: &mut ResMut<Assets<ColorMaterial>>,
) {
    for (transform_editor, _, _) in transform_editors.iter() {
        commands.entity(transform_editor).despawn();
    }

    for (entity, _, _) in objects.iter() {
        commands.entity(entity).despawn();
    }

    for object_and_transform in world.objects.iter() {
        object_and_transform.object.clone().create_entity(
            object_and_transform.transform(),
            commands,
            meshes,
            materials,
        );
    }
    camera.translation.x = 0.0;
    camera.translation.y = 0.0;
    **ui_state = EditorUiState::default();
}

fn editor_ui_system(
    mut next_state: ResMut<NextState<AppState>>,
    mut commands: Commands,
    mut contexts: EguiContexts,
    mut ui_state: ResMut<EditorUiState>,
    mouse_button_input: Res<Input<MouseButton>>,
    mut world: ResMut<World>,
    mut camera: Query<&mut Transform, (With<Camera>, Without<WorldObject>)>,
    mut objects: Query<(Entity, &mut WorldObject, &mut Transform)>,
    mut current_materials: Query<&mut Handle<ColorMaterial>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
    mut transform_editors: Query<
        (Entity, &mut Transform, &TransformEditor),
        (Without<WorldObject>, Without<Camera>),
    >,
    mut mouse_wheel_events: EventReader<MouseWheel>,
) {
    let mut camera_transform = camera.iter_mut().next().unwrap();

    let response = egui::Window::new("World editor")
        .scroll2([false, true])
        .show(contexts.ctx_mut(), |ui| {
            let mut new_state = None;

            ui.horizontal(|ui| {
                if ui.button("Play world").clicked() {
                    new_state = Some(AppState::Game);
                }

                let has_goal = objects
                    .iter()
                    .any(|(_, object, _)| matches!(object, WorldObject::Goal));

                if has_goal && ui.button("Train agent on world").clicked() {
                    new_state = Some(AppState::Train);
                }
            });

            if let Some(state) = new_state {
                next_state.set(state);
                return;
            }

            ui.add_space(10.0);

            ui.horizontal(|ui| {
                if ui.button("Open").clicked() {
                    if let Some(path) = rfd::FileDialog::new().pick_file() {
                        let new_world: Option<World> = fs::read_to_string(path)
                            .ok()
                            .and_then(|s| serde_json::from_str(&s).ok());

                        if let Some(new_world) = new_world {
                            let has_player = new_world
                                .objects
                                .iter()
                                .any(|object| matches!(object.object, WorldObject::Player));

                            if has_player {
                                *world = new_world;
                                load_world(
                                    &world,
                                    &mut commands,
                                    &objects,
                                    &transform_editors,
                                    &mut camera_transform,
                                    &mut ui_state,
                                    &mut meshes,
                                    &mut materials,
                                );
                            }
                        }
                    }
                }

                if ui.button("Save").clicked() {
                    if let Some(path) = rfd::FileDialog::new().save_file() {
                        let objects = objects
                            .iter()
                            .map(|(_, object, transform)| ObjectAndTransform {
                                object: object.clone(),
                                position: transform.translation.to_array(),
                                scale: transform.scale.to_array(),
                                rotation: transform.rotation.to_euler(EulerRot::XYZ).2,
                            })
                            .collect();
                        let world = World { objects };
                        if fs::write(path, serde_json::to_string(&world).unwrap()).is_err() {
                            // TODO: Show error in the UI.
                            println!("Couldn't save the world.");
                        }
                    }
                }
            });

            ui.add_space(10.0);

            if let Some(selected) = &mut ui_state.selected {
                let (_, mut object, mut transform) = objects.get_mut(selected.entity).unwrap();

                let mut back_clicked = false;
                let mut delete_clicked = false;

                ui.horizontal(|ui| {
                    if ui.button("Back").clicked() {
                        back_clicked = true;
                        return;
                    }

                    ui.add_space(100.0);

                    if !matches!(&*object, WorldObject::Player) && ui.button("Delete").clicked() {
                        delete_clicked = true;
                    }
                });

                if back_clicked {
                    ui_state.clear_selection(&mut objects, &mut commands);
                    return;
                }

                if delete_clicked {
                    let entity = selected.entity;
                    ui_state.clear_selection(&mut objects, &mut commands);
                    commands.entity(entity).despawn();
                    return;
                }

                ui.add_space(10.0);

                match &mut *object {
                    WorldObject::Player => {
                        ui.label("Player");
                        egui::Grid::new("Player grid")
                            .spacing([25.0, 5.0])
                            .show(ui, |ui| {
                                ui.label("Transform:");
                                ui.horizontal(|ui| {
                                    ui.add(DragValue::new(&mut transform.translation.x));
                                    ui.add(DragValue::new(&mut transform.translation.y));
                                });
                                ui.end_row();
                            });
                    }
                    WorldObject::Block { fixed } => {
                        let prev_fixed = *fixed;
                        ui.label("Block");
                        egui::Grid::new("Block grid")
                            .spacing([25.0, 5.0])
                            .show(ui, |ui| {
                                ui.label("Translation:");
                                ui.horizontal(|ui| {
                                    ui.add(DragValue::new(&mut transform.translation.x));
                                    ui.add(DragValue::new(&mut transform.translation.y));
                                });
                                ui.end_row();

                                ui.label("Scale:");
                                ui.horizontal(|ui| {
                                    ui.add(DragValue::new(&mut transform.scale.x));
                                    ui.add(DragValue::new(&mut transform.scale.y));
                                });
                                ui.end_row();

                                ui.label("Rotation:");
                                let mut rotation =
                                    transform.rotation.to_euler(EulerRot::XYZ).2 * 180.0 / PI;
                                ui.add(DragValue::new(&mut rotation));
                                transform.rotation = Quat::from_rotation_z(rotation * PI / 180.0);
                                ui.end_row();

                                ui.label("Fixed");
                                ui.checkbox(fixed, "");
                                ui.end_row();
                            });
                        selected
                            .transform_editors
                            .update_transform(&transform, &mut transform_editors);

                        if *fixed != prev_fixed {
                            let mut selected_material =
                                current_materials.get_mut(selected.entity).unwrap();
                            let color = if *fixed {
                                Color::BLACK
                            } else {
                                Color::DARK_GRAY
                            };
                            *selected_material = materials.add(ColorMaterial::from(color));
                        }
                    }
                    WorldObject::Goal => {
                        ui.label("Goal");
                        egui::Grid::new("Goal grid")
                            .spacing([25.0, 5.0])
                            .show(ui, |ui| {
                                ui.label("Translation:");
                                ui.horizontal(|ui| {
                                    ui.add(DragValue::new(&mut transform.translation.x));
                                    ui.add(DragValue::new(&mut transform.translation.y));
                                });
                                ui.end_row();

                                ui.label("Scale:");
                                ui.horizontal(|ui| {
                                    ui.add(DragValue::new(&mut transform.scale.x));
                                    ui.add(DragValue::new(&mut transform.scale.y));
                                });
                                ui.end_row();

                                ui.label("Rotation:");
                                let mut rotation =
                                    transform.rotation.to_euler(EulerRot::XYZ).2 * 180.0 / PI;
                                ui.add(DragValue::new(&mut rotation));
                                transform.rotation = Quat::from_rotation_z(rotation * PI / 180.0);
                                ui.end_row();
                            });
                        selected
                            .transform_editors
                            .update_transform(&transform, &mut transform_editors);
                    }
                }
            } else {
                ui.horizontal(|ui| {
                    let new_objects = [
                        ("block", WorldObject::Block { fixed: true }),
                        ("goal", WorldObject::Goal),
                    ];
                    for (name, object) in new_objects {
                        if ui.button(format!("New {name}")).clicked() {
                            ui_state.create_and_select(
                                object,
                                camera_transform.translation.truncate(),
                                camera_transform.scale.x,
                                &mut objects,
                                &mut commands,
                                &mut meshes,
                                &mut materials,
                            );
                        }
                    }
                });

                ui.add_space(10.0);

                ui.label("Objects:");

                egui::Grid::new("Object grid")
                    .spacing([50.0, 5.0])
                    .show(ui, |ui| {
                        for (entity, object, transform) in objects.iter_mut() {
                            let name = match *object {
                                WorldObject::Player => "Player",
                                WorldObject::Block { .. } => "Block",
                                WorldObject::Goal => "Goal",
                            };
                            if ui.button(name).clicked() {
                                camera_transform.translation.x = transform.translation.x;
                                camera_transform.translation.y = transform.translation.y;
                                ui_state.select(
                                    entity,
                                    camera_transform.scale.x,
                                    &mut objects,
                                    &mut commands,
                                    &mut meshes,
                                    &mut materials,
                                );
                                return;
                            }

                            if !matches!(&*object, WorldObject::Player)
                                && ui.button("Delete").clicked()
                            {
                                commands.entity(entity).despawn();
                                return;
                            }
                            ui.end_row();
                        }
                    });
            }
        });

    let response = if let Some(response) = response {
        response.response
    } else {
        return;
    };

    let pointer_position = if let Some(position) = contexts.ctx_mut().pointer_latest_pos() {
        position
    } else {
        return;
    };

    let pointer_on_egui = response.rect.contains(pointer_position);

    let screen_rect = contexts.ctx_mut().screen_rect();
    let pointer_offset_from_center = pointer_position - screen_rect.center();
    let mut pointer_offset_from_center =
        Vec2::new(pointer_offset_from_center.x, pointer_offset_from_center.y);
    pointer_offset_from_center.y *= -1.0; // Bevy's and EGUI's +y-axis have different directions.
    pointer_offset_from_center *= camera_transform.scale.x;
    let pointer_position = camera_transform.translation.truncate() + pointer_offset_from_center;

    if mouse_button_input.just_pressed(MouseButton::Left) {
        if !pointer_on_egui {
            ui_state.drag_start(
                pointer_position,
                pointer_offset_from_center,
                &mut objects,
                &mut transform_editors,
                &camera_transform,
                &mut commands,
                &mut meshes,
                &mut materials,
            );
        }
    } else if mouse_button_input.pressed(MouseButton::Left) {
        ui_state.on_drag(
            pointer_offset_from_center,
            &mut objects,
            &mut transform_editors,
            &mut camera_transform,
        );
    } else if mouse_button_input.just_released(MouseButton::Left) {
        ui_state.on_drag(
            pointer_offset_from_center,
            &mut objects,
            &mut transform_editors,
            &mut camera_transform,
        );
        ui_state.drag_end();
    }

    if !pointer_on_egui && ui_state.drag.is_none() {
        for event in mouse_wheel_events.iter() {
            let scale = camera_transform.scale.x;
            let new_scale = (scale * 0.9_f32.powf(event.y)).max(0.01);
            camera_transform.scale.x = new_scale;
            camera_transform.scale.y = new_scale;
            for (_, mut transform, transform_editor) in transform_editors.iter_mut() {
                match transform_editor {
                    TransformEditor::Anchor => {
                        transform.scale.x = new_scale;
                        transform.scale.y = new_scale;
                    }
                    TransformEditor::Ring => {
                        // The torus was initially parallel to the XZ plane, so we scale those directions.
                        transform.scale.x = new_scale;
                        transform.scale.z = new_scale;
                    }
                }
            }
            let new_translation = new_scale
                * (camera_transform.translation.truncate() / scale
                    + pointer_position * (1.0 / new_scale - 1.0 / scale));
            camera_transform.translation.x = new_translation.x;
            camera_transform.translation.y = new_translation.y;
        }
    }
}
