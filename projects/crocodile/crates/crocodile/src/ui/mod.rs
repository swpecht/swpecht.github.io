use animation::animate_sprite;
use bevy::{input::common_conditions::*, math::vec2, prelude::*, window::PrimaryWindow};
use character::{spawn_character, CharacterSpawnEvent};
use sprite::*;

use crate::{
    gamestate::{Action, SimCoords, SimId, SimState},
    PlayState,
};

pub mod animation;
pub mod character;
pub mod sprite;

pub(super) const TILE_LAYER: f32 = 0.0;
pub(super) const CHAR_LAYER: f32 = 1.0;
const PROJECTILE_LAYER: f32 = 2.0;
const UI_LAYER: f32 = 3.0;

const NORMAL_BUTTON: Color = Color::srgb(0.15, 0.15, 0.15);
const HOVERED_BUTTON: Color = Color::srgb(0.25, 0.25, 0.25);
const PRESSED_BUTTON: Color = Color::srgb(0.35, 0.75, 0.35);
const VALID_MOVE: Color = Color::srgba(0.0, 0.5, 0.5, 0.5);
const INCOHERENT_UNIT: Color = Color::srgba(0.7, 0.0, 0.0, 0.5);

pub struct UIPlugin;

impl Plugin for UIPlugin {
    fn build(&self, app: &mut App) {
        app.add_event::<ActionEvent>();
        app.add_event::<SpawnProjectileEvent>();
        app.add_event::<CharacterSpawnEvent>();

        app.add_systems(Startup, (setup_camera, sync_sim, setup_tiles))
            // Only process actions if we're actually waiting for action input
            .add_systems(Update, action_system.run_if(in_state(PlayState::Waiting)))
            .add_systems(OnExit(PlayState::Processing), (sync_sim, game_over, ai));

        app.add_systems(Startup, setup_ui)
            .add_systems(
                Update,
                (
                    button_system,
                    action_button_action,
                    cursor_locator,
                    tile_highlight,
                    animate_sprite,
                    healthbars,
                    process_curves,
                    // _paint_curves,
                    spawn_projectile,
                    cleanup_projectiles,
                    spawn_character,
                    // update_character_animation,
                ),
            )
            .add_systems(
                Update,
                (
                    selection
                        .before(populate_action_buttons)
                        .before(highlight_moves),
                    populate_action_buttons,
                    highlight_moves,
                )
                    .run_if(input_just_pressed(MouseButton::Left)),
            )
            .add_systems(
                Update,
                handle_right_click.run_if(input_just_pressed(MouseButton::Right)),
            )
            .add_systems(
                OnEnter(PlayState::Waiting),
                (
                    populate_action_buttons,
                    highlight_moves,
                    highlight_incoherent_unit,
                ),
            );
    }
}

#[derive(Resource, Default)]
struct SelectedModel(SimId);

/// Track which character is currently up to go
#[derive(Resource, Default)]
pub struct CurrentCharacter(pub SimId);
#[derive(Event, Debug)]
pub struct ActionEvent {
    pub action: Action,
}

#[derive(Component)]
struct ActionButtonParent;

#[derive(Component)]
struct ActionButton(usize);

#[derive(Component)]
struct MovementHighlight;

#[allow(clippy::type_complexity)]
fn button_system(
    mut interaction_query: Query<
        (&Interaction, &mut BackgroundColor, &mut BorderColor),
        (Changed<Interaction>, With<Button>),
    >,
    // mut query_player: Query<Entity, With<Player>>,
) {
    for (interaction, mut color, mut border_color) in &mut interaction_query {
        match *interaction {
            Interaction::Pressed => {
                *color = PRESSED_BUTTON.into();
                border_color.0 = Color::BLACK;
            }
            Interaction::Hovered => {
                *color = HOVERED_BUTTON.into();
                border_color.0 = Color::WHITE;
            }
            Interaction::None => {
                *color = NORMAL_BUTTON.into();
                border_color.0 = Color::BLACK;
            }
        }
    }
}

