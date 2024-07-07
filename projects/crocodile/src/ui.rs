use bevy::{input::common_conditions::*, math::vec2, prelude::*, window::PrimaryWindow};

use crate::{
    gamestate::{Action, SimCoords, SimId, SimState},
    sprite::{MainCamera, TILE_SIZE},
    PlayState,
};

const NORMAL_BUTTON: Color = Color::srgb(0.15, 0.15, 0.15);
const HOVERED_BUTTON: Color = Color::srgb(0.25, 0.25, 0.25);
const PRESSED_BUTTON: Color = Color::srgb(0.35, 0.75, 0.35);

pub struct UIPlugin;

impl Plugin for UIPlugin {
    fn build(&self, app: &mut App) {
        app.add_event::<ActionEvent>();

        app.add_systems(Startup, setup_ui)
            .add_systems(
                Update,
                (
                    button_system,
                    action_button_action,
                    cursor_locator,
                    tile_highlight,
                ),
            )
            .add_systems(
                Update,
                selection.run_if(input_just_pressed(MouseButton::Left)),
            )
            .add_systems(
                Update,
                handle_right_click.run_if(input_just_pressed(MouseButton::Right)),
            )
            .add_systems(OnEnter(PlayState::Waiting), populate_action_buttons);
    }
}

#[derive(Resource, Default)]
struct SelectedCharacter(SimId);

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
    commands.init_resource::<SelectedCharacter>();
    commands.init_resource::<CurrentCharacter>();

    // root node
    commands
        .spawn(NodeBundle {
            style: Style {
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                justify_content: JustifyContent::SpaceBetween,
                ..default()
            },
            ..default()
        })
        .with_children(|parent| {
            // left vertical fill (content)
            parent
                .spawn(NodeBundle {
                    style: Style {
                        width: Val::Percent(100.),
                        ..default()
                    },
                    background_color: Color::srgba(1.0, 1.0, 1.0, 0.0).into(),
                    ..default()
                })
                .with_children(|parent| {
                    // text
                    parent.spawn((
                        TextBundle::from_section(
                            "Text Example",
                            TextStyle {
                                font_size: 30.0,
                                ..default()
                            },
                        )
                        .with_style(Style {
                            margin: UiRect::all(Val::Px(5.)),
                            ..default()
                        }),
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
                    NodeBundle {
                        style: Style {
                            flex_direction: FlexDirection::Column,
                            justify_content: JustifyContent::Start,
                            align_items: AlignItems::Start,
                            width: Val::Px(400.),
                            border: UiRect::all(Val::Px(2.)),
                            ..default()
                        },
                        border_color: GREEN.into(),
                        background_color: Color::srgb(0.15, 0.15, 0.15).into(),
                        ..default()
                    },
                    ActionButtonParent,
                ))
                .with_children(|parent| {
                    // Title
                    parent.spawn((
                        TextBundle::from_section(
                            "Right bar",
                            TextStyle {
                                font_size: 25.,
                                ..default()
                            },
                        ),
                        Label,
                    ));
                });
        });
}

fn populate_action_buttons(
    mut commands: Commands,
    sim: Res<SimState>,
    mut query: Query<Entity, With<ActionButtonParent>>,
) {
    let mut parent = commands.entity(query.single_mut());
    // need to both despawna and clear the children
    parent.despawn_descendants();
    parent.clear_children();

    parent.with_children(|parent| {
        let mut actions = Vec::new();
        sim.legal_actions(&mut actions);
        debug!("{:?}", actions);
        for (idx, action) in actions.into_iter().enumerate() {
            spawn_action_button(parent, &action.to_string(), idx);
        }
    });
}

fn spawn_action_button(parent: &mut ChildBuilder, text: &str, idx: usize) {
    parent
        .spawn((
            ButtonBundle {
                style: Style {
                    width: Val::Px(300.0),
                    height: Val::Px(30.0),
                    border: UiRect::all(Val::Px(5.0)),
                    // horizontally center child text
                    justify_content: JustifyContent::Center,
                    // vertically center child text
                    align_items: AlignItems::Center,
                    ..default()
                },
                border_color: BorderColor(Color::BLACK),
                background_color: NORMAL_BUTTON.into(),
                ..default()
            },
            ActionButton(idx),
        ))
        .with_children(|parent| {
            parent.spawn(TextBundle::from_section(
                text,
                TextStyle {
                    font: Default::default(),
                    font_size: 25.0,
                    color: Color::srgb(0.9, 0.9, 0.9),
                },
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
        .and_then(|cursor| camera.viewport_to_world(camera_transform, cursor))
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
    gizmos.rect_2d(vec2(x, y), 0.0, Vec2::splat(TILE_SIZE as f32), Color::BLACK);
}

fn selection(
    mouse_coords: Res<MouseWorldCoords>,
    sim: Res<SimState>,
    mut selected: ResMut<SelectedCharacter>,
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
    sim: Res<SimState>,
) {
    debug!("handling right click");

    let target = mouse_coords.to_sim();

    let mut legal_actions = Vec::new();
    sim.legal_actions(&mut legal_actions);
    let action = Action::Move { target };
    if legal_actions.contains(&action) {
        ev_action.send(ActionEvent { action });
    } else {
        warn!("trying to move to location that's not a legal action")
    }
}

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
