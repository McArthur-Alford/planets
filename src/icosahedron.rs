use super::helpers;
use super::Wireframeable;
use bevy::asset::RenderAssetUsages;
use bevy::pbr::wireframe::Wireframe;
use bevy::prelude::*;
use bevy::render::mesh::Indices;
use bevy::render::mesh::PrimitiveTopology::TriangleList;
use std::collections::BTreeMap;

pub(crate) struct Icosahedron {
    pub(crate) vertices: Vec<[f32; 3]>,
    pub(crate) faces: Vec<[u32; 3]>,
}

impl Icosahedron {
    pub(crate) fn new() -> Self {
        // Generates the vertices of an icosahedron (20 faced polyhedron)
        //
        // Technique from https://blog.lslabs.dev/posts/generating_icosphere_with_code
        let phi = (1.0 + 5.0_f32.sqrt()) / 2.0;
        let du = 1.0 / (phi * phi + 1.0).sqrt();
        let dv = phi * du;

        let vertices = vec![
            [0.0, dv, du],
            [0.0, dv, -du],
            [0.0, -dv, du],
            [0.0, -dv, -du],
            [du, 0.0, dv],
            [-du, 0.0, dv],
            [du, 0.0, -dv],
            [-du, 0.0, -dv],
            [dv, du, 0.0],
            [dv, -du, 0.0],
            [-dv, du, 0.0],
            [-dv, -du, 0.0],
        ];

        let faces = vec![
            [0, 1, 8],
            [0, 4, 5],
            [0, 5, 10],
            [0, 8, 4],
            [0, 10, 1],
            [1, 6, 8],
            [1, 7, 6],
            [1, 10, 7],
            [2, 3, 11],
            [2, 4, 9],
            [2, 5, 4],
            [2, 9, 3],
            [2, 11, 5],
            [3, 6, 7],
            [3, 7, 11],
            [3, 9, 6],
            [4, 8, 9],
            [5, 11, 10],
            [6, 9, 8],
            [7, 10, 11],
        ]
        .into_iter()
        .map(|mut v| {
            v.reverse();
            v
        })
        .collect();

        Icosahedron { vertices, faces }
    }

    pub(crate) fn subdivide(&mut self) {
        // Subdivides self once
        // For each face:
        // 1) Split each edge with a new vertex in the middle.
        //    - Use some kind of map so that edges previously split are kept
        //    - Use (u32, u32) pairs of indices rather than float vectors for consistency
        //    - If it has already been split, instead get the index
        //    - If it is not already split, create a new index at the end of vertices and add it.
        // 2) After splitting the three edges of a face, create 4 new faces for each subtriangle.
        // 3) Add those faces to the new face vector.
        let mut btree: BTreeMap<(u32, u32), u32> = BTreeMap::new();
        let mut new_faces = Vec::<[u32; 3]>::new();
        for &[i, j, k] in &self.faces {
            // Splits i,j, j,k and k,i into 3 new vertices:
            let mut splits = Vec::new();
            for (u, v) in [(i, j), (j, k), (k, i)] {
                let index = *btree
                    .entry(helpers::ordered_2tuple(u, v))
                    .or_insert_with(|| {
                        self.vertices.push({
                            let x = self.vertices[u as usize];
                            let y = self.vertices[v as usize];
                            [
                                (x[0] + y[0]) / 2.0,
                                (x[1] + y[1]) / 2.0,
                                (x[2] + y[2]) / 2.0,
                            ]
                        });
                        (self.vertices.len() - 1) as u32
                    });
                splits.push(index);
            }
            let [ij, jk, ki] = splits[0..3] else {
                panic!("This should be impossible")
            };
            new_faces.extend([[i, ij, ki], [ij, j, jk], [ki, jk, k], [ij, jk, ki]]);
        }

        std::mem::swap(&mut self.faces, &mut new_faces);
    }

    pub(crate) fn slerp(&mut self) {
        // Slerps (sphere lerp?) all the vertices so that they lie on the unit sphere
        for vertex in self.vertices.iter_mut() {
            let len = (vertex[0].powi(2) + vertex[1].powi(2) + vertex[2].powi(2)).sqrt();
            vertex[0] /= len;
            vertex[1] /= len;
            vertex[2] /= len;
        }
    }
}

pub(crate) fn icosahedron_demo(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let mut ico = Icosahedron::new();
    for _ in 0..3 {
        ico.subdivide();
        ico.slerp();
    }
    let Icosahedron { vertices, faces } = ico;

    let mesh = Mesh::new(
        TriangleList,
        RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
    )
    .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, vertices)
    .with_inserted_indices(Indices::U32(faces.into_flattened()))
    .with_computed_smooth_normals();

    commands.spawn((
        Wireframeable,
        Wireframe,
        Mesh3d(meshes.add(mesh.clone())),
        Transform::from_xyz(0., 0., 0.).with_scale(Vec3::new(4.0, 4.0, 4.0)),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: Color::srgb_u8(0, 0, 255),
            // double_sided: true,
            // cull_mode: None,
            ..default()
        })),
    ));
}
