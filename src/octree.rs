use bevy::{pbr::wireframe::Wireframe, prelude::*};
use bevy_panorbit_camera::PanOrbitCamera;
use rand::random;

use crate::geometry_data::GeometryData;

// The plan:
// Break space up into cubic chunks, each containing cells.

#[derive(Debug)]
struct Point {
    position: Vec3,
    value: usize,
}

/// an octree that performs redistribution of ALL points into children
/// when the capacity is met
#[derive(Component, Debug)]
pub(crate) struct OcTree {
    children: Box<[Option<OcTree>; 8]>,
    center: Vec3,
    points: Option<Vec<Point>>,
    capacity: usize,
    bounds: f32,   // The distance to the edge of the octree from the center (half-width)
    height: usize, // The height of this node (distance from furthest leaf)
}

impl OcTree {
    fn new(capacity: usize, center: Vec3, bounds: f32) -> Self {
        OcTree {
            children: Box::new([const { None }; 8]),
            center,
            points: Some(Vec::new()),
            capacity,
            bounds,
            height: 0,
        }
    }

    fn pos_to_child(&self, pos: Vec3) -> usize {
        let diff = (pos - self.center).signum();

        // Diff is -1 and 1
        // Add 1: 0 and 2
        // Div 2: 0 and 1 easy index
        let diff = (diff + 1.) / 2.;

        // Treats the signs as bits, - is 0, + is 1
        // So (+x, +y, +z) is 111 or position 8 in the children
        let index = 1. * diff.x + 2. * diff.y + 4. * diff.z;

        index as usize
    }

    fn insert(&mut self, point: Point) {
        if self.points.is_some() && self.points.as_ref().unwrap().len() <= self.capacity {
            self.points.as_mut().unwrap().push(point);
            return;
        }

        let index = self.pos_to_child(point.position);
        if self.children[index].is_none() {
            let center = self.center + (point.position - self.center).signum() * self.bounds / 2.0;
            self.children[index] = Some(OcTree::new(self.capacity, center, self.bounds / 2.));
        }

        self.children[index].as_mut().map(|ot| ot.insert(point));

        // If self.points is some but we got here, we redistribute them into children
        // and set it to none. Nice and easy!
        // if let Some(points) = std::mem::take(&mut self.points) {
        //     for point in points {
        //         self.insert(point);
        //     }
        // }
        // self.points = None;

        for child in self.children.iter() {
            if let Some(child) = child {
                self.height = self.height.max(child.height + 1);
            }
        }
    }

    fn cells(&self) -> Vec<usize> {
        let mut results = Vec::new();
        if let Some(points) = &self.points {
            results.extend(points.iter().map(|p| p.value).collect::<Vec<_>>());
        } else {
            for child in self.children.iter() {
                if let Some(child) = child {
                    results.extend(child.cells());
                }
            }
        }

        results
    }

    fn get_chunks(&self, target: Vec3) -> Vec<Vec<usize>> {
        let multiplier = 0.1 * self.bounds; // 10% of the bound for each step

        let projected = target.clamp(
            self.center - Vec3::splat(self.bounds),
            self.center + Vec3::splat(self.bounds),
        );
        let dist = projected.distance(target);
        let mut desired_height = 0;

        while dist >= (desired_height as f32 + 1.) * multiplier {
            desired_height += 1;
        }

        let mut results = Vec::new();
        if desired_height == self.height {
            // Yay, return our current set of cells
            results.push(self.cells());
        } else if desired_height < self.height {
            // Nope, we are too high up, recurse to lower heights
            for child in self.children.iter() {
                if let Some(child) = child {
                    results.extend(child.get_chunks(target));
                };
            }
        }

        results
    }
}

#[derive(Component)]
pub(crate) struct OcTreeVisualiser;

pub(crate) fn octree_visualiser(
    octree_query: Query<&OcTree>,
    visualiser_query: Query<Entity, With<OcTreeVisualiser>>,
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
) {
    for entity in visualiser_query.iter() {
        commands.entity(entity).despawn();
    }

    let mut qts = vec![octree_query.single()];
    let mut chunk_meshes = Vec::new();

    while let Some(qt) = qts.pop() {
        let cube: Mesh = Cuboid::default().mesh().into();
        chunk_meshes.push(
            cube.scaled_by(Vec3::splat(qt.bounds * 2.0))
                .translated_by(qt.center),
        );
        for child in qt.children.iter() {
            if let Some(child) = child {
                qts.push(child);
            }
        }
    }

    let mut mesh = chunk_meshes.pop().unwrap();
    for m in chunk_meshes {
        mesh.merge(&m);
    }

    commands.spawn((
        Mesh3d(meshes.add(mesh)),
        Transform::default(),
        Wireframe,
        OcTreeVisualiser,
    ));
}

pub(crate) struct OcTreeDemoPlugin;

pub(crate) fn octree_demo_startup(mut commands: Commands) {
    let mut octree = OcTree::new(1, Vec3::ZERO, 50.0);

    let vertices = GeometryData::icosahedron()
        .subdivide_n(4)
        .slerp()
        .recell()
        .dual()
        .duplicate();
    for v in vertices.vertices {
        octree.insert(Point {
            position: v * 32.,
            value: 1,
        });
    }

    commands.spawn(octree);

    commands.spawn((
        Transform::from_translation(Vec3::new(0.0, 0.0, 1.0)),
        PanOrbitCamera {
            radius: Some(1000.0),
            ..Default::default()
        },
    ));
}

impl Plugin for OcTreeDemoPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Startup,
            (
                octree_demo_startup,
                octree_visualiser.after(octree_demo_startup),
            ),
        );
    }
}
