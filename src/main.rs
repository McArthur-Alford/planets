mod fibonacci_sphere;
mod fibonacci_sphere_visualiser;

use std::{
    cmp::Ordering,
    collections::{BTreeMap, BTreeSet},
};

use crate::fibonacci_sphere::*;
use bevy::{
    asset::RenderAssetUsages,
    color::palettes::css::GREEN,
    pbr::wireframe::{Wireframe, WireframeConfig, WireframePlugin},
    prelude::*,
    reflect::List,
    render::{
        mesh::{Indices, PrimitiveTopology::TriangleList},
        settings::{RenderCreation, WgpuFeatures, WgpuSettings},
        RenderPlugin,
    },
};
use bevy_panorbit_camera::{PanOrbitCamera, PanOrbitCameraPlugin};

fn ordered_3tuple(u: u32, v: u32, w: u32) -> (u32, u32, u32) {
    let mut arr = [u, v, w];
    arr.sort();
    (arr[0], arr[1], arr[2])
}

fn ordered_2tuple(u: u32, v: u32) -> (u32, u32) {
    if u > v {
        (u, v)
    } else {
        (v, u)
    }
}

struct Icosahedron {
    vertices: Vec<[f32; 3]>,
    faces: Vec<[u32; 3]>,
}

impl Icosahedron {
    fn new() -> Self {
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

    fn subdivide(&mut self) {
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
                let index = *btree.entry(ordered_2tuple(u, v)).or_insert_with(|| {
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

    fn slerp(&mut self) {
        // Slerps (sphere lerp?) all the vertices so that they lie on the unit sphere
        for vertex in self.vertices.iter_mut() {
            let len = (vertex[0].powi(2) + vertex[1].powi(2) + vertex[2].powi(2)).sqrt();
            vertex[0] /= len;
            vertex[1] /= len;
            vertex[2] /= len;
        }
    }
}

fn sort_poly_vertices(vertices: &Vec<[f32; 3]>, indicies: Vec<u32>) -> Vec<u32> {
    let mut u = indicies[0];
    let mut seen = BTreeSet::from([u]);
    let mut sorted = vec![u];

    // Get the indices closest to i and pick one that isnt already in sorted
    loop {
        if seen.len() == indicies.len() {
            break;
        }

        let mut max_distance = f32::INFINITY;
        let mut j = usize::MAX;
        for (i, v) in indicies.clone().into_iter().enumerate() {
            if v == u {
                continue;
            }

            let a = vertices[u as usize];
            let b = vertices[v as usize];

            let distance = (a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2);

            if distance < max_distance && !seen.contains(&v) {
                max_distance = distance;
                j = i;
            }
        }

        u = indicies[j];
        seen.insert(u);
        sorted.push(u);
    }

    sorted
}

struct GoldbergPoly {
    /// Hex (plus penta) positions
    hexes: Vec<[f32; 3]>,
    /// Adjacency list of hex (plus penta) indices
    adjacency: Vec<BTreeSet<u32>>,
    /// Vertices of the mesh
    vertices: Vec<[f32; 3]>,
    /// Faces of the mesh
    faces: Vec<[u32; 3]>,
}

impl GoldbergPoly {
    fn new(icosahedron: Icosahedron) -> Self {
        let Icosahedron {
            vertices: hexes,
            faces,
        } = icosahedron;

        // For each vertex, find all outgoing vertices
        // produce an adjacency matrix for easier lookup
        let mut adjacency = vec![BTreeSet::new(); hexes.len()];
        for face in faces {
            let (i, j, k) = (face[0], face[1], face[2]);
            adjacency[i as usize].insert(j);
            adjacency[i as usize].insert(k);
            adjacency[j as usize].insert(k);
            adjacency[j as usize].insert(i);
            adjacency[k as usize].insert(i);
            adjacency[k as usize].insert(j);
        }

        // Using the adj list, find all outgoing edges from each hex
        // split those edges and create a hexagon out of the split vertices
        // put that hexagons triangles/indices into the vertex/face list
        let mut btree: BTreeMap<(u32, u32, u32), u32> = BTreeMap::new();
        let mut vertices = hexes.clone();
        let mut faces = Vec::new();
        for i in 0..vertices.len() as u32 {
            let adjacent = adjacency[i as usize].iter().cloned().collect::<Vec<u32>>();
            let indicies = sort_poly_vertices(&vertices, adjacent);

            let mut splits = Vec::new();
            let mut prev = indicies.len() as u32 - 1;
            let mut edges = Vec::new();
            for j in 0..indicies.len() as u32 {
                // i, j, j+1 (with wraparound) form a triangle
                // create a split on the avg position of the vertices
                splits.push(
                    *btree
                        .entry(ordered_3tuple(
                            i,
                            indicies[j as usize],
                            indicies[(j as usize + 1) % indicies.len()] as u32,
                        ))
                        .or_insert({
                            vertices.push({
                                let x = vertices[i as usize];
                                let y = vertices[indicies[j as usize] as usize];
                                let z =
                                    vertices[indicies[(j as usize + 1) % indicies.len()] as usize];
                                [
                                    (x[0] + y[0] + z[0]) / 3.0,
                                    (x[1] + y[1] + z[1]) / 3.0,
                                    (x[2] + y[2] + z[2]) / 3.0,
                                ]
                            });
                            let curr = (vertices.len() - 1) as u32;
                            edges.push([prev, curr]);
                            prev = curr;
                            curr
                        }),
                );
            }

            // // We have no idea which of the 6 points connects to which other.
            // // For that reason, we compute the distances between all pairs
            // let mut edges = Vec::new();
            // for &u in &splits[0..] {
            //     let mut distances = Vec::new();
            //     for &v in &splits[0..] {
            //         if v == u {
            //             continue;
            //         }
            //         let a = vertices[u as usize];
            //         let b = vertices[v as usize];
            //         let dist =
            //             (a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2);

            //         distances.push((v, dist));
            //     }
            //     distances.sort_by(|&a, &b| a.1.partial_cmp(&b.1).unwrap_or(Ordering::Equal));
            //     let best: [_; 2] = distances.clone()[0..2].try_into().unwrap();
            //     edges.extend([[best[0].0, u], [u, best[1].0]]);
            // }

            // now form polygons from splits[0] and each edge
            let origin = splits[0];
            let mut new_faces = edges
                .into_iter()
                .map(|e| [origin, e[0], e[1]])
                .filter(|e| !(e[0] == e[1] || e[0] == e[2]))
                .collect::<Vec<_>>();
            for face in &mut new_faces {
                // We have a bunch of edges, make sure they are all clockwise
                // if not, flip their order
                let [a, b, c] = (0..3)
                    .map(|i| Vec3::from(vertices[face[i] as usize]))
                    .collect::<Vec<Vec3>>()[..3]
                else {
                    panic!("Impossible");
                };

                let norm = (b - a).cross(c - a);

                let w = norm.dot(a);
                if w < 0. {
                    face.reverse();
                }
            }
            faces.extend(new_faces);
        }

        Self {
            hexes,
            adjacency,
            vertices,
            faces,
        }
    }
}

fn setup_icosahedron(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    // let (points, indicies) = build_icosphere(1);
    let mut ico = Icosahedron::new();
    for i in (0..10) {
        ico.subdivide();
        ico.slerp();
    }

    let gold = GoldbergPoly::new(ico);

    let GoldbergPoly {
        hexes,
        adjacency,
        vertices,
        faces,
    } = gold;

    let mut mesh = Mesh::new(
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
        Transform::from_xyz(0., 0., 0.),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: Color::srgb_u8(0, 0, 255),
            // double_sided: true,
            // cull_mode: None,
            ..default()
        })),
    ));
}

