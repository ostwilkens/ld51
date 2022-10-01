use bevy::{
    math::{vec2, vec3},
    prelude::*,
    time::FixedTimestep,
};
use bevy_web_fullscreen::FullViewportPlugin;

// defines
static PAUSE_TIME: f32 = 0.7;

// resources
struct HitSound(Handle<AudioSource>);

struct LastMousePosition(Vec2);

struct BallAssets {
    mesh: Handle<Mesh>,
    material: Handle<StandardMaterial>,
}

#[derive(Debug, Clone, Eq, PartialEq, Hash)]
enum AppState {
    InGame,
    HitPause,
}

// components
#[derive(Default)]
struct PauseTimer(f32);

#[derive(Component)]
struct Bat;

#[derive(Component)]
struct BatCollider(i32);

#[derive(Component, Default)]
struct Velocity(Vec3);

#[derive(Component, Default)]
struct Size(f32);

#[derive(Component, Default)]
struct GameTime(f32);

#[derive(PartialEq)]
enum BallStatus {
    Thrown,
    Hit,
}

#[derive(Component)]
struct Status(BallStatus);

#[derive(Component)]
struct HistoricVelocity {
    previous_pos: Vec3,
    decaying_vel: Vec3,
}

// bundles
#[derive(Bundle)]
struct BallBundle {
    pub mesh: Handle<Mesh>,
    pub material: Handle<StandardMaterial>,
    pub transform: Transform,
    pub global_transform: GlobalTransform,
    pub visibility: Visibility,
    pub computed_visibility: ComputedVisibility,
    pub velocity: Velocity,
    pub size: Size,
    pub status: Status,
}

impl Default for BallBundle {
    fn default() -> Self {
        Self {
            mesh: Default::default(),
            material: Default::default(),
            transform: Default::default(),
            global_transform: Default::default(),
            visibility: Default::default(),
            computed_visibility: Default::default(),
            velocity: Default::default(),
            size: Default::default(),
            status: Status(BallStatus::Thrown),
        }
    }
}

fn main() {
    let mut app = App::new();

    app.add_plugins(DefaultPlugins)
        .add_state(AppState::InGame)
        .insert_resource(ClearColor(Color::rgb(0.24, 0.44, 0.94)))
        .insert_resource(PauseTimer(0.0))
        .insert_resource(LastMousePosition(vec2(0.0, 0.0)))
        .add_startup_system(setup)
        .add_system_set(
            // throw ball every x seconds
            SystemSet::on_update(AppState::InGame)
                .with_run_criteria(FixedTimestep::step(1.0))
                .with_system(throw_ball),
        )
        .add_system_set(
            // physics should only run when not paused
            SystemSet::on_update(AppState::InGame)
                .with_system(physics)
                .with_system(update_bat_transform),
        )
        .add_system_set(
            // when pause is triggered
            SystemSet::on_enter(AppState::HitPause)
                .with_system(start_pause_timer)
                .with_system(play_hit_sound),
        )
        .add_system_set(
            // while in pause state
            SystemSet::on_update(AppState::HitPause)
                .with_system(update_pause_timer)
                .with_system(camera_shake),
        )
        .add_system_set(
            // easiest to have this framerate independent
            SystemSet::new()
                .with_run_criteria(FixedTimestep::step(1.0 / 60.0))
                .with_system(update_collider_historic_velocity),
        );

    #[cfg(target_family = "wasm")]
    app.add_plugin(FullViewportPlugin);

    app.run();
}

fn setup(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    asset_server: Res<AssetServer>,
) {
    // load hit sound
    let hit_sound: Handle<AudioSource> = asset_server.load("hit.ogg");
    commands.insert_resource(HitSound(hit_sound));

    // init ball assets
    let ball_assets = BallAssets {
        mesh: meshes.add(Mesh::from(shape::Icosphere {
            radius: 1.0,
            subdivisions: 4,
        })),
        material: materials.add(Color::WHITE.into()),
    };
    commands.insert_resource(ball_assets);

    // ground plane
    commands.spawn_bundle(PbrBundle {
        mesh: meshes.add(Mesh::from(shape::Plane { size: 10.0 })),
        material: materials.add(Color::GREEN.into()),
        ..default()
    });

    // light
    commands.spawn_bundle(PointLightBundle {
        point_light: PointLight {
            intensity: 1500.0,
            shadows_enabled: true,
            ..default()
        },
        transform: Transform::from_xyz(4.0, 8.0, 4.0),
        ..default()
    });

    // spawn player
    commands
        .spawn_bundle(SpatialBundle {
            transform: Transform::from_xyz(5.0, 1.0, 5.0).looking_at(Vec3::ZERO, Vec3::Y),
            ..default()
        })
        .with_children(|parent| {
            // camera
            parent.spawn_bundle(Camera3dBundle { ..default() });

            // bat
            parent
                .spawn_bundle((
                    Bat,
                    Transform::from_xyz(0.0, 0.0, -1.0),
                    Visibility::default(),
                    ComputedVisibility::default(),
                    GlobalTransform::default(),
                ))
                .with_children(|parent| {
                    // bat visual
                    parent
                        .spawn_bundle(PbrBundle {
                            mesh: meshes.add(Mesh::from(shape::Capsule {
                                radius: 0.1,
                                rings: 4,
                                depth: 1.0,
                                latitudes: 4,
                                longitudes: 4,
                                ..default()
                            })),
                            material: materials.add(Color::WHITE.into()),
                            transform: Transform::from_xyz(0.0, 0.8, 0.0),
                            ..default()
                        })
                        .with_children(|parent| {
                            // bat collision points
                            for i in 0..7 {
                                parent
                                    .spawn_bundle(PbrBundle {
                                        mesh: meshes.add(Mesh::from(shape::Icosphere {
                                            radius: 0.12,
                                            subdivisions: 1,
                                        })),
                                        material: materials.add(Color::PURPLE.into()),
                                        transform: Transform::from_xyz(
                                            0.0,
                                            i as f32 * 0.15 - 0.4,
                                            0.0,
                                        ),
                                        visibility: Visibility { is_visible: false },
                                        ..default()
                                    })
                                    .insert(BatCollider(i))
                                    .insert(HistoricVelocity {
                                        previous_pos: vec3(0.0, 0.0, 0.0),
                                        decaying_vel: vec3(0.0, 0.0, 0.0),
                                    });
                            }
                        });
                });
        });
}

