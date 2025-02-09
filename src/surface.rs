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
use std::collections::{BTreeMap, BTreeSet};

use crate::{flatnormal::FlatNormalMaterial, Wireframeable};

// Easy way to tell chunks to split until they are under this
// size limit.
#[derive(Component)]
pub(crate) struct ChunkSizeLimit(pub usize);

#[derive(Component)]
pub(crate) struct Surface {
    pub(crate) cells: Vec<Cell>,
    pub(crate) chunks: Vec<Chunk>,
    pub(crate) cell_to_chunk: Vec<usize>,
}

#[derive(Default)]
pub(crate) struct Chunk {
    pub(crate) faces: Vec<[usize; 3]>,
    pub(crate) vertices: Vec<Vec3>,
    pub(crate) cell_to_face: BTreeMap<usize, Vec<usize>>,
    pub(crate) face_to_cell: Vec<usize>,
    pub(crate) mesh: Option<Entity>,
}

#[derive(Clone)]
pub(crate) struct Cell {
    pub(crate) position: Vec3,
    pub(crate) adjacent: BTreeSet<usize>,
}

/// Looks for surfaces with chunks that are too big and
/// starts splitting them up using voronoi-style chunks
pub(crate) fn chunker(mut surfaces: Query<(&ChunkSizeLimit, &mut Surface)>) {
    for (limit, mut surface) in surfaces.iter_mut() {
        for chunk in surface.chunks.iter() {
            if chunk.cell_to_face.len() <= limit.0 {
                continue;
            }

            let mut new_chunk = Chunk {
                cell_to_face: chunk
                    .cell_to_face
                    .clone()
                    .into_iter()
                    .skip(limit.0)
                    .collect(),
                ..Default::default()
            };
            new_chunk.face_to_cell = new_chunk
                .cell_to_face
                .iter()
                .flat_map(|(cell, faces)| faces.iter().map(|face| chunk.face_to_cell[*face]))
                .skip(limit.0)
                .collect();

            for face in limit.0..chunk.cell_to_face.len() {
                let face = chunk.faces[face];
                // Move this face and its vertices into the new chunk
                // We want to move these cells, their faces and vertices into
                // a new chunk. Tragically, this maybe means removing vertices,
                // and messing up the indices!!
            }

            // This chunk has more cells pointing into it
            // than the limit, so we split off all but the first
            // <limit> cells into their own new chunk
        }
    }
}

/// Looks for chunks that dont have a child mesh object
/// Creates the mesh based on chunk info
pub(crate) fn chunk_to_mesh(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    // mut materials: ResMut<Assets<StandardMaterial>>,
    mut flat_materials: ResMut<Assets<ExtendedMaterial<StandardMaterial, FlatNormalMaterial>>>,
    mut surfaces: Query<(Entity, &ChunkSizeLimit, &mut Surface, &Transform)>,
) {
    for (parent, limit, mut surface, transform) in surfaces.iter_mut() {
        let Surface {
            cells,
            chunks,
            cell_to_chunk,
        } = surface.into_inner();

        for chunk in chunks {
            if chunk.mesh.is_some() || chunk.cell_to_face.len() > limit.0 {
                continue;
            }

            // Generate a mesh for this chunk
            let mesh = Mesh::new(
                TriangleList,
                RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
            )
            .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, chunk.vertices.clone())
            .with_inserted_indices(Indices::U32(
                chunk
                    .faces
                    .clone()
                    .into_flattened()
                    .iter()
                    .map(|&c| c as u32)
                    .collect(),
            ))
            .with_inserted_attribute(
                Mesh::ATTRIBUTE_COLOR,
                vec![[0.0, 0.0, 0.0, 1.0]; chunk.vertices.len()],
            )
            .with_inserted_attribute(
                Mesh::ATTRIBUTE_NORMAL,
                chunk
                    .face_to_cell
                    .iter()
                    .map(|&h| [cells[h as usize].position; 3])
                    .flatten()
                    .collect::<Vec<_>>(),
            );

            // Update the chunk reference
            let mut entity = commands.spawn((
                Wireframeable,
                Wireframe,
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
