use bevy::{color::palettes::css::RED, prelude::*};
// Spherical camera shenangigans
// Needs to map the camera position to the nearest point on the sphere
// Camera transform gets set to that point
// Mouse drag should move the camerai

#[derive(Component)]
pub(crate) struct CameraTarget {
    pub(crate) radius: f32,
}

#[derive(Component)]
pub(crate) struct GameCamera;

pub(crate) fn setup_camera(mut commands: Commands) {
    commands.spawn((
        Camera3d::default(),
        Transform::from_xyz(0.0, 0.0, 1.0),
        GameCamera,
    ));
}

pub(crate) fn position_camera(
    mut camera: Query<&mut Transform, With<GameCamera>>,
    target: Query<(&Transform, &CameraTarget), Without<GameCamera>>,
    mut gizmos: Gizmos<DefaultGizmoConfigGroup>,
) {
    let mut camera_transform = camera.single_mut();
    let (target_transform, CameraTarget { radius }) = target.single();

    let tt = target_transform.translation;
    let ct = camera_transform.translation;
    camera_transform.translation = (ct - tt).normalize() * (radius * 2.0);

    gizmos.sphere((ct - tt).normalize() * radius, 0.2, RED);
}

// 2 things:
// scrollwheel to zoom projection
// mouse drag systems (1 for detect, 1 for release) for move
// Also dont forget to rotate cam to face center?
