use crate::common::{
    AppState, ObjectAndTransform, World, WorldObject, PLAYER_DEPTH, PLAYER_RADIUS,
};

use bevy::{prelude::*, sprite::MaterialMesh2dBundle};
use bevy_egui::{
    egui::{self, DragValue},
    EguiContexts,
};
use std::fs;

const ANCHOR_RADIUS: f32 = 5.0;

pub fn add_editor_systems(app: &mut App) {
    app.init_resource::<EditorUiState>()
        .add_system(setup_editor.in_schedule(OnEnter(AppState::Editor)))
        .add_system(editor_ui_system.in_set(OnUpdate(AppState::Editor)))
        .add_system(cleanup_editor.in_schedule(OnExit(AppState::Editor)));
}

#[derive(Component)]
struct Anchor;

fn create_anchor(
    position: (f32, f32, f32),
    commands: &mut Commands,
    meshes: &mut ResMut<Assets<Mesh>>,
    materials: &mut ResMut<Assets<ColorMaterial>>,
) -> Entity {
    commands
        .spawn(MaterialMesh2dBundle {
            mesh: meshes.add(shape::Circle::new(ANCHOR_RADIUS).into()).into(),
            material: materials.add(ColorMaterial::from(Color::RED)),
            transform: Transform::from_xyz(position.0, position.1, position.2),
            ..default()
        })
        .insert(Anchor)
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
                (pointer_position - translation).x.abs() < size.x.abs() / 2.0
                    && (pointer_position - translation).y.abs() < size.y.abs() / 2.0
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
            WorldObject::Block { .. } => commands
                .spawn(self)
                .insert(MaterialMesh2dBundle {
                    mesh: meshes.add(Mesh::from(shape::Quad::new(Vec2::ONE))).into(),
                    material: materials.add(ColorMaterial::from(Color::BLACK)),
                    transform,
                    ..default()
                })
                .id(),
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
    initial_point: Vec2,
    initial_translation: Vec2,
}

enum RectAnchor {
    Left,
    Right,
    Top,
    Bottom,
}

enum Anchors {
    Rect {
        left: Entity,
        right: Entity,
        top: Entity,
        bottom: Entity,
        drag_anchor: Option<RectAnchor>,
    },
    None,
}

impl Anchors {
    fn despawn_anchors(self, commands: &mut Commands) {
        match self {
            Anchors::Rect {
                left,
                right,
                top,
                bottom,
                ..
            } => {
                commands.entity(left).despawn();
                commands.entity(right).despawn();
                commands.entity(top).despawn();
                commands.entity(bottom).despawn();
            }
            Anchors::None => {}
        }
    }

    fn update_transform(
        &self,
        entity_transform: &Transform,
        anchors: &mut Query<
            (Entity, &mut Transform),
            (With<Anchor>, Without<WorldObject>, Without<Camera>),
        >,
    ) {
        match &self {
            Anchors::Rect {
                left,
                right,
                top,
                bottom,
                ..
            } => {
                let translation = entity_transform.translation.truncate();
                let size = entity_transform.scale.truncate();
                let width = Vec2::new(size.x, 0.0);
                let height = Vec2::new(0.0, size.y);
                let z_index = entity_transform.translation.z + 1.0;
                let (_, mut left_transform) = anchors.get_mut(*left).unwrap();
                left_transform.translation = (translation - width / 2.0).extend(z_index);
                let (_, mut right_transform) = anchors.get_mut(*right).unwrap();
                right_transform.translation = (translation + width / 2.0).extend(z_index);
                let (_, mut top_transform) = anchors.get_mut(*top).unwrap();
                top_transform.translation = (translation + height / 2.0).extend(z_index);
                let (_, mut bottom_transform) = anchors.get_mut(*bottom).unwrap();
                bottom_transform.translation = (translation - height / 2.0).extend(z_index);
            }
            Anchors::None => {}
        }
    }
}

struct SelectedState {
    entity: Entity,
    anchors: Anchors,
    prev_z_index: f32,
}

