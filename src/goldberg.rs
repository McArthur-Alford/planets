use super::Wireframeable;
use crate::flatnormal::FlatNormalMaterial;
use crate::helpers::ordered_3tuple;
use crate::helpers::sort_poly_vertices;
use crate::icosahedron::Icosahedron;
use bevy::asset::RenderAssetUsages;
use bevy::pbr::wireframe::Wireframe;
use bevy::pbr::ExtendedMaterial;
use bevy::pbr::OpaqueRendererMethod;
use bevy::prelude::*;
use bevy::render::mesh::Indices;
use bevy::render::mesh::PrimitiveTopology::TriangleList;
use std::collections::BTreeMap;
use std::collections::BTreeSet;

pub(crate) struct GoldbergPoly {
    /// Hex (plus penta) positions
    /// These are the icosahedron vertices
    pub(crate) hexes: Vec<[f32; 3]>,

    /// Adjacency list of hex (plus penta) indices
    /// These come from the icosahedron edges
    pub(crate) adjacency: Vec<BTreeSet<u32>>,

    /// Vertices of the mesh
    pub(crate) vertices: Vec<[f32; 3]>,

    /// Faces of the mesh
    pub(crate) faces: Vec<[u32; 3]>,
}

impl GoldbergPoly {
    pub(crate) fn new(icosahedron: Icosahedron) -> Self {
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
                gold_faces.push([o, d[0], d[1]])
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
        }
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

pub(crate) fn setup_hex(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ExtendedMaterial<StandardMaterial, FlatNormalMaterial>>>,
) {
    let mut ico = Icosahedron::new();
    for _ in 0..8 {
        ico.subdivide();
        ico.slerp();
    }

    let mut gold = GoldbergPoly::new(ico);
    gold.slerp();

    let GoldbergPoly {
        hexes: _,
        adjacency: _,
        vertices,
        faces,
    } = gold;

    let mesh = Mesh::new(
        TriangleList,
        RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
    )
    .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, vertices)
    .with_inserted_indices(Indices::U32(faces.into_flattened()));

    commands.spawn((
        Wireframeable,
        Wireframe,
        Mesh3d(meshes.add(mesh.clone())),
        Transform::from_xyz(0., 0., 0.).with_scale(Vec3::new(16.0, 16.0, 16.0)),
        MeshMaterial3d(materials.add(ExtendedMaterial {
            base: StandardMaterial {
                base_color: Color::srgb_u8(0, 0, 255),
                // double_sided: true,
                // cull_mode: None,
                opaque_render_method: OpaqueRendererMethod::Auto,
                ..default()
            },
            extension: FlatNormalMaterial {},
        })),
    ));
}
