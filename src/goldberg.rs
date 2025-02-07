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
use rand::random_range;
use std::collections::BTreeMap;
use std::collections::BTreeSet;

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

        // map of faces to their hexagons
        let mut face_to_hex = Vec::new();

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
        }
    }
}

impl GoldbergPoly {
    pub(crate) fn new(divisions: usize) -> Self {
        let mut ico = Icosahedron::new();
        for _ in 0..divisions {
            ico.subdivide();
            ico.slerp();
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

pub(crate) fn setup_hex(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut flat_materials: ResMut<Assets<ExtendedMaterial<StandardMaterial, FlatNormalMaterial>>>,
) {
    let mut gold = GoldbergPoly::new(5);
    gold.separate_shared_vertices();

    let GoldbergPoly {
        hexes,
        vertices,
        faces,
        face_to_hex,
        ..
    } = gold;

    // Quickly generate enough unique colours for each face:
    let dark = [0.57, 0.43, 0.20, 1.0];
    let bright = [0.98, 0.95, 0.59, 1.0];

    // let dark = [1.0, 0.0, 0.0, 1.0];
    // let bright = [0.0, 1.0, 0.0, 1.0];

    // let dark = [1.0, 0.0, 1.0, 1.0];
    // let bright = [0.0, 1.0, 1.0, 1.0];

    let mut face_colors = Vec::<[f32; 4]>::new();
    for &h in &face_to_hex {
        let t = random_range(0.0..=1.0);

        // let hex = hexes[h as usize];
        // let t = (noisy_bevy::simplex_noise_3d(Vec3::from(hex)) + 1.0) / 2.0;

        // Linear interpolation for each channel:
        let mut r = dark[0] + t * (bright[0] - dark[0]);
        let mut g = dark[1] + t * (bright[1] - dark[1]);
        let mut b = dark[2] + t * (bright[2] - dark[2]);
        let a = 1.0; // Keep alpha at 1.0

        // if t < 0.4 {
        //     r = 0.0;
        //     g = 0.0;
        //     b = 1.0;
        // } else if t < 0.8 {
        //     r = 0.0;
        //     g = 0.5;
        //     b = 0.0;
        // } else {
        //     r = 0.9;
        //     g = 0.9;
        //     b = 0.91;
        // }

        face_colors.push([r, g, b, a]);
    }

    let mut vertex_colors = vec![[0., 0., 0., 0.]; vertices.len()];
    for (f, [i, j, k]) in faces.iter().enumerate() {
        // We have a face with vertices i, j, k, for the hex at face_to_hex[f]
        vertex_colors[*i as usize] = face_colors[f];
        vertex_colors[*j as usize] = face_colors[f];
        vertex_colors[*k as usize] = face_colors[f];
    }

    let mesh = Mesh::new(
        TriangleList,
        RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
    )
    .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, vertices)
    .with_inserted_indices(Indices::U32(faces.into_flattened()))
    .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, vertex_colors)
    .with_inserted_attribute(
        Mesh::ATTRIBUTE_NORMAL,
        face_to_hex
            .iter()
            .map(|&h| [hexes[h as usize]; 3])
            .flatten()
            .collect::<Vec<_>>(),
    );

    commands.spawn((
        Wireframeable,
        Wireframe,
        Mesh3d(meshes.add(mesh.clone())),
        Transform::from_xyz(0., 0., 0.).with_scale(Vec3::new(16.0, 16.0, 16.0)),
        MeshMaterial3d(flat_materials.add(ExtendedMaterial {
            base: StandardMaterial {
                // double_sided: true,
                // cull_mode: None,
                opaque_render_method: OpaqueRendererMethod::Auto,
                ..default()
            },
            extension: FlatNormalMaterial {},
        })),
        // MeshMaterial3d(materials.add(StandardMaterial {
        //     base_color: Color::srgb_u8(0, 0, 255),
        //     ..Default::default()
        // })),
    ));
}
