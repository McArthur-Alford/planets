mod camera;
mod chunk_manager;
mod chunk_storage;
mod chunk_tree;
mod chunking;
mod colors;
mod fibonacci_sphere;
mod fibonacci_sphere_visualiser;
mod flatnormal;
mod geometry_data;
mod helpers;
mod octree;

use bevy::{
    color::palettes::css::GREEN,
    pbr::wireframe::{Wireframe, WireframeConfig, WireframePlugin},
    prelude::*,
    render::{
        settings::{RenderCreation, WgpuFeatures, WgpuSettings},
        RenderPlugin,
    },
};
use bevy_fps_counter::FpsCounterPlugin;
use bevy_inspector_egui::quick::WorldInspectorPlugin;
use bevy_panorbit_camera::PanOrbitCameraPlugin;
use camera::CameraPlugin;
use chunk_storage::{despawn_chunks, spawn_ready_chunks, ChunkingPlugin};
use chunking::ChunkManagerDemoPlugin;
use colors::{randomize_colors, update_mesh_colors};
use flatnormal::FlatNormalMaterialPlugin;

#[derive(Default, Reflect, GizmoConfigGroup)]
struct Gizmos;

fn setup(mut commands: Commands) {
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
        .add_plugins((WireframePlugin))
        .add_plugins(FpsCounterPlugin)
        // .add_plugins(WorldInspectorPlugin::new())
        // .add_plugins(OctreeVisualiserPlugin)
        .add_plugins(CameraPlugin)
        .add_plugins(ChunkingPlugin)
        .insert_resource(WireframeConfig {
            global: false,
            default_color: GREEN.into(),
        })
        .add_systems(Startup, setup)
        // .add_systems(Startup, setup_demo_sphere)
        .add_systems(Update, toggle_wireframe)
        .add_systems(FixedUpdate, spin_light)
        .add_systems(FixedUpdate, randomize_colors)
        .add_systems(FixedUpdate, update_mesh_colors.after(despawn_chunks))
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
