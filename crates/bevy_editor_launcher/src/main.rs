//! The launcher for the Bevy Editor.
//!
//! The launcher provide a bunch of functionalities to manage your projects.

use std::path::PathBuf;

use bevy::{
    prelude::*,
    tasks::{block_on, futures_lite::future, IoTaskPool, Task},
    time::{Timer, TimerMode},
};

use bevy_editor::project::{
    create_new_project, get_local_projects, set_project_list, templates::Templates, ProjectInfo,
};
use bevy_editor_styles::{StylesPlugin, Theme};
use bevy_footer_bar::{FooterBarPlugin, FooterBarSet};
use bevy_scroll_box::ScrollBoxPlugin;
use ui::ProjectList;

mod ui;

/// The Task that creates a new project
#[derive(Component)]
struct CreateProjectTask(Task<std::io::Result<ProjectInfo>>);

/// Component to mark the loading window
#[derive(Component)]
struct LoadingWindow;

/// Resource to store the log messages
#[derive(Resource, Default, Clone)]
struct ProjectCreationLogs(Vec<String>);

/// Resource to track when to close the log window
#[derive(Resource)]
struct ProjectCreationLogTimer {
    timer: Timer,
    entity: Entity,
}

/// A utils to run a system only if the [`CreateProjectTask`] is running
fn run_if_task_is_running(task_query: Query<Entity, With<CreateProjectTask>>) -> bool {
    task_query.iter().count() > 0
}

/// Spawn the loading window
fn spawn_loading_window(mut commands: Commands, theme: Res<Theme>, logs: Res<ProjectCreationLogs>) {
    let window_entity = commands
        .spawn((
            Node {
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                position_type: PositionType::Absolute,
                display: Display::Flex,
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                ..default()
            },
            BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.5)),
            LoadingWindow,
        ))
        .id();

    commands.entity(window_entity).with_children(|parent| {
        // Content container
        parent
            .spawn((
                Node {
                    width: Val::Px(500.0),
                    height: Val::Px(400.0),
                    flex_direction: FlexDirection::Column,
                    align_items: AlignItems::Center,
                    justify_content: JustifyContent::FlexStart,
                    padding: UiRect::all(Val::Px(20.0)),
                    ..default()
                },
                theme.pane.area_background_color,
                BorderRadius::all(Val::Px(10.0)),
            ))
            .with_children(|parent| {
                // Title
                parent.spawn((
                    Text::new("Creating new project..."),
                    TextFont {
                        font: theme.text.font.clone(),
                        font_size: 24.0,
                        ..default()
                    },
                ));

                // Log area
                parent
                    .spawn((
                        Node {
                            width: Val::Percent(100.0),
                            height: Val::Px(300.0),
                            margin: UiRect::top(Val::Px(20.0)),
                            padding: UiRect::all(Val::Px(10.0)),
                            flex_direction: FlexDirection::Column,
                            overflow: Overflow::clip(),
                            ..default()
                        },
                        BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.2)),
                        BorderRadius::all(Val::Px(5.0)),
                        ProjectLogContent,
                    ))
                    .with_children(|parent| {
                        // Add log entries
                        for log in logs.0.iter() {
                            parent.spawn((
                                Text::new(log.clone()),
                                TextFont {
                                    font: theme.text.font.clone(),
                                    font_size: 14.0,
                                    ..default()
                                },
                            ));
                        }
                    });
            });
    });
}

/// Component to mark the project log content
#[derive(Component)]
struct ProjectLogContent;

/// Update the project creation logs
fn update_project_logs(
    mut commands: Commands,
    logs: Res<ProjectCreationLogs>,
    log_content_query: Query<Entity, With<ProjectLogContent>>,
    theme: Res<Theme>,
) {
    for log_content_entity in log_content_query.iter() {
        // First, completely despawn the log content entity
        commands.entity(log_content_entity).despawn();

        // Create a new one in its place
        commands
            .spawn((
                Node {
                    width: Val::Percent(100.0),
                    height: Val::Px(300.0),
                    flex_direction: FlexDirection::Column,
                    overflow: Overflow::clip(),
                    margin: UiRect::top(Val::Px(20.0)),
                    padding: UiRect::all(Val::Px(10.0)),
                    ..default()
                },
                BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.2)),
                BorderRadius::all(Val::Px(5.0)),
                ProjectLogContent,
            ))
            .with_children(|parent| {
                for log in logs.0.iter() {
                    parent.spawn((
                        Text::new(log.clone()),
                        TextFont {
                            font: theme.text.font.clone(),
                            font_size: 14.0,
                            ..default()
                        },
                    ));
                }
            });
    }
}