fn update_pause_timer(
    time: Res<Time>,
    mut pause_timer: ResMut<PauseTimer>,
    mut state: ResMut<State<AppState>>,
) {
    pause_timer.0 -= time.delta_seconds();

    if pause_timer.0 < 0.0 {
        state.set(AppState::InGame).unwrap();
    }
}

fn start_pause_timer(mut pause_timer: ResMut<PauseTimer>) {
    pause_timer.0 = PAUSE_TIME;
}

fn play_hit_sound(audio: Res<Audio>, hit_sound: Res<HitSound>) {
    audio.play(hit_sound.0.clone_weak());
}

fn camera_shake(pause_timer: Res<PauseTimer>, mut q: Query<&mut Transform, With<Camera>>) {
    let mut camera_transform = q.single_mut();
    let pause_progress = 1.0 - (PAUSE_TIME - pause_timer.0) / PAUSE_TIME;
    let shake_amount = (pause_progress - 0.0).max(0.0) * 0.5;

    camera_transform.translation.x = (rand::random::<f32>() - 0.5) * shake_amount;
    camera_transform.translation.x = (rand::random::<f32>() - 0.5) * shake_amount;
}

fn physics(
    mut app_state: ResMut<State<AppState>>,
    time: Res<Time>,
    mut q_balls: Query<(&mut Transform, &mut Velocity, &Size, &mut Status)>,
    q_colliders: Query<(&GlobalTransform, &BatCollider, &HistoricVelocity)>,
) {
    for (mut transform, mut velocity, size, mut status) in q_balls.iter_mut() {
        // apply gravity
        velocity.0.y -= time.delta_seconds() * 2.0;

        let mut new_translation = transform.translation + velocity.0 * time.delta_seconds();

        // snap & bounce on ground
        if new_translation.y < size.0 {
            new_translation.y = size.0;
            velocity.0.y = -velocity.0.y;
            velocity.0 *= 0.7;
        }

        // bat collision
        if status.0 == BallStatus::Thrown {
            for (global_transform, _bat_collider, historical_vel) in q_colliders.iter() {
                let collider_pos = global_transform.translation();
                let ball_pos = transform.translation;

                if ball_pos.distance(collider_pos) < size.0 + 0.15 {
                    status.0 = BallStatus::Hit;
                    let hit_power = historical_vel.decaying_vel.length();

                    // bounce back based on hit_power
                    let mut new_velocity = -velocity.0 * hit_power * 4.0;

                    // affected by bat vector
                    new_velocity += historical_vel.decaying_vel * 15.0;

                    new_velocity.y *= 0.5;

                    if hit_power > 0.3 {
                        new_velocity *= 1.2;

                        app_state.set(AppState::HitPause).unwrap();
                    }

                    velocity.0 = new_velocity;

                    break;
                }
            }
        }

        // apply velocity
        transform.translation = new_translation;
    }
}

fn throw_ball(mut commands: Commands, ball_assets: Res<BallAssets>) {
    let radius = 0.05;
    commands.spawn_bundle(BallBundle {
        mesh: ball_assets.mesh.clone_weak(),
        material: ball_assets.material.clone_weak(),
        transform: Transform::from_translation(vec3(-2.5, 0.5, -2.5))
            .with_scale(Vec3::splat(radius)),
        size: Size(radius),
        velocity: Velocity(vec3(5.03, 1.82, 5.0)),
        ..default()
    });
}

fn update_collider_historic_velocity(
    mut q: Query<(&BatCollider, &GlobalTransform, &mut HistoricVelocity)>,
) {
    for (_collider, global_transform, mut historical_velocity) in q.iter_mut() {
        let new_pos = global_transform.translation();
        let diff = new_pos - historical_velocity.previous_pos;
        historical_velocity.previous_pos = new_pos;

        // increase by diff
        historical_velocity.decaying_vel += diff;

        // decay
        historical_velocity.decaying_vel *= 0.7;
    }
}

fn update_bat_transform(
    time: Res<Time>,
    mut q_bat: Query<&mut Transform, With<Bat>>,
    windows: Res<Windows>,
    mut last_mouse_position: ResMut<LastMousePosition>,
) {
    let window = windows.get_primary().unwrap();
    let mut bat_transform = q_bat.single_mut();

    let cursor_position = match window.cursor_position() {
        Some(position) => {
            last_mouse_position.0 = position;
            position
        }
        None => last_mouse_position.0,
    };

    // virtual joystick
    let aim_x = cursor_position.x / window.width() - 0.5;
    let aim_y = cursor_position.y / window.height() - 0.5;

    let new_y = aim_y - 0.2;
    let new_rotation = Quat::from_euler(EulerRot::XYZ, -0.6, 0.1, -0.7)
        * Quat::from_euler(EulerRot::XYZ, 0.0, 0.0, -aim_x * 2.2 + 0.5);

    let n = time.delta_seconds() * 40.0;

    // smooth transition to new values
    bat_transform.translation.y = bat_transform.translation.y * (1.0 - n) + new_y * n;
    bat_transform.rotation = bat_transform.rotation * (1.0 - n) + new_rotation * n;
}
