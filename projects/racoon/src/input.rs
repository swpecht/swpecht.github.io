use bevy::{input::mouse::MouseWheel, prelude::*, window::PrimaryWindow};

const CAMERA_PAN_SPEED: f32 = 2.0;
const CAMERA_ZOOM_SPEED: f32 = 0.1;

pub struct CursorPlugin {}

impl Plugin for CursorPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, input_setup)
            .add_systems(Update, (cursor_system, camera_pan, camera_zoom));
    }
}

/// We will store the world position of the mouse cursor here.
#[derive(Resource, Default)]
pub struct MouseCoords(Vec2);

impl MouseCoords {
    pub fn loc(&self) -> &Vec2 {
        &self.0
    }
}

/// Used to help identify our main camera
#[derive(Component)]
pub struct MainCamera;

fn input_setup(mut commands: Commands) {
    commands.init_resource::<MouseCoords>();
    // Make sure to add the marker component when you set up your camera
    commands.spawn((Camera2dBundle::default(), MainCamera));
}

fn cursor_system(
    mut mycoords: ResMut<MouseCoords>,
    // query to get the window (so we can read the current cursor position)
    q_window: Query<&Window, With<PrimaryWindow>>,
    // query to get camera transform
    q_camera: Query<(&Camera, &GlobalTransform), With<MainCamera>>,
) {
    // get the camera info and transform
    // assuming there is exactly one main camera entity, so Query::single() is OK
    let (camera, camera_transform) = q_camera.single();

    // There is only one primary window, so we can similarly get it from the query:
    let window = q_window.single();

    // check if the cursor is inside the window and get its position
    // then, ask bevy to convert into world coordinates, and truncate to discard Z
    if let Some(world_position) = window
        .cursor_position()
        .and_then(|cursor| camera.viewport_to_world(camera_transform, cursor))
        .map(|ray| ray.origin.truncate())
    {
        mycoords.0 = world_position;
    }
}

fn camera_pan(
    mut q_camera: Query<(&Camera, &mut Transform), With<MainCamera>>,
    keys: Res<Input<KeyCode>>,
) {
    let (_, mut camera_transform) = q_camera.single_mut();
    if keys.pressed(KeyCode::W) {
        camera_transform.translation += Vec3::Y * CAMERA_PAN_SPEED;
    }

    if keys.pressed(KeyCode::A) {
        camera_transform.translation -= Vec3::X * CAMERA_PAN_SPEED;
    }

    if keys.pressed(KeyCode::S) {
        camera_transform.translation -= Vec3::Y * CAMERA_PAN_SPEED;
    }

    if keys.pressed(KeyCode::D) {
        camera_transform.translation += Vec3::X * CAMERA_PAN_SPEED;
    }
}

fn camera_zoom(
    mut query: Query<&mut OrthographicProjection, With<Camera>>,
    mut scroll_evr: EventReader<MouseWheel>,
) {
    let mut scroll_amount = 0.;
    use bevy::input::mouse::MouseScrollUnit;
    for ev in scroll_evr.iter() {
        match ev.unit {
            MouseScrollUnit::Line => {
                scroll_amount += ev.y;
            }
            MouseScrollUnit::Pixel => {
                scroll_amount += ev.y;
            }
        }
    }

    for mut projection in query.iter_mut() {
        let mut log_scale = projection.scale.ln();
        log_scale -= CAMERA_ZOOM_SPEED * scroll_amount;
        projection.scale = log_scale.exp();
    }
}
