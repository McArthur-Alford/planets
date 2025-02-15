// mod colors;
mod fibonacci_sphere;
mod fibonacci_sphere_visualiser;
mod flatnormal;
mod geometry_data;
// mod goldberg;
mod helpers;
// mod icosahedron;
// mod surface;

use bevy::{
    color::palettes::css::GREEN,
    pbr::wireframe::{Wireframe, WireframeConfig, WireframePlugin},
    prelude::*,
    render::{
        settings::{RenderCreation, WgpuFeatures, WgpuSettings},
        RenderPlugin,
    },
    time::common_conditions::on_timer,
};
use bevy_fps_counter::FpsCounterPlugin;
use bevy_panorbit_camera::{PanOrbitCamera, PanOrbitCameraPlugin};
// use colors::{randomize_colors, update_mesh_colors};
use flatnormal::FlatNormalMaterialPlugin;
use geometry_data::setup_demo_sphere;
// use goldberg::setup_hex;
use std::time::Duration;
// use surface::{chunk_to_mesh, orderless_chunker};

fn setup(mut commands: Commands) {
    commands.spawn((
        Transform::from_translation(Vec3::new(0.0, 1.5, 5.0)),
        PanOrbitCamera {
            radius: Some(60.0),
            ..Default::default()
        },
    ));

    commands.spawn(DirectionalLight {
        ..Default::default()
    });
}

fn spin_light(mut query: Query<(&mut Transform, &DirectionalLight)>) {
    for (mut t, d) in query.iter_mut() {
        t.rotate_x(std::f32::consts::PI / (60. * 80.));
        t.rotate_y(std::f32::consts::PI / (60. * 20.));
    }
}

fn main() {
    App::new()
        .add_plugins(DefaultPlugins.set(RenderPlugin {
            render_creation: RenderCreation::Automatic(WgpuSettings {
                features: WgpuFeatures::POLYGON_MODE_LINE,
                ..default()
            }),
            ..default()
        }))
        .add_plugins(FlatNormalMaterialPlugin)
        .add_plugins((PanOrbitCameraPlugin, WireframePlugin))
        .add_plugins(FpsCounterPlugin)
        .insert_resource(WireframeConfig {
            global: false,
            default_color: GREEN.into(),
        })
        .add_systems(Startup, (setup, setup_demo_sphere))
        .add_systems(Update, toggle_wireframe)
        // .add_systems(
        //     FixedUpdate,
        //     (
        //         chunk_to_mesh,
        //         orderless_chunker.run_if(on_timer(Duration::from_millis(10))),
        //     ),
        // )
        // .add_systems(
        //     Update,
        //     (
        //         update_mesh_colors,
        //         randomize_colors.run_if(on_timer(Duration::from_millis(50))),
        //     ),
        // )
        .add_systems(FixedUpdate, spin_light)
        .run();
}

#[derive(Component)]
struct Wireframeable;

fn toggle_wireframe(
    mut commands: Commands,
    with_wireframe: Query<Entity, (With<Wireframeable>, With<Wireframe>)>,
    without_wireframe: Query<Entity, (With<Wireframeable>, Without<Wireframe>)>,
    input: Res<ButtonInput<KeyCode>>,
) {
    if input.just_pressed(KeyCode::Space) {
        for entity in with_wireframe.iter() {
            commands.entity(entity).remove::<Wireframe>();
        }

        for entity in without_wireframe.iter() {
            commands.entity(entity).insert(Wireframe);
        }
    }
}
