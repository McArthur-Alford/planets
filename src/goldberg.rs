use super::Wireframeable;
use crate::colors::HexColors;
use crate::flatnormal::FlatNormalMaterial;
use crate::helpers::ordered_3tuple;
use crate::helpers::sort_poly_vertices;
use crate::icosahedron::Icosahedron;
use crate::surface;
use crate::surface::Cell;
use crate::surface::Chunk;
use crate::surface::ChunkSizeLimit;
use crate::surface::Surface;
use bevy::asset::RenderAssetUsages;
use bevy::pbr::wireframe::Wireframe;
use bevy::pbr::ExtendedMaterial;
use bevy::pbr::OpaqueRendererMethod;
use bevy::prelude::*;
use bevy::render::mesh::Indices;
use bevy::render::mesh::PrimitiveTopology::TriangleList;
use std::collections::BTreeMap;
use std::collections::BTreeSet;

#[derive(Clone)]
pub(crate) struct GoldbergPoly {
    /// Hex (plus penta) positions
    /// These are the icosahedron vertices
    /// They also conveniently work as normals for the faces
    pub(crate) hexes: Vec<[f32; 3]>,

    /// Adjacency list of hex (plus penta) indices
    /// These come from the icosahedron edges
    pub(crate) adjacency: Vec<BTreeSet<u32>>,

    /// Vertices of the mesh
    pub(crate) vertices: Vec<[f32; 3]>,

    /// Faces of the mesh
    pub(crate) faces: Vec<[u32; 3]>,

    // Mapping of face (by index into faces) to
    // the associated hex (as an index into hexes)
    pub(crate) face_to_hex: Vec<u32>,

    // Mapping of hex (by index into hexes) to
    // the associated face (as an index into faces)
    pub(crate) hex_to_face: Vec<Vec<u32>>,
}

impl From<Icosahedron> for GoldbergPoly {
    fn from(icosahedron: Icosahedron) -> Self {
        let Icosahedron {
            vertices: ico_vertices,
            faces: ico_faces,
        } = icosahedron;

        // split_map maps icosahedron triangles to goldberg vertices, storing the index
        let mut split_map = BTreeMap::<(u32, u32, u32), u32>::new();
        // goldberg vertices for the new goldberg polyhedron
        let mut gold_vertices = Vec::<[f32; 3]>::new();
        // goldberg faces for the new goldberg polyhedron
        let mut gold_faces = Vec::<[u32; 3]>::new();

        // map of faces to their hexagons and the reverse
        let mut face_to_hex = Vec::new();
        let mut hex_to_face = vec![Vec::new(); ico_vertices.len()];

        // For each vertex, find all outgoing vertices
        // produce an adjacency matrix for easier lookup
        let mut adjacency = vec![BTreeSet::new(); ico_vertices.len()];
        for face in ico_faces {
            let (i, j, k) = (face[0], face[1], face[2]);
            adjacency[i as usize].insert(j);
            adjacency[i as usize].insert(k);
            adjacency[j as usize].insert(k);
            adjacency[j as usize].insert(i);
            adjacency[k as usize].insert(i);
            adjacency[k as usize].insert(j);
        }

        // Iterate over each vertex
        // vertex c is at the center of the hex, at index ci
        for (ci, c) in ico_vertices.iter().enumerate() {
            // Sort all adjacent vertices clockwise
            // adjacent has indices into the icosahedron
            let adjacent = adjacency[ci].iter().cloned().collect::<Vec<u32>>();
            let adjacent = sort_poly_vertices(&ico_vertices, adjacent);

            // For each triple of (c, adjacent[i], adjacent[i+1]) get the center of the poly
            // This is our split location
            // Also track the triangle indices (into icosahedron vertices) for good measure
            let mut splits = Vec::new();
            let mut triangles = Vec::new();

            for ai in 0..adjacent.len() {
                let ai_1 = adjacent[ai];
                let ai_2 = adjacent[(ai + 1) % adjacent.len()];
                let a1 = ico_vertices[ai_1 as usize];
                let a2 = ico_vertices[ai_2 as usize];
                let avg = [
                    (c[0] + a1[0] + a2[0]) / 3.0,
                    (c[1] + a1[1] + a2[1]) / 3.0,
                    (c[2] + a1[2] + a2[2]) / 3.0,
                ];
                splits.push(avg);
                triangles.push((ci as u32, ai_1, ai_2))
            }

            // Iterate over the splits/triangles
            // slot them into the BTreeMap exactly once, returning a list of goldberg indexes
            // These are potentially created as part of the loop
            let mut gold_indices = Vec::new();
            for (vert, tri) in splits.iter().zip(triangles) {
                let gi = *split_map.entry(ordered_3tuple(tri)).or_insert_with(|| {
                    // Push it onto vertices
                    // Return vertices.len() - 1 as the new index
                    gold_vertices.push(*vert);
                    (gold_vertices.len() - 1) as u32
                });

                gold_indices.push(gi);
            }

            // The list of goldberg indexes can be turned into a ring of edges for the hex
            // Triangulate that and slap the edges into the goldberg faces
            let o = gold_indices[0];
            for d in gold_indices[1..].windows(2) {
                gold_faces.push([o, d[0], d[1]]);
                face_to_hex.push(ci as u32);
                hex_to_face[ci].push(face_to_hex.len() as u32 - 1);
            }
        }

        // And as a final precaution against back-face culling,
        // flip any faces order that is not clockwise
        for face in &mut gold_faces {
            let [a, b, c] = (0..3)
                .map(|i| Vec3::from(gold_vertices[face[i] as usize]))
                .collect::<Vec<Vec3>>()[..3]
            else {
                panic!("Impossible!!")
            };

            // dot the normal with a vector and see if its <0
            if (b - a).cross(c - a).dot(a) < 0. {
                face.reverse();
            }
        }

        Self {
            hexes: ico_vertices,
            adjacency,
            vertices: gold_vertices,
            faces: gold_faces,
            face_to_hex,
            hex_to_face,
        }
    }
}

