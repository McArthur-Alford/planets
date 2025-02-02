use crate::fibonacci_sphere;
use crate::Wireframeable;
use bevy::prelude::*;

#[derive(Resource)]
struct FibonacciConfig(usize, Vec<Vec3>);

#[derive(Component)]
struct Point {
    index: usize,
    end: Vec3,
}

pub(crate) fn more_balls(mut fib: ResMut<FibonacciConfig>) {
    // fib.0 += 1;
    // let i = fib.0 as u32;
    // fib.1 = fibonacci_sphere(i)
}

pub(crate) fn move_balls(mut points: Query<(&mut Point, &mut Transform)>) {
    for (mut point, mut transform) in points.iter_mut() {
        let start = transform.translation;
        let end = point.end;
        let mut dir = end - start;
        if dir.length() < 0.001 {
            transform.translation = end;
            continue;
        }
        let dist = dir.length();
        let speed = 0.001;
        dir = dir.normalize();
        let delta = (dir * speed).clamp_length(0.0, dist.sqrt());
        transform.translation += delta;
    }
}

pub(crate) fn manage_fibonacci(
    fib: ResMut<FibonacciConfig>,
    mut points: Query<&mut Point>,
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let mut threshold = 0;
    for mut point in points.iter_mut() {
        threshold = point.index.max(threshold);
        point.end = fib.1[point.index];
    }
    // This is where any new points should start counting indices
    threshold += 1;

    if threshold < fib.0 {
        for i in threshold..fib.0 {
            commands.spawn((
                Wireframeable,
                Point {
                    index: i,
                    end: fib.1[i],
                },
                Transform::from_xyz(0., 0., 0.),
                Mesh3d(meshes.add(Sphere::new(0.01))),
                MeshMaterial3d(materials.add(Color::srgb_u8(124, 144, 255))),
            ));
        }
    }
}