fn setup_ui(mut commands: Commands) {
    commands.init_resource::<MouseWorldCoords>();
    commands.init_resource::<SelectedModel>();
    commands.init_resource::<CurrentCharacter>();

    // root node
    commands
        .spawn(Node {
            width: Val::Percent(100.0),
            height: Val::Percent(100.0),
            ..default()
        })
        .with_children(|parent| {
            // left vertical fill (content)
            parent
                .spawn((
                    Node {
                        width: Val::Percent(100.),
                        ..default()
                    },
                    BackgroundColor(Color::srgba(1.0, 1.0, 1.0, 0.0)),
                ))
                .with_children(|parent| {
                    // text
                    parent.spawn((
                        Text("Text Example".to_string()),
                        TextFont {
                            font_size: 30.0,
                            ..default()
                        },
                        // Because this is a distinct label widget and
                        // not button/list item text, this is necessary
                        // for accessibility to treat the text accordingly.
                        Label,
                    ));
                });

            // right vertical fill
            use bevy::color::palettes::css::*;
            parent
                .spawn((
                    Node {
                        flex_direction: FlexDirection::Column,
                        justify_content: JustifyContent::Start,
                        align_items: AlignItems::Start,
                        width: Val::Px(400.),
                        border: UiRect::all(Val::Px(2.)),
                        ..default()
                    },
                    BorderColor(GREEN.into()),
                    BackgroundColor(Color::srgb(0.15, 0.15, 0.15)),
                    ActionButtonParent,
                ))
                .with_children(|parent| {
                    // Title
                    parent.spawn((
                        Text("Right bar".to_string()),
                        TextFont {
                            font_size: 25.,
                            ..default()
                        },
                        Label,
                    ));
                });
        });
}

fn populate_action_buttons(
    mut commands: Commands,
    sim: Res<SimState>,
    selected: Res<SelectedModel>,
    mut query: Query<Entity, With<ActionButtonParent>>,
) {
    debug!("populating action buttons");

    let mut parent = commands.entity(query.single_mut());
    // need to both despawna and clear the children
    parent.despawn_descendants();
    parent.clear_children();

    parent.with_children(|parent| {
        let mut actions = Vec::new();
        sim.legal_actions(&mut actions);
        for (idx, action) in actions.into_iter().enumerate() {
            if matches!(action, Action::EndTurn) {
                spawn_action_button(parent, &action.to_string(), idx);
            } else if matches!(action, Action::RemoveModel { id } if id == selected.0) {
                spawn_action_button(parent, "Remove model", idx);
            }
        }
    });
}

fn spawn_action_button(parent: &mut ChildBuilder, text: &str, idx: usize) {
    parent
        .spawn((
            Button,
            Node {
                width: Val::Px(300.0),
                height: Val::Px(30.0),
                border: UiRect::all(Val::Px(5.0)),
                // horizontally center child text
                justify_content: JustifyContent::Center,
                // vertically center child text
                align_items: AlignItems::Center,
                ..default()
            },
            BackgroundColor(NORMAL_BUTTON),
            BorderColor(Color::BLACK),
            ActionButton(idx),
        ))
        .with_children(|parent| {
            parent.spawn((
                Text(text.to_string()),
                TextFont {
                    font: Default::default(),
                    font_size: 25.0,
                    ..default()
                },
                TextColor(Color::srgb(0.9, 0.9, 0.9)),
            ));
        });
}

/// We will store the world position of the mouse cursor here.
#[derive(Resource, Default)]
struct MouseWorldCoords(Vec2);

impl MouseWorldCoords {
    fn to_sim(&self) -> SimCoords {
        let offset = (TILE_SIZE / 2) as f32;
        SimCoords {
            x: (self.0.x + offset) as usize / TILE_SIZE,
            y: (self.0.y + offset) as usize / TILE_SIZE,
        }
    }
}

impl SimCoords {
    pub fn to_world(&self) -> Vec2 {
        vec2((self.x * TILE_SIZE) as f32, (self.y * TILE_SIZE) as f32)
    }
}

/// Stores the position of the mouse in terms of world coords
/// https://bevy-cheatbook.github.io/cookbook/cursor2world.html
fn cursor_locator(
    mut mycoords: ResMut<MouseWorldCoords>,
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
        .map(|cursor| camera.viewport_to_world(camera_transform, cursor).unwrap())
        .map(|ray| ray.origin.truncate())
    {
        mycoords.0 = world_position;
    }
}