impl SelectedState {
    fn can_drag(
        &self,
        pointer_position: Vec2,
        objects: &mut Query<(Entity, &mut WorldObject, &mut Transform)>,
        anchors: &mut Query<
            (Entity, &mut Transform),
            (With<Anchor>, Without<WorldObject>, Without<Camera>),
        >,
    ) -> bool {
        for (_, transform) in anchors {
            if (transform.translation.truncate() - pointer_position).length() < ANCHOR_RADIUS {
                return true;
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
        // Handle deletion of selected entity?
        let (_, _, mut transform) = objects.get_mut(self.entity).unwrap();
        transform.translation.z = self.prev_z_index;
        self.anchors.despawn_anchors(commands);
    }

    fn drag_start(
        &mut self,
        pointer_position: Vec2,
        objects: &mut Query<(Entity, &mut WorldObject, &mut Transform)>,
        selected_by_drag: bool,
    ) -> Vec2 {
        match &mut self.anchors {
            Anchors::Rect { drag_anchor, .. } => {
                let (_, object, transform) = objects.get(self.entity).unwrap();
                let translation = transform.translation.truncate();
                let [size_x, size_y] = transform.scale.truncate().to_array();
                let width = Vec2::new(size_x, 0.0);
                let height = Vec2::new(0.0, size_y);

                if selected_by_drag {
                    *drag_anchor = None;
                    transform.translation.truncate()
                } else if (translation - width / 2.0 - pointer_position).length() < ANCHOR_RADIUS {
                    *drag_anchor = Some(RectAnchor::Left);
                    translation - width / 2.0
                } else if (translation + width / 2.0 - pointer_position).length() < ANCHOR_RADIUS {
                    *drag_anchor = Some(RectAnchor::Right);
                    translation + width / 2.0
                } else if (translation + height / 2.0 - pointer_position).length() < ANCHOR_RADIUS {
                    *drag_anchor = Some(RectAnchor::Top);
                    translation + height / 2.0
                } else if (translation - height / 2.0 - pointer_position).length() < ANCHOR_RADIUS {
                    *drag_anchor = Some(RectAnchor::Bottom);
                    translation - height / 2.0
                } else if object.can_drag(transform, pointer_position) {
                    *drag_anchor = None;
                    transform.translation.truncate()
                } else {
                    unreachable!("Should be draggable.")
                }
            }
            Anchors::None => {
                let (_, _, transform) = objects.get(self.entity).unwrap();
                transform.translation.truncate()
            }
        }
    }

    fn drag(
        &mut self,
        objects: &mut Query<(Entity, &mut WorldObject, &mut Transform)>,
        anchors: &mut Query<
            (Entity, &mut Transform),
            (With<Anchor>, Without<WorldObject>, Without<Camera>),
        >,
        new_position: Vec2,
    ) {
        match &self.anchors {
            Anchors::Rect { drag_anchor, .. } => {
                let (_, _, mut rect_transform) = objects.get_mut(self.entity).unwrap();

                let mut size = rect_transform.scale.truncate();
                match drag_anchor {
                    None => {
                        rect_transform.translation.x = new_position.x;
                        rect_transform.translation.y = new_position.y;
                    }
                    Some(RectAnchor::Left) => {
                        let new_translation_x =
                            (new_position.x + rect_transform.translation.x + size.x / 2.0) / 2.0;
                        let new_size_x =
                            rect_transform.translation.x + size.x / 2.0 - new_position.x;
                        size.x = new_size_x;
                        rect_transform.scale.x = new_size_x;
                        rect_transform.translation.x = new_translation_x;
                    }
                    Some(RectAnchor::Right) => {
                        let new_translation_x =
                            (new_position.x + rect_transform.translation.x - size.x / 2.0) / 2.0;
                        let new_size_x =
                            new_position.x - (rect_transform.translation.x - size.x / 2.0);
                        size.x = new_size_x;
                        rect_transform.scale.x = new_size_x;
                        rect_transform.translation.x = new_translation_x;
                    }
                    Some(RectAnchor::Top) => {
                        let new_translation_y =
                            (new_position.y + rect_transform.translation.y - size.y / 2.0) / 2.0;
                        let new_size_y =
                            new_position.y - (rect_transform.translation.y - size.y / 2.0);
                        size.y = new_size_y;
                        rect_transform.scale.y = new_size_y;
                        rect_transform.translation.y = new_translation_y;
                    }
                    Some(RectAnchor::Bottom) => {
                        let new_translation_y =
                            (new_position.y + rect_transform.translation.y + size.y / 2.0) / 2.0;
                        let new_size_y =
                            rect_transform.translation.y + size.y / 2.0 - new_position.y;
                        size.y = new_size_y;
                        rect_transform.scale.y = new_size_y;
                        rect_transform.translation.y = new_translation_y;
                    }
                }

                self.anchors.update_transform(&rect_transform, anchors);
            }
            Anchors::None => {
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
    fn create_and_select(
        &mut self,
        world_object: WorldObject,
        position: Vec2,
        selection_z_index: f32,
        commands: &mut Commands,
        meshes: &mut ResMut<Assets<Mesh>>,
        materials: &mut ResMut<Assets<ColorMaterial>>,
    ) {
        let transform = match world_object {
            WorldObject::Block { .. } | WorldObject::Goal => {
                Transform::from_xyz(position.x, position.y, selection_z_index)
                    .with_scale(Vec3::new(20.0, 20.0, 1.0))
            }
            WorldObject::Player => Transform::from_xyz(position.x, position.y, selection_z_index),
        };
        let entity = world_object
            .clone()
            .create_entity(transform, commands, meshes, materials);

        self.selected = Some(SelectedState {
            entity,
            anchors: self.create_anchors(
                &world_object,
                &transform,
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
        world_object: &WorldObject,
        entity: Entity,
        transform: &mut Transform,
        selection_z_index: f32,
        commands: &mut Commands,
        meshes: &mut ResMut<Assets<Mesh>>,
        materials: &mut ResMut<Assets<ColorMaterial>>,
    ) -> &'a mut SelectedState {
        self.selected = Some(SelectedState {
            entity,
            anchors: self.create_anchors(
                world_object,
                transform,
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

    fn create_anchors(
        &self,
        world_object: &WorldObject,
        transform: &Transform,
        selection_z_index: f32,
        commands: &mut Commands,
        meshes: &mut ResMut<Assets<Mesh>>,
        materials: &mut ResMut<Assets<ColorMaterial>>,
    ) -> Anchors {
        match world_object {
            WorldObject::Block { .. } | WorldObject::Goal => {
                let translation = transform.translation;
                let size = transform.scale.truncate();
                let left = create_anchor(
                    (
                        translation.x - size.x / 2.0,
                        translation.y,
                        selection_z_index + 1.0,
                    ),
                    commands,
                    meshes,
                    materials,
                );
                let right = create_anchor(
                    (
                        translation.x + size.x / 2.0,
                        translation.y,
                        selection_z_index + 1.0,
                    ),
                    commands,
                    meshes,
                    materials,
                );
                let top = create_anchor(
                    (
                        translation.x,
                        translation.y + size.y / 2.0,
                        selection_z_index + 1.0,
                    ),
                    commands,
                    meshes,
                    materials,
                );
                let bottom = create_anchor(
                    (
                        translation.x,
                        translation.y - size.y / 2.0,
                        selection_z_index + 1.0,
                    ),
                    commands,
                    meshes,
                    materials,
                );
                Anchors::Rect {
                    left,
                    right,
                    top,
                    bottom,
                    drag_anchor: None,
                }
            }
            WorldObject::Player => Anchors::None,
        }
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
        let ObjectAndTransform {
            object,
            position,
            scale,
        } = object_and_transform;
        let transform = Transform {
            translation: Vec3::from_array(*position),
            scale: Vec3::from_array(*scale),
            ..Default::default()
        };
        object
            .clone()
            .create_entity(transform, &mut commands, &mut meshes, &mut materials);
    }
    let mut camera_transform = camera.iter_mut().next().unwrap();
    camera_transform.translation.x = 0.0;
    camera_transform.translation.y = 0.0;
    *ui_state = EditorUiState::default();
}

fn cleanup_editor(
    mut commands: Commands,
    mut world: ResMut<World>,
    objects: Query<(Entity, &WorldObject, &Transform)>,
    anchors: Query<Entity, (With<Anchor>, Without<WorldObject>)>,
) {
    for anchor in anchors.iter() {
        commands.entity(anchor).despawn();
    }

    world.objects.clear();
    for (entity, object, transform) in objects.iter() {
        world.objects.push(ObjectAndTransform {
            object: object.clone(),
            position: transform.translation.to_array(),
            scale: transform.scale.to_array(),
        });
        commands.entity(entity).despawn();
    }
}

fn load_world(
    world: &ResMut<World>,
    commands: &mut Commands,
    objects: &Query<(Entity, &mut WorldObject, &mut Transform)>,
    anchors: &Query<
        (Entity, &mut Transform),
        (With<Anchor>, Without<WorldObject>, Without<Camera>),
    >,
    camera: &mut Query<&mut Transform, (With<Camera>, Without<WorldObject>)>,
    ui_state: &mut ResMut<EditorUiState>,
    meshes: &mut ResMut<Assets<Mesh>>,
    materials: &mut ResMut<Assets<ColorMaterial>>,
) {
    for (anchor, _) in anchors.iter() {
        commands.entity(anchor).despawn();
    }

    for (entity, _, _) in objects.iter() {
        commands.entity(entity).despawn();
    }

    for object_and_transform in world.objects.iter() {
        let ObjectAndTransform {
            object,
            position,
            scale,
        } = object_and_transform;
        let transform = Transform {
            translation: Vec3::from_array(*position),
            scale: Vec3::from_array(*scale),
            ..Default::default()
        };
        object
            .clone()
            .create_entity(transform, commands, meshes, materials);
    }
    let mut camera_transform = camera.iter_mut().next().unwrap();
    camera_transform.translation.x = 0.0;
    camera_transform.translation.y = 0.0;
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
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
    mut anchors: Query<
        (Entity, &mut Transform),
        (With<Anchor>, Without<WorldObject>, Without<Camera>),
    >,
) {
    let camera_translation = camera.iter_mut().next().unwrap().translation.truncate();

    let response = egui::Window::new("Environment").show(contexts.ctx_mut(), |ui| {
        if ui.button("Play world").clicked() {
            next_state.set(AppState::Game);
        }
        if ui.button("Open").clicked() {
            if let Some(path) = rfd::FileDialog::new().pick_file() {
                let new_world: Result<World, _> =
                    serde_json::from_str(&fs::read_to_string(path).unwrap());
                if let Ok(new_world) = new_world {
                    // Add a check for player in the world.
                    *world = new_world;
                    load_world(
                        &world,
                        &mut commands,
                        &objects,
                        &anchors,
                        &mut camera,
                        &mut ui_state,
                        &mut meshes,
                        &mut materials,
                    );
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
                    })
                    .collect();
                let world = World { objects };
                // Write may fail - remove the unwrap.
                fs::write(path, serde_json::to_string(&world).unwrap()).unwrap();
            }
        }
        if let Some(selected) = &mut ui_state.selected {
            if let Ok((_, mut object, mut transform)) = objects.get_mut(selected.entity) {
                let clear_selection = ui.button("Back").clicked();
                match &mut *object {
                    WorldObject::Player => {
                        ui.label("Player");
                        ui.label("Transform:");
                        ui.horizontal(|ui| {
                            ui.add(DragValue::new(&mut transform.translation.x));
                            ui.add(DragValue::new(&mut transform.translation.y));
                        });
                    }
                    WorldObject::Block { fixed } => {
                        ui.label("Block");
                        ui.label("Transform:");
                        ui.horizontal(|ui| {
                            ui.add(DragValue::new(&mut transform.translation.x));
                            ui.add(DragValue::new(&mut transform.translation.y));
                        });
                        ui.label("Scale:");
                        ui.horizontal(|ui| {
                            ui.add(DragValue::new(&mut transform.scale.x));
                            ui.add(DragValue::new(&mut transform.scale.y));
                        });
                        selected.anchors.update_transform(&transform, &mut anchors);
                        ui.checkbox(fixed, "Fixed");
                    }
                    WorldObject::Goal => {
                        ui.label("Goal");
                        ui.label("Transform:");
                        ui.horizontal(|ui| {
                            ui.add(DragValue::new(&mut transform.translation.x));
                            ui.add(DragValue::new(&mut transform.translation.y));
                        });
                        ui.label("Scale:");
                        ui.horizontal(|ui| {
                            ui.add(DragValue::new(&mut transform.scale.x));
                            ui.add(DragValue::new(&mut transform.scale.y));
                        });
                        selected.anchors.update_transform(&transform, &mut anchors);
                    }
                }
                if clear_selection {
                    ui_state
                        .selected
                        .take()
                        .unwrap()
                        .clear_selection(&mut objects, &mut commands);
                }
            }
        } else {
            let new_objects = [
                ("block", WorldObject::Block { fixed: true }),
                ("goal", WorldObject::Goal),
            ];
            for (name, object) in new_objects {
                if ui.button(format!("New {name}")).clicked() {
                    if let Some(selected_state) = ui_state.selected.take() {
                        selected_state.clear_selection(&mut objects, &mut commands);
                    }

                    let selection_z_index = objects
                        .iter()
                        .map(|(_, _, transform)| transform.translation.z)
                        .reduce(f32::max)
                        .unwrap()
                        + 1.0; // We can unwrap as player will always be there.

                    ui_state.create_and_select(
                        object,
                        camera_translation,
                        selection_z_index,
                        &mut commands,
                        &mut meshes,
                        &mut materials,
                    );
                }
            }

            let selection_z_index = objects
                .iter()
                .map(|(_, _, transform)| transform.translation.z)
                .reduce(f32::max)
                .unwrap()
                + 1.0;

            let mut camera_transform = camera.iter_mut().next().unwrap();

            for (entity, object, mut transform) in objects.iter_mut() {
                let name = match *object {
                    WorldObject::Player => "Player",
                    WorldObject::Block { .. } => "Block",
                    WorldObject::Goal => "Goal",
                };
                match *object {
                    WorldObject::Player => {
                        if ui.button("Player").clicked() {
                            ui_state.select(
                                &object,
                                entity,
                                &mut transform,
                                selection_z_index,
                                &mut commands,
                                &mut meshes,
                                &mut materials,
                            );
                            camera_transform.translation.x = transform.translation.x;
                            camera_transform.translation.y = transform.translation.y;
                        }
                    }
                    WorldObject::Block { .. } | WorldObject::Goal => {
                        ui.horizontal(|ui| {
                            if ui.button(name).clicked() {
                                ui_state.select(
                                    &object,
                                    entity,
                                    &mut transform,
                                    selection_z_index,
                                    &mut commands,
                                    &mut meshes,
                                    &mut materials,
                                );
                                camera_transform.translation.x = transform.translation.x;
                                camera_transform.translation.y = transform.translation.y;
                            }
                            if ui.button("Delete").clicked() {
                                commands.entity(entity).despawn();
                            }
                        });
                    }
                };
            }
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
    let pointer_position = camera_translation + pointer_offset_from_center;

    let mut camera_transform = camera.iter_mut().next().unwrap();

    if mouse_button_input.just_pressed(MouseButton::Left) {
        if !pointer_on_egui {
            // First check selected.
            if let Some(selected_state) = &mut ui_state.selected {
                if selected_state.can_drag(pointer_position, &mut objects, &mut anchors) {
                    let initial_translation =
                        selected_state.drag_start(pointer_position, &mut objects, false);
                    ui_state.drag = Some(DragState {
                        initial_point: pointer_offset_from_center,
                        initial_translation,
                    });
                    return;
                } else {
                    ui_state
                        .selected
                        .take()
                        .unwrap()
                        .clear_selection(&mut objects, &mut commands);
                }
            }

            let max_z_index = objects
                .iter()
                .map(|(_, _, transform)| transform.translation.z)
                .reduce(f32::max);

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
                let (_, object, mut transform) = objects.get_mut(drag_entity).unwrap();

                let selection_z_index = max_z_index.unwrap() + 1.0;
                let selected_state = ui_state.select(
                    &object,
                    drag_entity,
                    &mut transform,
                    selection_z_index,
                    &mut commands,
                    &mut meshes,
                    &mut materials,
                );
                let initial_translation =
                    selected_state.drag_start(pointer_position, &mut objects, true);
                ui_state.drag = Some(DragState {
                    initial_point: pointer_offset_from_center,
                    initial_translation,
                });
            } else {
                ui_state.drag = Some(DragState {
                    initial_point: pointer_offset_from_center,
                    initial_translation: camera_translation,
                });
            }
        }
    } else if mouse_button_input.pressed(MouseButton::Left) {
        if let Some(DragState {
            initial_point,
            initial_translation,
        }) = ui_state.drag
        {
            if let Some(selected_state) = &mut ui_state.selected {
                let new_position = initial_translation + pointer_offset_from_center - initial_point;
                selected_state.drag(&mut objects, &mut anchors, new_position);
            } else {
                let new_position =
                    initial_translation - (pointer_offset_from_center - initial_point);
                camera_transform.translation.x = new_position.x;
                camera_transform.translation.y = new_position.y;
            }
        }
    } else if mouse_button_input.just_released(MouseButton::Left) {
        if let Some(DragState {
            initial_point,
            initial_translation,
        }) = ui_state.drag
        {
            if let Some(selected_state) = &mut ui_state.selected {
                let new_position = initial_translation + pointer_offset_from_center - initial_point;
                selected_state.drag(&mut objects, &mut anchors, new_position);
            } else {
                let new_position =
                    initial_translation - (pointer_offset_from_center - initial_point);
                camera_transform.translation.x = new_position.x;
                camera_transform.translation.y = new_position.y;
            }
            ui_state.drag = None;
        }
    }
}
