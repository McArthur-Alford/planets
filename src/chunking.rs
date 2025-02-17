use bevy::pbr::{ExtendedMaterial, OpaqueRendererMethod};
use bevy::prelude::*;
use bevy::utils::HashMap;
use std::collections::{BTreeMap, BTreeSet, VecDeque};

use crate::camera::CameraTarget;
use crate::flatnormal::FlatNormalMaterial;
use crate::geometry_data::GeometryData;
use crate::octree::{Octree, Point};

#[derive(Debug)]
pub enum ChunkAction {
    Create(Vec<u8>),
    Delete(Vec<u8>),
}

#[derive(Component)]
pub struct ChunkManager {
    pub geometry: GeometryData,
    pub octree: Octree,
    pub active_chunks: HashMap<Vec<u8>, Entity>,
    pub backlog: VecDeque<ChunkAction>,
    pub pov: Vec3,
}

impl ChunkManager {
    pub fn new(geometry: GeometryData) -> Self {
        // Example parameters
        let capacity = 256;
        let bounds = 1.0;
        let center = Vec3::ZERO;
        let mut octree = Octree::new(capacity, center, bounds, 0, vec![]);

        // Insert each cell
        for (cell_index, &position) in geometry.cell_normals.iter().enumerate() {
            octree.insert(Point {
                position,
                value: cell_index,
            });
        }

        Self {
            geometry,
            octree,
            active_chunks: HashMap::new(),
            backlog: VecDeque::new(),
            pov: Vec3::ZERO,
        }
    }

    /// When POV changes, figure out which chunks should exist, queue up
    /// creation for newly needed chunks, and queue deletion for chunks no longer needed.
    pub fn update_pov(&mut self, new_pov: Vec3) {
        if self.pov == new_pov {
            return;
        }

        self.pov = new_pov;

        // 1) Query the octree for the correct chunk indices near POV
        let needed_indices = self.octree.get_chunk_indices(new_pov);

        // 2) Find which are new vs. which we already have.
        let needed_set: HashMap<_, _> = needed_indices
            .iter()
            .map(|idx| (idx.clone(), true))
            .collect();
        let current_set: HashMap<_, _> = self
            .active_chunks
            .keys()
            .map(|idx| (idx.clone(), true))
            .collect();

        // For every needed index that we do NOT have, queue creation
        for idx in needed_indices {
            if !current_set.contains_key(&idx) {
                self.backlog.push_back(ChunkAction::Create(idx));
            }
        }

        // For every currently active index that is NOT needed, queue deletion
        for idx in self.active_chunks.keys() {
            if !needed_set.contains_key(idx) {
                self.backlog.push_back(ChunkAction::Delete(idx.clone()));
            }
        }
    }

    pub fn process_backlog(
        &mut self,
        n: usize,
        commands: &mut Commands,
        meshes: &mut Assets<Mesh>,
        mut flat_materials: ResMut<Assets<ExtendedMaterial<StandardMaterial, FlatNormalMaterial>>>,
    ) {
        for _ in 0..n {
            if let Some(action) = self.backlog.pop_front() {
                match action {
                    ChunkAction::Create(index) => {
                        if let Some(cells) = self.octree.get_cells_for_index(&index) {
                            let chunk_geo = self.build_chunk_geometry(&cells);
                            let mesh = meshes.add(chunk_geo.mesh());

                            let e = commands
                                .spawn((
                                    Mesh3d(mesh),
                                    Transform::from_scale(Vec3::splat(32.0)),
                                    MeshMaterial3d(flat_materials.add(ExtendedMaterial {
                                        base: StandardMaterial {
                                            opaque_render_method: OpaqueRendererMethod::Auto,
                                            ..Default::default()
                                        },
                                        extension: FlatNormalMaterial {},
                                    })),
                                    Name::new(format!("Chunk {:?}", index)),
                                ))
                                .id();

                            self.active_chunks.insert(index, e);
                        }
                    }
                    ChunkAction::Delete(index) => {
                        if let Some(entity) = self.active_chunks.remove(&index) {
                            commands.entity(entity).despawn_recursive();
                        }
                    }
                }
            } else {
                break;
            }
        }
    }

    pub fn build_chunk_geometry(&self, cells: &[usize]) -> GeometryData {
        let mut chunk_vertices = Vec::new();
        let mut chunk_faces = Vec::new();
        let mut chunk_cells = Vec::new();
        let mut chunk_cell_normals = Vec::new();

        let mut cell_map = BTreeMap::new();

        for &cell in cells {
            let face_indices = &self.geometry.cells[cell];
            let mut new_cell_faces = Vec::new();

            for &face_idx in face_indices {
                let face = self.geometry.faces[face_idx];

                for &vert_idx in &face {
                    chunk_vertices.push(self.geometry.vertices[vert_idx]);
                }
                let start = chunk_vertices.len() - 3;
                chunk_faces.push([start, start + 1, start + 2]);
                new_cell_faces.push(chunk_faces.len() - 1);
            }

            chunk_cells.push(new_cell_faces);
            chunk_cell_normals.push(self.geometry.cell_normals[cell]);
            cell_map.insert(cell, chunk_cells.len() - 1);
        }

        let mut chunk_cell_neighbors = vec![BTreeSet::new(); chunk_cells.len()];
        for (&global_cell, &local_cell) in &cell_map {
            for &neighbor in &self.geometry.cell_neighbors[global_cell] {
                if let Some(&local_neighbor) = cell_map.get(&neighbor) {
                    chunk_cell_neighbors[local_cell].insert(local_neighbor);
                    chunk_cell_neighbors[local_neighbor].insert(local_cell);
                }
            }
        }

        GeometryData {
            vertices: chunk_vertices,
            faces: chunk_faces,
            cells: chunk_cells,
            cell_neighbors: chunk_cell_neighbors,
            cell_normals: chunk_cell_normals,
        }
    }
}

pub fn update_chunk_pov_system(
    mut query: Query<&mut ChunkManager>,
    camera_query: Query<&Transform, With<Camera>>,
) {
    if let Ok(camera_transform) = camera_query.get_single() {
        if let Ok(mut manager) = query.get_single_mut() {
            manager.update_pov(camera_transform.translation.normalize());
        }
    }
}

pub fn process_chunk_backlog_system(
    mut commands: Commands,
    mut query: Query<&mut ChunkManager>,
    mut meshes: ResMut<Assets<Mesh>>,
    flat_materials: ResMut<Assets<ExtendedMaterial<StandardMaterial, FlatNormalMaterial>>>,
) {
    if let Ok(mut manager) = query.get_single_mut() {
        manager.process_backlog(32, &mut commands, &mut meshes, flat_materials);
    }
}

pub fn setup_demo_chunk_manager(mut commands: Commands) {
    let geom = crate::geometry_data::GeometryData::icosahedron()
        .subdivide_n(8)
        .slerp()
        .recell()
        .dual()
        .duplicate();

    let manager = ChunkManager::new(geom);

    commands.spawn((manager, Name::new("Planet ChunkManager")));
    commands.spawn((Transform::IDENTITY, CameraTarget { radius: 32.0 }));
}

pub struct ChunkManagerDemoPlugin;
impl Plugin for ChunkManagerDemoPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, setup_demo_chunk_manager)
            .add_systems(
                FixedUpdate,
                (update_chunk_pov_system, process_chunk_backlog_system),
            );
    }
}