/// Highlight the tile the mouse is hovering over
fn tile_highlight(mouse_coords: Res<MouseWorldCoords>, mut gizmos: Gizmos) {
    let offset = (TILE_SIZE / 2) as f32;
    let x = mouse_coords.0.x - (mouse_coords.0.x + offset) % TILE_SIZE as f32 + offset;
    let y = mouse_coords.0.y - (mouse_coords.0.y + offset) % TILE_SIZE as f32 + offset;
    gizmos.rect_2d(
        Isometry2d::from_translation(vec2(x, y)),
        Vec2::splat(TILE_SIZE as f32),
        Color::BLACK,
    );
}

fn selection(
    mouse_coords: Res<MouseWorldCoords>,
    sim: Res<SimState>,
    mut selected: ResMut<SelectedModel>,
) {
    // convert mouse coords to sim coords
    // get entity at the relevant sim position
    // set the resrouce to that entity, probably a different ID than the bevy entity, this is the Sim specific id
    // separate system to draw the selection highlight box whenever that is populated

    debug!(
        "attempting to select character at: {:?} for raw coords: {:?}",
        mouse_coords.to_sim(),
        mouse_coords.0
    );

    let Some(new_selection) = sim.get_id(mouse_coords.to_sim()) else {
        return;
    };

    selected.0 = new_selection;
    debug!("new character selected: {:?}", new_selection);
}

fn handle_right_click(
    mut ev_action: EventWriter<ActionEvent>,
    mouse_coords: Res<MouseWorldCoords>,
    selected: Res<SelectedModel>,
    sim: Res<SimState>,
) {
    debug!("handling right click");

    let mut legal_actions = Vec::new();
    sim.legal_actions(&mut legal_actions);
    let from = sim.get_loc(selected.0).unwrap();
    let action = Action::Move {
        to: mouse_coords.to_sim(),
        id: selected.0,
        from,
    };
    if legal_actions.contains(&action) {
        ev_action.send(ActionEvent { action });
    } else {
        warn!("trying to move to location that's not a legal action")
    }
}

/// Outline the tiles a character can validally move within
fn highlight_moves(
    mut commands: Commands,
    sim: Res<SimState>,
    selected: Res<SelectedModel>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
    old_highlights: Query<Entity, With<MovementHighlight>>,
) {
    // delete the old highlights
    old_highlights
        .iter()
        .for_each(|e| commands.entity(e).despawn());

    let mut actions = Vec::new();
    sim.legal_actions(&mut actions);

    let rect = Rectangle::new(TILE_SIZE as f32, TILE_SIZE as f32);
    for a in actions.iter() {
        if let Action::Move {
            id,
            from: _from,
            to,
        } = a
        {
            if id != &selected.0 {
                continue; // only show moves for the selected character
            }

            let wc = to.to_world();
            commands.spawn((
                Mesh2d(meshes.add(rect)),
                MeshMaterial2d(materials.add(VALID_MOVE)),
                Transform::from_xyz(wc.x, wc.y, UI_LAYER),
                StateScoped(PlayState::Waiting), // automatically unspawn when leave waiting
                MovementHighlight,
            ));
        }
    }
}

fn highlight_incoherent_unit(
    mut commands: Commands,
    sim: Res<SimState>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
) {
    debug!("running incoherence system");
    let rect = Rectangle::new(TILE_SIZE as f32, TILE_SIZE as f32);

    for incoherent_id in sim
        .unit_coherency()
        .iter()
        .filter(|(_, is_coherent)| !is_coherent)
        .map(|x| x.0)
    {
        debug!("incoherent unit found");
        let loc = sim.get_loc(incoherent_id).unwrap();
        let wc = loc.to_world();
        commands.spawn((
            Mesh2d(meshes.add(rect)),
            MeshMaterial2d(materials.add(INCOHERENT_UNIT)),
            Transform::from_xyz(wc.x, wc.y, UI_LAYER),
            StateScoped(PlayState::Waiting), // automatically unspawn when leave waiting
        ));
    }
}

#[allow(clippy::type_complexity)]
fn action_button_action(
    interaction_query: Query<(&Interaction, &ActionButton), (Changed<Interaction>, With<Button>)>,
    mut ev_action: EventWriter<ActionEvent>,
    sim: Res<SimState>,
) {
    let mut legal_actions = Vec::new();
    sim.legal_actions(&mut legal_actions);
    for (interaction, action_id) in &interaction_query {
        if *interaction == Interaction::Pressed {
            ev_action.send(ActionEvent {
                action: legal_actions[action_id.0],
            });
        }
    }
}
