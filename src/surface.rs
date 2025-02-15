//! A general surface component
//!
//! (useful in case i want to use something other than goldberg in
//! the future e.g. a voronoi style mesh from fibonacci sphere)

use bevy::{
    asset::RenderAssetUsages,
    pbr::{wireframe::Wireframe, ExtendedMaterial, OpaqueRendererMethod},
    prelude::*,
    render::mesh::{Indices, PrimitiveTopology::TriangleList},
};
use rand::{random, random_range};
use std::collections::{vec_deque, BTreeMap, BTreeSet, VecDeque};

use crate::{flatnormal::FlatNormalMaterial, Wireframeable};

// Easy way to tell chunks to split until they are under this
// size limit.
#[derive(Component)]
pub(crate) struct ChunkSizeLimit(pub usize);

#[derive(Component, Debug)]
pub(crate) struct Surface {
    pub(crate) cells: Vec<Cell>,
    pub(crate) chunks: Vec<Chunk>,
    pub(crate) cell_to_chunk: Vec<usize>,
}

#[derive(Default, Debug)]
pub(crate) struct Chunk {
    pub(crate) cells: Vec<usize>,
    /// Quick reverse lookup for getting indexes of  entries in cells ^^
    pub(crate) cell_to_local: BTreeMap<usize, usize>,
    pub(crate) mesh: Option<Entity>,
}

#[derive(Clone, Debug)]
pub(crate) struct Cell {
    pub(crate) position: Vec3,
    pub(crate) adjacent: BTreeSet<usize>,
    pub(crate) faces: Vec<[usize; 3]>,
    pub(crate) vertices: Vec<Vec3>,
}

/// Looks for surfaces with chunks that are too big and
/// starts splitting them up using voronoi-style chunks
pub(crate) fn neighbour_chunker(mut surfaces: Query<(&ChunkSizeLimit, &mut Surface)>) {
    for (limit, mut surface) in surfaces.iter_mut() {
        let mut splits = Vec::new();
        let Surface {
            cells,
            chunks,
            cell_to_chunk,
        } = surface.into_inner();

        let mut counter = 0;
        let chunks_len = chunks.len();
        for (i, chunk) in chunks.iter_mut().enumerate() {
            if chunk.mesh.is_some() || (chunk.cells.len() < limit.0) {
                continue;
            }
            counter += 1;

            let mut frontier = VecDeque::from([chunk.cells[random_range(0..chunk.cells.len())]]);
            let mut seen = BTreeSet::new();

            while seen.len() < limit.0 {
                let Some(front) = frontier.pop_front() else {
                    break;
                };
                if seen.contains(&front) {
                    continue;
                }
                if cell_to_chunk[front] != i {
                    continue;
                }

                seen.insert(front);

                cell_to_chunk[front] = chunks_len + splits.len();

                for adj in &cells[front].adjacent {
                    frontier.push_back(*adj);
                }
            }

            chunk.cell_to_local.retain(|c, l| !seen.contains(c));
            chunk.cells.retain(|c| !seen.contains(c));

            splits.push(Chunk {
                cells: seen.iter().cloned().collect(),
                cell_to_local: seen.into_iter().enumerate().map(|(i, c)| (c, i)).collect(),
                mesh: None,
            })
        }

        chunks.extend(splits);
    }
}

pub(crate) fn orderless_chunker(mut surfaces: Query<(&ChunkSizeLimit, &mut Surface)>) {
    for (limit, mut surface) in surfaces.iter_mut() {
        let mut splits = Vec::new();
        let Surface {
            cells,
            chunks,
            cell_to_chunk,
        } = surface.into_inner();

        let len = chunks.len();
        for chunk in &mut *chunks {
            if chunk.mesh.is_some() || (limit.0 > chunk.cells.len()) {
                continue;
            }

            // Here we can chunk it!
            chunk.cell_to_local.retain(|c, l| *l >= limit.0);
            let new_cells = chunk.cells.split_off(limit.0);
            splits.push(Chunk {
                cells: new_cells.clone(),
                cell_to_local: new_cells
                    .into_iter()
                    .enumerate()
                    .map(|(i, c)| (c, i))
                    .collect(),
                mesh: None,
            });

            for cell in &splits.last().unwrap().cells {
                cell_to_chunk[*cell] = len + splits.len()
            }
        }

        chunks.extend(splits);
    }
}

/// Looks for chunks that dont have a child mesh object
/// Creates the mesh based on chunk info
pub(crate) fn chunk_to_mesh(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    // mut materials: ResMut<Assets<StandardMaterial>>,
    mut flat_materials: ResMut<Assets<ExtendedMaterial<StandardMaterial, FlatNormalMaterial>>>,
    mut surfaces: Query<(Entity, Option<&ChunkSizeLimit>, &mut Surface)>,
) {
    for (parent, limit, surface) in surfaces.iter_mut() {
        let Surface {
            cells,
            chunks,
            cell_to_chunk,
        } = surface.into_inner();

        for chunk in chunks {
            if chunk.mesh.is_some() || (limit.is_some() && chunk.cells.len() > limit.unwrap().0) {
                continue;
            }

            let mut local_map = BTreeMap::new();

            let mut cells_sliced = Vec::new();
            for cell_idx in &chunk.cells {
                cells_sliced.push(&cells[*cell_idx]);
            }

            // Get all the vertices/faces but remap them into the local map first
            // also flatten and cast to u32 as necessary for mesh
            let mut faces = Vec::new();
            let mut vertices = Vec::new();
            let mut colors = Vec::new();
            let mut normals = Vec::new();
            let mut c = 0;
            for cell in cells_sliced {
                let color = [random::<f32>(), random(), random(), 1.0];
                for face in &cell.faces {
                    // i is an index into the cell vertices (typically 0..11)
                    // i + c is an index into the new vertices (way bigger)
                    for &i in face {
                        // remap i here
                        let i = *local_map.entry(i + c).or_insert_with(|| {
                            vertices.push(cell.vertices[i]);
                            vertices.len() - 1
                        }) as u32;
                        faces.push(i);
                        normals.push(cell.position);
                        colors.push(color.clone());
                    }
                }
                c = faces.len();
            }

            // Generate a mesh for this chunk
            let mesh = Mesh::new(
                TriangleList,
                RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
            )
            .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, vertices)
            .with_inserted_indices(Indices::U32(faces))
            .with_inserted_attribute(
                Mesh::ATTRIBUTE_COLOR,
                // vec![[1.0, 0.0, 0.0, 1.0]; normals.len()],
                colors,
            )
            .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, normals);

            // Update the chunk reference
            let mut entity = commands.spawn((
                Wireframeable,
                // Wireframe,
                Mesh3d(meshes.add(mesh)),
                // transform.clone(),
                Transform::IDENTITY,
                MeshMaterial3d(flat_materials.add(ExtendedMaterial {
                    base: StandardMaterial {
                        opaque_render_method: OpaqueRendererMethod::Auto,
                        ..Default::default()
                    },
                    extension: FlatNormalMaterial {},
                })),
            ));

            // Add a child
            entity.set_parent(parent);
            chunk.mesh = Some(entity.id());
        }
    }
}