/// Check on the status of the [`CreateProjectTask`] and handle the result when done
fn poll_create_project_task(
    mut commands: Commands,
    mut task_query: Query<(Entity, &mut CreateProjectTask)>,
    query: Query<(Entity, &Children), With<ProjectList>>,
    theme: Res<Theme>,
    asset_server: Res<AssetServer>,
    mut project_list: ResMut<ProjectInfoList>,
    mut logs: ResMut<ProjectCreationLogs>,
) {
    let (task_entity, mut task) = task_query.single_mut();
    if let Some(result) = block_on(future::poll_once(&mut task.0)) {
        match result {
            Ok(project_info) => {
                // Add a log message
                logs.0.push(format!(
                    "Successfully created new project at: {:?}",
                    project_info.path
                ));
                info!(
                    "Successfully created new project at: {:?}",
                    project_info.path
                );

                // Add the new project to the list of projects
                project_list.0.push(project_info.clone());
                set_project_list(project_list.0.clone());

                // Add new project node Ui element
                commands.entity(task_entity).despawn();
                let (project_list_entity, children) = query.iter().next().unwrap();
                let plus_button_entity = children.last().unwrap();

                commands.entity(*plus_button_entity).remove::<ChildOf>();
                commands
                    .entity(project_list_entity)
                    .with_children(|builder| {
                        ui::spawn_project_node(builder, &theme, &asset_server, &project_info);
                    });
                commands
                    .entity(*plus_button_entity)
                    .insert(ChildOf(project_list_entity));
            }
            Err(error) => {
                // Add a log message
                logs.0
                    .push(format!("Failed to create new project: {:?}", error));
                error!("Failed to create new project: {:?}", error);

                commands.entity(task_entity).despawn();
            }
        };

        // Show the logs for a short time before closing
        let timer_entity = commands.spawn_empty().id();
        commands.insert_resource(ProjectCreationLogTimer {
            timer: Timer::from_seconds(5.0, TimerMode::Once),
            entity: timer_entity,
        });
    }
}

/// System to handle closing the log window after timer expires
fn handle_log_timer(
    mut commands: Commands,
    time: Res<Time>,
    timer_res: Option<ResMut<ProjectCreationLogTimer>>,
    loading_window_query: Query<Entity, With<LoadingWindow>>,
) {
    if let Some(mut timer) = timer_res {
        timer.timer.tick(time.delta());
        if timer.timer.finished() {
            // Close the log window
            for entity in loading_window_query.iter() {
                {
                    let this = &mut commands.entity(entity);
                    this.despawn();
                };
            }

            // Remove the timer
            commands.entity(timer.entity).despawn();
            commands.remove_resource::<ProjectCreationLogTimer>();
        }
    }
}

/// Spawn a new [`CreateProjectTask`] to create a new project
fn spawn_create_new_project_task(commands: &mut Commands, template: Templates, path: PathBuf) {
    info!("Starting to create new project at: {:?}", path);
    let task = IoTaskPool::get().spawn(async move { create_new_project(template, path).await });
    commands.spawn_empty().insert(CreateProjectTask(task));
}

#[derive(Resource)]
struct ProjectInfoList(Vec<ProjectInfo>);

fn main() {
    App::new()
        .add_plugins((
            DefaultPlugins.set(WindowPlugin {
                primary_window: Some(Window {
                    title: "Bevy Editor Launcher".to_string(),
                    ..default()
                }),
                ..default()
            }),
            StylesPlugin,
            FooterBarPlugin,
            ScrollBoxPlugin,
        ))
        .insert_resource(ProjectInfoList(get_local_projects()))
        .insert_resource(ProjectCreationLogs::default())
        .add_systems(Startup, ui::setup)
        .add_systems(
            Update,
            (
                poll_create_project_task.run_if(run_if_task_is_running),
                spawn_loading_window.run_if(run_if_task_is_running),
                update_project_logs.run_if(run_if_task_is_running),
                handle_log_timer,
            ),
        )
        .configure_sets(Startup, FooterBarSet.after(ui::setup))
        .run();
}
