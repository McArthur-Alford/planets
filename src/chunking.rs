use bevy::prelude::*;
use std::collections::{BTreeMap, BTreeSet};

use crate::geometry_data::GeometryData;
use crate::octree::{Octree, Point};

#[derive(Component)]
pub struct ChunkManager {
    pub geometry: GeometryData,
    pub octree: Octree,
}

impl ChunkManager {
    pub fn new(geometry: GeometryData) -> Self {
        let capacity = 4;

        let bounds = 1.0;
        let center = Vec3::ZERO;

        let mut octree = Octree::new(capacity, center, bounds, 0);

        for (cell_index, &position) in geometry.cell_normals.iter().enumerate() {
            octree.insert(Point {
                position,
                value: cell_index,
            });
        }

        Self { geometry, octree }
    }

    pub fn get_chunks(&self, target: Vec3) -> Vec<Vec<usize>> {
        self.octree.get_chunks(target)
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

    pub fn build_geometries_for_chunks(&self, chunk_list: Vec<Vec<usize>>) -> Vec<GeometryData> {
        chunk_list
            .into_iter()
            .map(|cells| self.build_chunk_geometry(&cells))
            .collect()
    }
}
