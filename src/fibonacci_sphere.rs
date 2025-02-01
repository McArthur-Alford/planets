use bevy::prelude::*;

pub(crate) fn fibonacci_sphere_point(i: u32, n: u32) -> Vec3 {
    let phi = std::f32::consts::PI * (5.0f32.sqrt() - 1.0);

    let y = 1.0 - (i as f32 / (n as f32 - 1.0)) * 2.0;
    let radius = (1.0 - y * y).sqrt();

    let theta = phi * i as f32;

    let x = theta.cos() * radius;
    let z = theta.sin() * radius;

    Vec3::new(x, y, z)
}

pub(crate) fn fibonacci_sphere(n: u32) -> Vec<Vec3> {
    (0..n).map(|i| fibonacci_sphere_point(i, n)).collect()
}
