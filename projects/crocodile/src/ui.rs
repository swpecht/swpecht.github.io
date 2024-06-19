use bevy::{math::vec2, prelude::*, window::PrimaryWindow};

use crate::{
    gamestate::Action,
    sprite::{MainCamera, TILE_SIZE},
};

const NORMAL_BUTTON: Color = Color::rgb(0.15, 0.15, 0.15);
const HOVERED_BUTTON: Color = Color::rgb(0.25, 0.25, 0.25);
const PRESSED_BUTTON: Color = Color::rgb(0.35, 0.75, 0.35);

pub struct UIPlugin;

impl Plugin for UIPlugin {
    fn build(&self, app: &mut App) {
        app.add_event::<ActionEvent>();

        app.add_systems(Startup, setup_ui)
            .add_systems(Update, (button_system, cursor_locator, tile_highlight));
    }
}

#[derive(Resource)]
struct SelectedCharacter(Entity);

#[derive(Event, Debug)]
pub struct ActionEvent {
    pub entity: Entity,
    pub action: Action,
}

fn button_system(
    mut ev_action: EventWriter<ActionEvent>,
    selected: Option<Res<SelectedCharacter>>,
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
                border_color.0 = Color::RED;
                warn!("button clicked");

                if let Some(selected) = &selected {
                    ev_action.send(ActionEvent {
                        entity: selected.0,
                        action: Action::MoveRight,
                    });
                }
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
                    background_color: Color::rgba(1.0, 1.0, 1.0, 0.0).into(),
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
            parent
                .spawn(NodeBundle {
                    style: Style {
                        flex_direction: FlexDirection::Column,
                        justify_content: JustifyContent::Start,
                        align_items: AlignItems::Start,
                        width: Val::Px(200.),
                        border: UiRect::all(Val::Px(2.)),
                        ..default()
                    },
                    border_color: Color::GREEN.into(),
                    background_color: Color::rgb(0.15, 0.15, 0.15).into(),
                    ..default()
                })
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
                })
                .with_children(|parent| {
                    parent
                        .spawn(ButtonBundle {
                            style: Style {
                                width: Val::Px(150.0),
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
                        })
                        .with_children(|parent| {
                            parent.spawn(TextBundle::from_section(
                                "Button",
                                TextStyle {
                                    font: Default::default(),
                                    font_size: 25.0,
                                    color: Color::rgb(0.9, 0.9, 0.9),
                                },
                            ));
                        });
                });
        });
}

/// We will store the world position of the mouse cursor here.
#[derive(Resource, Default)]
struct MouseWorldCoords(Vec2);

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
fn tile_highlight(mycoords: Res<MouseWorldCoords>, mut gizmos: Gizmos) {
    let offset = (TILE_SIZE / 2) as f32;
    let x = mycoords.0.x - (mycoords.0.x + offset) % TILE_SIZE as f32 + offset;
    let y = mycoords.0.y - (mycoords.0.y + offset) % TILE_SIZE as f32 + offset;
    // TODO: fix the alignment
    gizmos.rect_2d(vec2(x, y), 0.0, Vec2::splat(TILE_SIZE as f32), Color::BLACK);
}
