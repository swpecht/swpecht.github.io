use bevy::prelude::*;
use simulation::gamestate::Action;

use crate::{
    sim_wrapper::SimStateResource, PlayState, INCOHERENT_UNIT, NORMAL_BUTTON, TILE_SIZE, UI_LAYER,
};

use super::{selection, to_world, ActionEvent, SelectedModel};

pub(super) struct RightPanelPlugin;

impl bevy::app::Plugin for RightPanelPlugin {
    fn build(&self, app: &mut bevy::prelude::App) {
        app.add_systems(
            Update,
            (
                update_team_tracker,
                action_button_click,
                action_button_hover,
                populate_action_buttons,
            ),
        )
        .add_systems(
            OnEnter(PlayState::Waiting),
            populate_action_buttons.after(selection),
        );
    }
}

#[derive(Component)]
struct TeamTracker;

#[derive(Component)]
struct ActionButtonParent;

#[derive(Component)]
struct ActionInfoParent;

#[derive(Component)]
struct ActionButton(Action);

pub(super) fn setup_right_panel(parent: &mut ChildBuilder) {
    use bevy::color::palettes::css::*;
    parent
        .spawn((
            Node {
                flex_direction: FlexDirection::Column,
                width: Val::Px(400.),
                border: UiRect::all(Val::Px(2.)),
                ..default()
            },
            BorderColor(GREEN.into()),
        ))
        .with_children(|parent| {
            parent.spawn((
                Text("Team: Players".to_string()),
                TextFont {
                    font: Default::default(),
                    font_size: 25.0,
                    ..default()
                },
                TextColor(Color::srgb(0.9, 0.9, 0.9)),
                TeamTracker,
            ));

            // Action button area
            parent.spawn((
                ActionButtonParent,
                Node {
                    flex_direction: FlexDirection::Column,
                    justify_content: JustifyContent::Start,
                    align_items: AlignItems::Start,
                    ..default()
                },
                BackgroundColor(Color::srgb(0.15, 0.15, 0.15)),
            ));

            // Action information Area
            parent.spawn((
                ActionInfoParent,
                Node {
                    flex_direction: FlexDirection::Column,
                    justify_content: JustifyContent::Start,
                    align_items: AlignItems::Start,
                    ..default()
                },
                BackgroundColor(Color::srgb(0.15, 0.15, 0.15)),
            ));
        });
}

fn populate_action_buttons(
    mut commands: Commands,
    sim: Res<SimStateResource>,
    selected: Res<SelectedModel>,
    mut query: Query<Entity, With<ActionButtonParent>>,
) {
    if !selected.is_changed() && !sim.is_changed() {
        return;
    }

    debug!("populating action buttons");

    let mut parent = commands.entity(query.single_mut());
    // need to both despawna and clear the children
    parent.despawn_descendants();
    parent.clear_children();

    parent.with_children(|parent| {
        let mut actions = Vec::new();
        sim.0.legal_actions(&mut actions);
        for action in actions.into_iter() {
            match action {
                Action::EndPhase => {
                    spawn_action_button(parent, &format!("End {}", sim.0.phase()), action)
                }
                Action::UseWeapon {
                    from,
                    to: _,
                    weapon: ranged_weapon,
                } if sim.0.get_model_unit(selected.0) == from => {
                    spawn_action_button(parent, &format!("{}", ranged_weapon), action);
                }
                Action::RemoveModel { id } if id == selected.0 => {
                    spawn_action_button(parent, "Remove model", action)
                }
                _ => {}
            }
        }
    });
}

fn spawn_action_button(parent: &mut ChildBuilder, text: &str, action: Action) {
    parent
        .spawn((
            Button,
            Node {
                width: Val::Percent(100.),
                // width: Val::Px(300.0),
                // height: Val::Px(30.0),
                border: UiRect::all(Val::Px(5.0)),
                // horizontally center child text
                justify_content: JustifyContent::Center,
                // vertically center child text
                align_items: AlignItems::Center,
                ..default()
            },
            BackgroundColor(NORMAL_BUTTON),
            BorderColor(Color::BLACK),
            ActionButton(action),
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

fn update_team_tracker(mut query: Query<&mut Text, With<TeamTracker>>, sim: Res<SimStateResource>) {
    for mut text in query.iter_mut() {
        text.0 = format!("Team: {}", sim.0.cur_team());
    }
}

#[derive(Component)]
struct ActionButtonHoverHighlight {}

#[allow(clippy::type_complexity)]
fn action_button_hover(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
    previous_hovers: Query<Entity, With<ActionButtonHoverHighlight>>,
    interaction_query: Query<(&Interaction, &ActionButton), (Changed<Interaction>, With<Button>)>,
    mut query_action_info_parent: Query<Entity, With<ActionInfoParent>>,
    sim: Res<SimStateResource>,
) {
    //todo: switch to using an index system to show the weapsons stats. Will fix the stuttering

    for (interaction, action) in &interaction_query {
        match interaction {
            Interaction::Hovered => {
                if let Action::UseWeapon {
                    from: _,
                    to,
                    weapon,
                } = action.0
                {
                    for (_, loc, _) in sim.0.unit_sprites(to) {
                        let rect = Rectangle::new(TILE_SIZE as f32, TILE_SIZE as f32);
                        let wc = to_world(&loc);
                        commands.spawn((
                            Mesh2d(meshes.add(rect)),
                            MeshMaterial2d(materials.add(INCOHERENT_UNIT)),
                            Transform::from_xyz(wc.x, wc.y, UI_LAYER),
                            ActionButtonHoverHighlight {},
                        ));

                        let mut parent = commands.entity(query_action_info_parent.single_mut());
                        parent.despawn_descendants();
                        parent.clear_children();
                        parent.with_children(|parent| {
                            parent.spawn(Text::new(format!("{}", weapon)));
                            let stats = weapon.stats();
                            parent.spawn(Text::new(format!("R: {}", stats.range)));
                            parent.spawn(Text::new(format!("A: {}", stats.num_attacks)));
                            parent.spawn(Text::new(format!("WS: {}", stats.skill)));
                            parent.spawn(Text::new(format!("S: {}", stats.strength)));
                            parent.spawn(Text::new(format!("AP: {}", stats.armor_penetration)));
                            parent.spawn(Text::new(format!("D: {}", stats.damage)));
                        });
                    }
                }
            }
            Interaction::None => {
                for entity in &previous_hovers {
                    commands.entity(entity).despawn();

                    // need to both despawna and clear the children
                    let mut parent = commands.entity(query_action_info_parent.single_mut());
                    parent.despawn_descendants();
                    parent.clear_children();
                }
            }
            _ => {}
        }
    }
}

#[allow(clippy::type_complexity)]
fn action_button_click(
    interaction_query: Query<(&Interaction, &ActionButton), Changed<Interaction>>,
    mut ev_action: EventWriter<ActionEvent>,
    sim: Res<SimStateResource>,
) {
    let mut legal_actions = Vec::new();
    sim.0.legal_actions(&mut legal_actions);

    for (interaction, action) in &interaction_query {
        if *interaction == Interaction::Pressed {
            let a = action.0;
            assert!(
                legal_actions.contains(&a),
                "Attempting to play an illegal action"
            );
            ev_action.send(ActionEvent { action: a });
        }
    }
}
