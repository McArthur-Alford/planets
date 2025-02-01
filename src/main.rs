mod fibonacci_sphere;
mod fibonacci_sphere_visualiser;

use crate::fibonacci_sphere::*;
use bevy::{
    pbr::wireframe::{Wireframe, WireframePlugin},
    prelude::*,
    time::common_conditions::on_timer,
};
use bevy_panorbit_camera::{PanOrbitCamera, PanOrbitCameraPlugin};
use fibonacci_sphere_visualiser::{manage_fibonacci, more_balls, move_balls};
use std::time::Duration;

#[derive(Resource)]
struct FibonacciConfig(usize, Vec<Vec3>);

#[derive(Component)]
struct Point {
    index: usize,
    end: Vec3,
}

fn setup(mut commands: Commands) {
    commands.insert_resource(FibonacciConfig(1, fibonacci_sphere(1)));

    commands.spawn((
        Transform::from_translation(Vec3::new(0.0, 1.5, 5.0)),
        PanOrbitCamera::default(),
    ));

    commands.spawn(DirectionalLight {
        ..Default::default()
    });
}

fn spin_light(mut query: Query<(&mut Transform, &DirectionalLight)>) {
    for (mut t, d) in query.iter_mut() {
        t.rotate_x(std::f32::consts::PI / (60. * 10.));
        t.rotate_y(std::f32::consts::PI / (60. * 20.));
    }
}

// fn warp_box(mut query: Query<&Mesh3d>, mut meshes: ResMut<Assets<Mesh>>, time: Res<Time>) {
//     for mut mesh in query.iter_mut() {
//         let Some(mesh) = meshes.get_mut(mesh) else {
//             continue;
//         };

//         if let Some(VertexAttributeValues::Float32x3(positions)) =
//             mesh.attribute_mut(Mesh::ATTRIBUTE_POSITION)
//         {
//             let corner = &mut positions[0];
//             corner[0] += time.elapsed_secs().sin() / 1000.;
//         };
//         mesh.compute_normals();
//         // mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, vec![[-0.5, 0.5, -0.5]]);
//     }
// }

fn main() {
    App::new()
        .add_plugins(DefaultPlugins)
        .add_plugins((PanOrbitCameraPlugin, WireframePlugin))
        .add_systems(Startup, setup)
        .add_systems(
            Update,
            (
                spin_light,
                manage_fibonacci,
                move_balls,
                more_balls.run_if(on_timer(Duration::from_millis(100))),
                toggle_wireframe,
            ),
        )
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
        for (entity) in with_wireframe.iter() {
            commands.entity(entity).insert(Wireframe);
        }

        for (entity) in without_wireframe.iter() {
            commands.entity(entity).remove::<Wireframe>();
        }
    }
}