fn setup_hex(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let points = (0..6i8)
        .map(f32::from)
        .map(|i| (std::f32::consts::PI * 2.0 * (i / 6.0)))
        .map(|r| [r.cos(), 0.0, r.sin()])
        .collect::<Vec<_>>();

    let indicies = (1..5)
        .map(|i| [0, i + 1, i])
        .flatten()
        // .map(|i| (i + 3) % 3)
        .collect::<Vec<_>>();

    // Spawn a hexagon mesh somehow..
    let mesh = Mesh::new(
        TriangleList,
        RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
    )
    .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, points)
    .with_computed_normals()
    // .with_inserted_indices(Indices::U32(vec![0, 1, 2, 0, 2, 3, 0, 3, 4, 0, 4, 5]));
    .with_inserted_indices(Indices::U32(indicies));

    commands.spawn((
        Wireframeable,
        Wireframe,
        Transform::from_xyz(0., 0., 0.),
        Mesh3d(meshes.add(mesh)),
        MeshMaterial3d(materials.add(Color::srgb_u8(0, 0, 255))),
    ));
}

fn setup(mut commands: Commands) {
    commands.spawn((
        Transform::from_translation(Vec3::new(0.0, 1.5, 5.0)),
        PanOrbitCamera::default(),
    ));

    commands.spawn(DirectionalLight {
        ..Default::default()
    });
}

fn spin_light(mut query: Query<(&mut Transform, &DirectionalLight)>) {
    for (mut t, d) in query.iter_mut() {
        t.rotate_x(std::f32::consts::PI / (60. * 10.));
        t.rotate_y(std::f32::consts::PI / (60. * 20.));
    }
}

// fn warp_box(mut query: Query<&Mesh3d>, mut meshes: ResMut<Assets<Mesh>>, time: Res<Time>) {
//     for mut mesh in query.iter_mut() {
//         let Some(mesh) = meshes.get_mut(mesh) else {
//             continue;
//         };

//         if let Some(VertexAttributeValues::Float32x3(positions)) =
//             mesh.attribute_mut(Mesh::ATTRIBUTE_POSITION)
//         {
//             let corner = &mut positions[0];
//             corner[0] += time.elapsed_secs().sin() / 1000.;
//         };
//         mesh.compute_normals();
//         // mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, vec![[-0.5, 0.5, -0.5]]);
//     }
// }

fn main() {
    App::new()
        .add_plugins(DefaultPlugins.set(RenderPlugin {
            render_creation: RenderCreation::Automatic(WgpuSettings {
                features: WgpuFeatures::POLYGON_MODE_LINE,
                ..default()
            }),
            ..default()
        }))
        .add_plugins((PanOrbitCameraPlugin, WireframePlugin))
        .insert_resource(WireframeConfig {
            global: false,
            default_color: GREEN.into(),
        })
        .add_systems(Startup, (setup, setup_icosahedron))
        .add_systems(Update, (spin_light, toggle_wireframe))
        .run();
}

#[derive(Component)]
struct Wireframeable;

fn toggle_wireframe(
    mut commands: Commands,
    with_wireframe: Query<Entity, (With<Wireframeable>, With<Wireframe>)>,
    without_wireframe: Query<Entity, (With<Wireframeable>, Without<Wireframe>)>,
    input: Res<ButtonInput<KeyCode>>,
) {
    if input.just_pressed(KeyCode::Space) {
        for entity in with_wireframe.iter() {
            commands.entity(entity).remove::<Wireframe>();
        }

        for entity in without_wireframe.iter() {
            commands.entity(entity).insert(Wireframe);
        }
    }
}
