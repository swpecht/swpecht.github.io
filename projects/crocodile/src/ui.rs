use bevy::{math::vec2, prelude::*};

use crate::gamestate::Action;

const NORMAL_BUTTON: Color = Color::rgb(0.15, 0.15, 0.15);
const HOVERED_BUTTON: Color = Color::rgb(0.25, 0.25, 0.25);
const PRESSED_BUTTON: Color = Color::rgb(0.35, 0.75, 0.35);

pub struct UIPlugin;

impl Plugin for UIPlugin {
    fn build(&self, app: &mut App) {
        app.add_event::<ActionEvent>();

        app.add_systems(Startup, setup_ui)
            .add_systems(Update, (button_system));
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
