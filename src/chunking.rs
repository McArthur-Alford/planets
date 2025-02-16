use crate::geometry_data::GeometryData;
use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::collections::VecDeque;

#[derive(Debug)]
pub(crate) enum Chunk {
    /// Store a list of cell indices into the root geometry data
    Leaf(Vec<usize>),
    /// Stores a list of chunks of lower resolutions
    Parent(Vec<Chunk>),
}

pub(crate) trait Chunker {
    fn subdivide(&self, cells: &[usize], geom: &GeometryData) -> Vec<Vec<usize>>;
}

pub(crate) struct FloodfillChunker {
    // The minimum size for a chunk
    pub(crate) min_size: usize,
    // The maximum amount of chunks (will try to reach this)
    pub(crate) max_chunks: usize,
}

impl Chunker for FloodfillChunker {
    fn subdivide(&self, cells: &[usize], geom: &GeometryData) -> Vec<Vec<usize>> {
        // If small enough, no need to subdivide
        if cells.len() <= self.min_size {
            return vec![cells.to_vec()];
        }

        // Compute the number of cells per chunk
        // If there arent enough for max chunks, reduce the number of chunks until its over min_size
        let mut max_chunks = self.max_chunks;
        while (cells.len() / max_chunks) <= self.min_size {
            max_chunks -= 1;
            if max_chunks == 1 {
                // Break early here
                return vec![cells.to_vec()];
            };
        }
        let max_cells = cells.len() / max_chunks;

        // Otherwise, do some BFS/flood‐fill to group them up
        let mut visited = BTreeSet::new();
        let mut result = Vec::new();

        let cell_set: BTreeSet<usize> = cells.iter().copied().collect();

        for &cell in cells {
            if visited.contains(&cell) {
                continue;
            }

            // Start a new chunk
            let mut stack = VecDeque::from(vec![cell]);
            let mut group = Vec::new();

            while let Some(current) = stack.pop_back() {
                if !visited.insert(current) {
                    continue; // already visited
                }
                group.push(current);

                // If we are about to exceed max cells, bail
                if group.len() >= max_cells {
                    break;
                }

                // Add neighbors (intersecting with `cells`) to stack
                for &nbr in &geom.cell_neighbors[current] {
                    if cell_set.contains(&nbr) && !visited.contains(&nbr) {
                        stack.push_front(nbr);
                    }
                }
            }
            result.push(group);
        }

        result
    }
}

impl Chunk {
    pub(crate) fn build(cells: Vec<usize>, geom: &GeometryData, chunker: &impl Chunker) -> Self {
        let subsets = chunker.subdivide(&cells, geom);

        match subsets.len() {
            0 => Chunk::Leaf(cells),
            1 => Chunk::Leaf(subsets[0].clone()),
            _ => Chunk::Parent(
                subsets
                    .into_iter()
                    .map(|subset| Chunk::build(subset, geom, chunker))
                    .collect(),
            ),
        }
    }

    // Get refs to chunks at a specified depth
    // Depth 0 returns self
    pub(crate) fn depth(&self, depth: usize) -> Vec<&Chunk> {
        if depth == 0 {
            return vec![self];
        }

        let mut result = Vec::new();
        match self {
            Chunk::Leaf(_) => result.push(self),
            Chunk::Parent(vec) => {
                for chunk in vec {
                    result.extend(chunk.depth(depth - 1));
                }
            }
        }

        result
    }

    pub(crate) fn cells(&self) -> Vec<usize> {
        let mut result = Vec::new();
        match self {
            Chunk::Leaf(vec) => result.extend_from_slice(vec),
            Chunk::Parent(vec) => {
                for chunk in vec {
                    result.extend(chunk.cells());
                }
            }
        }

        result
    }

    pub(crate) fn local_geometry(&self, geometry_data: &GeometryData) -> GeometryData {
        let cells = self.cells();

        // Yoink the points from the geometrydata.
        // When getting the associated faces... translate to local faces.
        // Same for cells, and store those translations in the map.
        let mut chunk_vertices = Vec::new();
        let mut chunk_faces = Vec::new();
        let mut chunk_cells = Vec::new();
        let mut cell_map = BTreeMap::new();
        for cell in cells {
            let faces = &geometry_data.cells[cell];
            let mut new_cell = Vec::new();
            for &face in faces {
                let vertices = geometry_data.faces[face];
                for vert in vertices {
                    chunk_vertices.push(geometry_data.vertices[vert]);
                }
                let start = chunk_vertices.len() - 3;
                chunk_faces.push([start, start + 1, start + 2]);
                new_cell.push(chunk_faces.len() - 1);
            }
            chunk_cells.push(new_cell);
            cell_map.insert(cell, chunk_cells.len() - 1);
        }

        let mut chunk_cell_neighbors = vec![BTreeSet::new(); chunk_cells.len()];
        for (&global, &local) in &cell_map {
            // Get adjacent cells to global
            // Map those to local
            // For the ones that can be mapped, insert them into positions:
            // 1. chunk_cell_neighbors[local_destination -> local]
            // 2. chunk_cell_neighbours[local -> local_destination]
            for &local_target in geometry_data.cell_neighbors[global]
                .iter()
                .filter_map(|n| cell_map.get(n))
            {
                chunk_cell_neighbors[local_target].insert(local);
                chunk_cell_neighbors[local].insert(local_target);
            }
        }

        GeometryData {
            vertices: chunk_vertices,
            faces: chunk_faces,
            cells: chunk_cells,
            cell_neighbors: chunk_cell_neighbors,
        }
    }
}
