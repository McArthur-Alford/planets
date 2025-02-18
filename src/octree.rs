use std::sync::Arc;

use bevy::{math::NormedVectorSpace, pbr::wireframe::Wireframe, prelude::*};
use bevy_panorbit_camera::PanOrbitCamera;

use crate::chunking::ChunkManager;

// The plan:
// Break space up into cubic chunks, each containing cells.

#[derive(Debug, Clone)]
pub(crate) struct Point {
    pub(crate) position: Vec3,
    pub(crate) value: usize,
}

/// an octree that performs redistribution of ALL points into children
/// when the capacity is met
#[derive(Component, Debug, Clone)]
pub(crate) struct Octree {
    pub(crate) children: Box<[Option<Octree>; 8]>,
    pub(crate) center: Vec3,
    pub(crate) points: Option<Vec<Point>>,
    pub(crate) capacity: usize,
    pub(crate) bounds: f32, // The distance to the edge of the octree from the center (half-width)
    pub(crate) height: usize, // The height of this node (distance from furthest leaf)
    pub(crate) depth: usize,
    pub(crate) octree_index: Vec<u8>,
}

impl Octree {
    pub(crate) fn new(
        capacity: usize,
        center: Vec3,
        bounds: f32,
        depth: usize,
        octree_index: Vec<u8>,
    ) -> Self {
        Octree {
            children: Box::new([const { None }; 8]),
            center,
            points: Some(Vec::new()),
            capacity,
            bounds,
            height: 0,
            depth,
            octree_index,
        }
    }

    pub(crate) fn pos_to_child(&self, pos: Vec3) -> usize {
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

    pub(crate) fn insert(&mut self, point: Point) {
        // Add points to self if points is some and within capacity
        if self.points.is_some() && self.points.as_ref().unwrap().len() <= self.capacity {
            self.points.as_mut().unwrap().push(point);
            return;
        }

        // Otherwise (points is none or we exceed cap)
        // Add to a child
        let index = self.pos_to_child(point.position);
        if self.children[index].is_none() {
            let center = self.center + (point.position - self.center).signum() * self.bounds / 2.0;
            let mut octree_index = self.octree_index.clone();
            octree_index.push(index as u8);
            self.children[index] = Some(Octree::new(
                self.capacity,
                center,
                self.bounds / 2.,
                self.depth + 1,
                octree_index,
            ));
        }
        if let Some(ot) = self.children[index].as_mut() {
            ot.insert(point)
        }

        // If self.points is some but we got here (over capacity), we redistribute them into children
        // and set it to none. Nice and easy!
        if self.points.is_some() {
            if let Some(points) = std::mem::take(&mut self.points) {
                for point in points {
                    self.insert(point);
                }
            }
        }

        for child in self.children.iter() {
            if let Some(child) = child {
                self.height = self.height.max(child.height + 1);
            }
        }
    }

    pub(crate) fn cells(&self) -> Vec<usize> {
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

    pub(crate) fn get_chunks(&self, target: Vec3) -> Vec<Vec<usize>> {
        let multiplier = (1.0 / self.height as f32) * self.bounds; // 1/max_depth steps

        let projected = target.clamp(
            self.center - Vec3::splat(self.bounds),
            self.center + Vec3::splat(self.bounds),
        );

        let dist = projected.distance(target);
        let mut desired_height = 0;

        while dist >= (desired_height as f32 + 0.1) * multiplier {
            desired_height += 1;
        }

        let mut results = Vec::new();
        if desired_height >= self.height {
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

    pub(crate) fn get_chunk_indices(&self, target: Vec3) -> Vec<Vec<u8>> {
        let multiplier = (1.0 / self.height.max(1) as f32) * self.bounds;
        let projected = target.clamp(
            self.center - Vec3::splat(self.bounds),
            self.center + Vec3::splat(self.bounds),
        );
        let dist = projected.distance_squared(target).powf(1.5);
        let mut desired_height = 0;
        while dist >= (desired_height as f32 + 0.1) * multiplier {
            desired_height += 1;
        }

        if dist > 0.9 {
            return Vec::new();
        }

        let mut results = Vec::new();
        if desired_height >= self.height {
            results.push(self.octree_index.clone());
        } else {
            for child in self.children.iter().flatten() {
                results.extend(child.get_chunk_indices(target));
            }
        }
        results
    }

    pub(crate) fn get_cells_for_index(&self, index_path: &[u8]) -> Option<Vec<usize>> {
        if self.octree_index == index_path {
            return Some(self.cells());
        }

        if index_path.starts_with(&self.octree_index) && index_path.len() > self.octree_index.len()
        {
            let next_child = index_path[self.octree_index.len()] as usize;
            if let Some(ref child) = self.children[next_child] {
                return child.get_cells_for_index(index_path);
            } else {
                return None;
            }
        }
        None
    }
}

#[derive(Component)]
pub(crate) struct OctreeVisualiser;

pub(crate) fn octree_visualiser(
    octree_query: Query<&ChunkManager>,
    visualiser_query: Query<Entity, With<OctreeVisualiser>>,
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
) {
    for entity in visualiser_query.iter() {
        commands.entity(entity).despawn();
    }

    if octree_query.is_empty() {
        return;
    }

    let mut qts = vec![octree_query.single().octree.clone()];
    let mut chunk_meshes = Vec::new();

    while let Some(qt) = qts.pop() {
        let cube: Mesh = Cuboid::default().mesh().into();
        chunk_meshes.push(
            cube.scaled_by(Vec3::splat(qt.bounds * 2.0))
                .translated_by(qt.center),
        );
        for child in qt.children.iter() {
            if let Some(child) = child {
                qts.push(Arc::new(child.clone()));
            }
        }
    }

    let mut mesh = chunk_meshes.pop().unwrap();
    for m in chunk_meshes {
        mesh.merge(&m);
    }

    commands.spawn((
        Mesh3d(meshes.add(mesh)),
        Transform::default().with_scale(Vec3::splat(32.0)),
        Wireframe,
        OctreeVisualiser,
    ));
}

pub(crate) struct OctreeVisualiserPlugin;

pub(crate) fn octree_visualiser_startup(mut commands: Commands) {
    let octree = Octree::new(5, Vec3::ZERO, 50.0, 0, vec![0]);

    commands.spawn(octree);

    commands.spawn((
        Transform::from_translation(Vec3::new(0.0, 0.0, 1.0)),
        PanOrbitCamera {
            radius: Some(1000.0),
            ..Default::default()
        },
    ));
}

impl Plugin for OctreeVisualiserPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            PostStartup,
            (
                // octree_visualiser_startup,
                octree_visualiser,
            ),
        );
    }
}
