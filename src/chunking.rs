use bevy::pbr::{ExtendedMaterial, OpaqueRendererMethod};
use bevy::prelude::*;
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::time::Instant;

use crate::camera::CameraTarget;
use crate::flatnormal::FlatNormalMaterial;
use crate::geometry_data::GeometryData;
use crate::octree::{Octree, Point};

type ChunkIndex = Vec<u8>;

#[derive(Debug)]
pub enum ChunkAction {
    Create(ChunkIndex),
    Delete(ChunkIndex),
}

#[derive(Component)]
pub struct ChunkManager {
    pub geometry: GeometryData,
    pub octree: Octree,
    pub active_chunks: BTreeMap<ChunkIndex, Entity>,
    pub mesh_handles: BTreeMap<ChunkIndex, Handle<Mesh>>,
    pub backlog: VecDeque<ChunkAction>,
    pub pov: Vec3,
}

impl ChunkManager {
    pub fn new(geometry: GeometryData) -> Self {
        // Example parameters
        let capacity = 32;
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
            active_chunks: BTreeMap::new(),
            mesh_handles: BTreeMap::new(),
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
        self.backlog.clear();

        self.pov = new_pov;

        // 1) Query the octree for the correct chunk indices near POV
        let needed_indices = self.octree.get_chunk_indices(new_pov);

        // 2) Find which are new vs. which we already have.
        let needed_set: BTreeMap<_, _> = needed_indices
            .iter()
            .map(|idx| (idx.clone(), true))
            .collect();
        let current_set: BTreeMap<_, _> = self
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
        let start = Instant::now();

        let material = self.create_material(&mut flat_materials);

        for _ in 0..n {
            if start.elapsed().as_millis() > 3 {
                break;
            }

            let Some(action) = self.backlog.pop_front() else {
                break;
            };

            match action {
                ChunkAction::Create(index) => {
                    self.handle_create_chunk(index, commands, meshes, &material)
                }
                ChunkAction::Delete(index) => {
                    self.handle_delete_chunk(index, commands);
                }
            }
        }
    }

    fn create_material(
        &self,
        flat_materials: &mut ResMut<Assets<ExtendedMaterial<StandardMaterial, FlatNormalMaterial>>>,
    ) -> MeshMaterial3d<ExtendedMaterial<StandardMaterial, FlatNormalMaterial>> {
        let extended_material = ExtendedMaterial {
            base: StandardMaterial {
                opaque_render_method: OpaqueRendererMethod::Auto,
                ..Default::default()
            },
            extension: FlatNormalMaterial {},
        };
        MeshMaterial3d(flat_materials.add(extended_material))
    }

    fn handle_create_chunk(
        &mut self,
        index: ChunkIndex,
        commands: &mut Commands,
        meshes: &mut Assets<Mesh>,
        material: &MeshMaterial3d<ExtendedMaterial<StandardMaterial, FlatNormalMaterial>>,
    ) {
        let handle = match self.mesh_handles.get(&index) {
            Some(handle) => handle.clone(),
            None => {
                let cells = match self.octree.get_cells_for_index(&index) {
                    Some(c) => c,
                    None => return,
                };
                let chunk_geo = self.build_chunk_geometry(&cells);
                let new_handle = meshes.add(chunk_geo.mesh());
                self.mesh_handles
                    .entry(index.clone())
                    .or_insert(new_handle)
                    .clone()
            }
        };

        let entity = commands
            .spawn((
                Mesh3d(handle),
                material.clone(),
                Transform::from_scale(Vec3::splat(32.0)),
                Name::new(format!("Chunk {:?}", index)),
            ))
            .id();

        self.active_chunks.insert(index, entity);
    }

    fn handle_delete_chunk(&mut self, index: ChunkIndex, commands: &mut Commands) {
        if let Some(entity) = self.active_chunks.remove(&index) {
            commands.entity(entity).despawn_recursive();
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
    camera_query: Query<(&Transform, &Projection), With<Camera>>,
) {
    if let Ok((camera_transform, projection)) = camera_query.get_single() {
        if let Ok(mut manager) = query.get_single_mut() {
            let Projection::Perspective(projection) = projection else {
                return;
            };
            dbg!(&projection.fov);
            manager.update_pov(
                camera_transform.translation.normalize()
                    + camera_transform.translation.normalize() * projection.fov / 5.0,
            );
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
        // TODO
        // In the future, we could probably find a way to make this run in a worker
        // and send back chunks on a channel to the chunkmanager which simply spawns them as it gets them
        manager.process_backlog(128, &mut commands, &mut meshes, flat_materials);
    }
}

pub fn setup_demo_chunk_manager(mut commands: Commands) {
    let geom = crate::geometry_data::GeometryData::icosahedron()
        .subdivide_n(9)
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
