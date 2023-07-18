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
        // TODO: Handle deletion of selected entity?
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
        entity: Entity,
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

    fn drag_start(
        &mut self,
        pointer_position: Vec2,
        pointer_offset_from_center: Vec2,
        objects: &mut Query<(Entity, &mut WorldObject, &mut Transform)>,
        anchors: &mut Query<
            (Entity, &mut Transform),
            (With<Anchor>, Without<WorldObject>, Without<Camera>),
        >,
        camera_transform: &Transform,
        commands: &mut Commands,
        meshes: &mut ResMut<Assets<Mesh>>,
        materials: &mut ResMut<Assets<ColorMaterial>>,
    ) {
        // First check selected.
        if let Some(selected_state) = &mut self.selected {
            if selected_state.can_drag(pointer_position, objects, anchors) {
                let initial_translation =
                    selected_state.drag_start(pointer_position, objects, false);
                self.drag = Some(DragState {
                    initial_pointer_offset: pointer_offset_from_center,
                    initial_translation,
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
            let selected_state = self.select(drag_entity, objects, commands, meshes, materials);
            let initial_translation = selected_state.drag_start(pointer_position, objects, true);
            self.drag = Some(DragState {
                initial_pointer_offset: pointer_offset_from_center,
                initial_translation,
            });
        } else {
            self.drag = Some(DragState {
                initial_pointer_offset: pointer_offset_from_center,
                initial_translation: camera_transform.translation.truncate(),
            });
        }
    }

    fn on_drag(
        &mut self,
        pointer_offset_from_center: Vec2,
        objects: &mut Query<(Entity, &mut WorldObject, &mut Transform)>,
        anchors: &mut Query<
            (Entity, &mut Transform),
            (With<Anchor>, Without<WorldObject>, Without<Camera>),
        >,
        camera_transform: &mut Transform,
    ) {
        if let Some(DragState {
            initial_pointer_offset,
            initial_translation,
        }) = self.drag
        {
            if let Some(selected_state) = &mut self.selected {
                let new_position =
                    initial_translation + pointer_offset_from_center - initial_pointer_offset;
                selected_state.drag(objects, anchors, new_position);
            } else {
                // Camera will dragged in the opposite direction,
                // this makes it appear as if the world is dragged in the correct direction.
                let new_position =
                    initial_translation - (pointer_offset_from_center - initial_pointer_offset);
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
    mut ui_state: ResMut<EditorUiState>,
    mut objects: Query<(Entity, &mut WorldObject, &mut Transform)>,
    anchors: Query<Entity, (With<Anchor>, Without<WorldObject>)>,
) {
    ui_state.clear_selection(&mut objects, &mut commands);

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
    camera: &mut Transform,
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
    mut anchors: Query<
        (Entity, &mut Transform),
        (With<Anchor>, Without<WorldObject>, Without<Camera>),
    >,
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
                                    &anchors,
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

                if ui.button("Back").clicked() {
                    ui_state.clear_selection(&mut objects, &mut commands);
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
                                ui.label("Transform:");
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

                                ui.label("Fixed");
                                ui.checkbox(fixed, "");
                                ui.end_row();
                            });
                        selected.anchors.update_transform(&transform, &mut anchors);

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
                                ui.label("Transform:");
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
                            });
                        selected.anchors.update_transform(&transform, &mut anchors);
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
    let pointer_position = camera_transform.translation.truncate() + pointer_offset_from_center;

    if mouse_button_input.just_pressed(MouseButton::Left) {
        if !pointer_on_egui {
            ui_state.drag_start(
                pointer_position,
                pointer_offset_from_center,
                &mut objects,
                &mut anchors,
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
            &mut anchors,
            &mut camera_transform,
        );
    } else if mouse_button_input.just_released(MouseButton::Left) {
        ui_state.on_drag(
            pointer_offset_from_center,
            &mut objects,
            &mut anchors,
            &mut camera_transform,
        );
        ui_state.drag_end();
    }
}
