use bevy::app::Update;
use bevy::color::palettes::css::RED;
use bevy::prelude::*;

use crate::{sim_wrapper::SimStateResource, PlayState, NORMAL_BUTTON};

use super::SelectedModel;
pub(super) struct LeftPanelPlugin;

impl bevy::app::Plugin for LeftPanelPlugin {
    fn build(&self, app: &mut bevy::prelude::App) {
        app.add_systems(Update, (undo_button_click, populate_character_info));
    }
}

#[derive(Component)]
struct CharacterInfoParent;

#[derive(Component)]
#[require(Button)]
struct UndoButton;

pub(super) fn setup_left_panel(parent: &mut ChildBuilder) {
    parent
        .spawn((
            Node {
                width: Val::Px(400.),
                flex_direction: FlexDirection::Column,
                ..default()
            },
            BackgroundColor(Color::srgba(1.0, 1.0, 1.0, 0.0)),
        ))
        .with_children(|parent| {
            parent.spawn((
                CharacterInfoParent,
                Node {
                    flex_direction: FlexDirection::Column,
                    ..default()
                },
            ));

            // push things to bottom
            parent.spawn((
                Node {
                    height: Val::Percent(100.0),

                    ..default()
                },
                BorderColor(RED.into()),
            ));
            parent
                .spawn((
                    UndoButton,
                    Node {
                        border: UiRect::all(Val::Px(5.0)),
                        // horizontally center child text
                        justify_content: JustifyContent::Center,
                        // vertically center child text
                        align_items: AlignItems::Center,
                        ..default()
                    },
                    BackgroundColor(NORMAL_BUTTON),
                    BorderColor(Color::BLACK),
                ))
                .with_children(|parent| {
                    parent.spawn((
                        Text("Undo".to_string()),
                        TextFont {
                            font: Default::default(),
                            font_size: 25.0,
                            ..default()
                        },
                        TextColor(Color::srgb(0.9, 0.9, 0.9)),
                    ));
                });
        });
}

fn undo_button_click(
    interaction_query: Query<&Interaction, (Changed<Interaction>, With<UndoButton>)>,
    mut next_state: ResMut<NextState<PlayState>>,
    mut sim: ResMut<SimStateResource>,
) {
    for interaction in &interaction_query {
        if *interaction == Interaction::Pressed {
            debug!("undoing last action");
            sim.0.undo();
            next_state.set(PlayState::Processing);
        }
    }
}

fn populate_character_info(
    mut commands: Commands,
    selected_model: Res<SelectedModel>,
    mut query_action_info_parent: Query<Entity, With<CharacterInfoParent>>,
    sim: Res<SimStateResource>,
) {
    if !selected_model.is_changed() {
        return;
    }

    let mut parent = commands.entity(query_action_info_parent.single_mut());
    parent.despawn_descendants();
    parent.clear_children();
    let stats = sim.0.stats(&selected_model.0);
    parent.with_children(|parent| {
        parent.spawn(Text::new("Model stats:"));
        parent.spawn(Text::new(format!("M: {}", stats.movement)));
        parent.spawn(Text::new(format!("W: {}", stats.wound)));
        parent.spawn(Text::new(format!("T: {}", stats.toughness)));
        parent.spawn(Text::new(format!("S: {}", stats.save)));
    });
}