impl GoldbergPoly {
    pub(crate) fn new(divisions: usize) -> Self {
        let mut ico = Icosahedron::new();
        for _ in 0..divisions {
            ico.subdivide();
        }
        let mut gold = GoldbergPoly::from(ico);
        gold.slerp();

        gold
    }

    pub(crate) fn separate_shared_vertices(&mut self) {
        // Take old vertices/faces out
        let old_vertices = std::mem::take(&mut self.vertices);
        let old_faces = std::mem::take(&mut self.faces);

        let mut new_vertices = Vec::with_capacity(old_faces.len() * 3);
        let mut new_faces = Vec::with_capacity(old_faces.len());

        // For each face, duplicate its 3 vertices so that no face shares them.
        for [i0, i1, i2] in old_faces {
            let v0 = old_vertices[i0 as usize];
            let v1 = old_vertices[i1 as usize];
            let v2 = old_vertices[i2 as usize];

            // Record the starting index of the new face's vertices
            let start_index = new_vertices.len() as u32;
            new_vertices.push(v0);
            new_vertices.push(v1);
            new_vertices.push(v2);

            // Update face indices to the newly pushed vertices
            new_faces.push([start_index, start_index + 1, start_index + 2]);
        }

        // Put new data back in
        self.vertices = new_vertices;
        self.faces = new_faces;
    }

    pub(crate) fn slerp(&mut self) {
        // Slerps (sphere lerp?) all the vertices so that they lie on the unit sphere
        // useful for dealing with hexagons not being perfectly flat
        for vertex in self.vertices.iter_mut() {
            let len = (vertex[0].powi(2) + vertex[1].powi(2) + vertex[2].powi(2)).sqrt();
            vertex[0] /= len;
            vertex[1] /= len;
            vertex[2] /= len;
        }
    }
}

impl Into<Surface> for GoldbergPoly {
    fn into(self) -> Surface {
        let cells = self
            .hexes
            .iter()
            .zip(self.adjacency)
            .map(|(hex, adj)| Cell {
                position: Vec3::from_slice(hex),
                adjacent: adj.iter().map(|&adj| adj as usize).collect(),
            })
            .collect::<Vec<_>>();

        // Start out with a single chunk, it can get split later
        let chunks = vec![Chunk {
            faces: self
                .faces
                .iter()
                .map(|&[a, b, c]| [a as usize, b as usize, c as usize])
                .collect(),
            vertices: self
                .vertices
                .iter()
                .map(|&[a, b, c]| Vec3::new(a, b, c))
                .collect(),
            cell_to_face: self
                .hex_to_face
                .iter()
                .enumerate()
                .map(|(c, f)| (c, f.iter().map(|&f| f as usize).collect()))
                .collect(),
            face_to_cell: self.face_to_hex.iter().map(|&f| f as usize).collect(),
            mesh: None,
        }];

        // They can all just map to 0 for now :)
        let cell_to_chunk = vec![0; cells.len()];

        Surface {
            cells,
            chunks,
            cell_to_chunk,
        }
    }
}

pub(crate) fn setup_hex(mut commands: Commands) {
    let mut gold = GoldbergPoly::new(4);
    gold.separate_shared_vertices();
    let surface: Surface = gold.into();

    commands.spawn((
        surface,
        Transform::IDENTITY.with_scale(Vec3::new(16.0, 16.0, 16.0)),
        ChunkSizeLimit(50),
    ));
}
