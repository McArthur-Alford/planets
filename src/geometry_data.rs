use bevy::pbr::wireframe::Wireframe;
use bevy::pbr::{ExtendedMaterial, OpaqueRendererMethod};
use bevy::render::mesh::{Indices, PrimitiveTopology::TriangleList};
use bevy::{asset::RenderAssetUsages, prelude::*};
use rand::{random, random_range};
use std::collections::{BTreeMap, BTreeSet, VecDeque};

use crate::flatnormal::FlatNormalMaterial;
use crate::helpers::{self, sort_poly_vertices};
use crate::Wireframeable;

#[derive(Default, Clone)]
pub(crate) struct GeometryData {
    /// Stores the position of vertex i at index i
    vertices: Vec<Vec3>,
    /// Stores the faces in the geometry
    faces: Vec<[usize; 3]>,
    /// Stores the groupings of faces into cells
    cells: Vec<Vec<usize>>,
    /// Stores cell neighbors
    cell_neighbors: Vec<BTreeSet<usize>>,
    /// Tree of the parent vertices for each vertex created out of subdivision
    /// Subdivision technically means 3 parent vertices, but we grab one at random
    /// The initial vertices just point to themselves for solo chunks
    /// So that we can go back up the tree for roughly even chunking
    chunks: Vec<usize>,
}

// #[derive(Debug)]
// pub(crate) enum Chunk {
//     /// Store a list of cell indices into the root geometry data
//     Leaf(Vec<usize>),
//     /// Stores a list of chunks of lower resolutions
//     Parent(Vec<Chunk>),
// }

// pub(crate) trait Chunker {
//     fn subdivide(&self, cells: &[usize], geom: &GeometryData) -> Vec<Vec<usize>>;
// }

// pub(crate) struct FloodfillChunker {
//     // The minimum size for a chunk
//     min_size: usize,
//     // The maximum amount of chunks (will try to reach this)
//     max_chunks: usize,
// }

// impl Chunker for FloodfillChunker {
//     fn subdivide(&self, cells: &[usize], geom: &GeometryData) -> Vec<Vec<usize>> {
//         // If small enough, no need to subdivide
//         if cells.len() <= self.min_size {
//             return vec![cells.to_vec()];
//         }

//         // Compute the number of cells per chunk
//         // If there arent enough for max chunks, reduce the number of chunks until its over min_size
//         let mut max_chunks = self.max_chunks;
//         while (cells.len() / max_chunks) <= self.min_size {
//             max_chunks -= 1;
//             if max_chunks == 1 {
//                 // Break early here
//                 return vec![cells.to_vec()];
//             };
//         }
//         let max_cells = cells.len() / max_chunks;

//         // Otherwise, do some BFS/flood‐fill to group them up
//         let mut visited = BTreeSet::new();
//         let mut result = Vec::new();

//         for &cell in cells {
//             if visited.contains(&cell) {
//                 continue;
//             }

//             // Start a new chunk
//             let mut stack = VecDeque::from(vec![cell]);
//             let mut group = Vec::new();

//             while let Some(current) = stack.pop_back() {
//                 if !visited.insert(current) {
//                     continue; // already visited
//                 }
//                 group.push(current);

//                 // If we are about to exceed max cells, bail
//                 if group.len() >= max_cells {
//                     break;
//                 }

//                 // Add neighbors (intersecting with `cells`) to stack
//                 for &nbr in &geom.cell_neighbors[current] {
//                     println!("///");
//                     dbg!(cells.binary_search(&nbr).is_ok());
//                     dbg!(!visited.contains(&nbr));
//                     println!("///");
//                     if cells.binary_search(&nbr).is_ok() && !visited.contains(&nbr) {
//                         stack.push_front(nbr);
//                     }
//                 }
//             }
//             result.push(group);
//         }

//         println!("-----------");
//         dbg!(cells.len());
//         dbg!(max_chunks);
//         dbg!(max_cells);
//         dbg!(result.len());
//         for item in &result {
//             dbg!(item.len());
//         }
//         println!("-----------");
//         result
//     }
// }

// impl Chunk {
//     fn build(cells: Vec<usize>, geom: &GeometryData, chunker: &impl Chunker) -> Self {
//         let subsets = chunker.subdivide(&cells, geom);

//         match subsets.len() {
//             0 => Chunk::Leaf(cells),
//             1 => Chunk::Leaf(subsets[0].clone()),
//             _ => Chunk::Parent(
//                 subsets
//                     .into_iter()
//                     .map(|subset| Chunk::build(subset, geom, chunker))
//                     .collect(),
//             ),
//         }
//     }

//     // Get refs to chunks at a specified depth
//     // Depth 0 returns self
//     fn depth(&self, depth: usize) -> Vec<&Chunk> {
//         if depth == 0 {
//             return vec![self];
//         }

//         let mut result = Vec::new();
//         match self {
//             Chunk::Leaf(_) => result.push(self),
//             Chunk::Parent(vec) => {
//                 for chunk in vec {
//                     result.extend(chunk.depth(depth - 1));
//                 }
//             }
//         }

//         result
//     }

//     fn local_geometry(&self, geometry_data: &GeometryData) -> GeometryData {
//         let cells = match self {
//             Chunk::Leaf(vec) => vec,
//             // This one will combine the cell refs of all leaves and generate *that* mesh.
//             Chunk::Parent(vec) => todo!(),
//         };

//         // Yoink the points from the geometrydata.
//         // When getting the associated faces... translate to local faces.
//         // Same for cells, and store those translations in the map.
//         let mut chunk_vertices = Vec::new();
//         let mut chunk_faces = Vec::new();
//         let mut chunk_cells = Vec::new();
//         let mut cell_map = BTreeMap::new();
//         for &cell in cells {
//             let faces = &geometry_data.cells[cell];
//             let mut new_cell = Vec::new();
//             for &face in faces {
//                 let vertices = geometry_data.faces[face];
//                 for vert in vertices {
//                     chunk_vertices.push(geometry_data.vertices[vert]);
//                 }
//                 let start = chunk_vertices.len() - 3;
//                 chunk_faces.push([start, start + 1, start + 2]);
//                 new_cell.push(chunk_faces.len() - 1);
//             }
//             chunk_cells.push(new_cell);
//             cell_map.insert(cell, chunk_cells.len() - 1);
//         }

//         let mut chunk_cell_neighbors = vec![BTreeSet::new(); chunk_cells.len()];
//         for (&global, &local) in &cell_map {
//             // Get adjacent cells to global
//             // Map those to local
//             // For the ones that can be mapped, insert them into positions:
//             // 1. chunk_cell_neighbors[local_destination -> local]
//             // 2. chunk_cell_neighbours[local -> local_destination]
//             for &local_target in geometry_data.cell_neighbors[global]
//                 .iter()
//                 .filter_map(|n| cell_map.get(n))
//             {
//                 chunk_cell_neighbors[local_target].insert(local);
//                 chunk_cell_neighbors[local].insert(local_target);
//             }
//         }

//         GeometryData {
//             vertices: chunk_vertices,
//             faces: chunk_faces,
//             cells: chunk_cells,
//             cell_neighbors: chunk_cell_neighbors,
//         }
//     }
// }

impl GeometryData {
    fn dual(mut self) -> Self {
        let mut dual_vertices = Vec::new();
        let mut dual_faces = Vec::new();
        let mut dual_cells = Vec::new();

        // Maps a face to its centroid index in dual_vertices if it already has been created
        let mut centroids = BTreeMap::<usize, usize>::new();
        for face_indices in self.cells.iter() {
            dual_cells.push(Vec::new());

            let mut sorted = Vec::new();
            for &f in face_indices {
                let face = self.faces[f];
                // Get the centroid of the face
                let mut avg = Vec3::ZERO;
                for v in face {
                    avg += self.vertices[v];
                }
                avg /= 3.0;

                sorted.push(*centroids.entry(f).or_insert_with(|| {
                    dual_vertices.push(avg);
                    dual_vertices.len() - 1
                }));
            }

            sorted = sort_poly_vertices(&dual_vertices, sorted);

            // Utilizing the list of sorted vertices, construct faces
            let o = sorted[0];
            for d in sorted[1..].windows(2) {
                dual_faces.push([o, d[0], d[1]]);
                dual_cells
                    .last_mut()
                    .expect("Should have an element")
                    .push(dual_faces.len() - 1);
            }
        }

        let mut dual_cell_neighbors = vec![BTreeSet::default(); dual_cells.len()];
        for face in &self.faces {
            dual_cell_neighbors[face[0]].insert(face[1]);
            dual_cell_neighbors[face[1]].insert(face[2]);
            dual_cell_neighbors[face[2]].insert(face[0]);
        }

        // And as a final precaution against back-face culling,
        // flip any faces order that is not clockwise
        for face in &mut dual_faces {
            let [a, b, c] = (0..3)
                .map(|i| Vec3::from(dual_vertices[face[i]]))
                .collect::<Vec<Vec3>>()[..3]
            else {
                panic!("Impossible!!")
            };

            // dot the normal with a vector and see if its <0
            if (b - a).cross(c - a).dot(a) < 0. {
                face.reverse();
            }
        }

        std::mem::swap(&mut self.vertices, &mut dual_vertices);
        std::mem::swap(&mut self.faces, &mut dual_faces);
        std::mem::swap(&mut self.cells, &mut dual_cells);
        std::mem::swap(&mut self.cell_neighbors, &mut dual_cell_neighbors);

        self
    }

    /// Duplicates vertices (necessary for proper normals)
    fn duplicate(mut self) -> Self {
        let mut new_vertices = Vec::with_capacity(self.faces.len() * 3);
        let mut new_faces = Vec::with_capacity(self.faces.len());

        for [i0, i1, i2] in self.faces {
            let v0 = self.vertices[i0 as usize];
            let v1 = self.vertices[i1 as usize];
            let v2 = self.vertices[i2 as usize];

            let start_index = new_vertices.len();
            new_vertices.push(v0);
            new_vertices.push(v1);
            new_vertices.push(v2);

            new_faces.push([start_index, start_index + 1, start_index + 2]);
        }

        self.vertices = new_vertices;
        self.faces = new_faces;

        self
    }

    fn subdivide_n(mut self, n: usize) -> Self {
        for _ in 0..n {
            self = self.subdivide();
        }
        self
    }

    fn subdivide(mut self) -> Self {
        // Subdivides self once
        // For each face:
        // 1) Split each edge with a new vertex in the middle.
        //    - Use some kind of map so that edges previously split are kept
        //    - Use (u32, u32) pairs of indices rather than float vectors for consistency
        //    - If it has already been split, instead get the index
        //    - If it is not already split, create a new index at the end of vertices and add it.
        // 2) After splitting the three edges of a face, create 4 new faces for each subtriangle.
        // 3) Add those faces to the new face vector.
        let mut btree: BTreeMap<(usize, usize), usize> = BTreeMap::new();
        let mut new_faces = Vec::<[usize; 3]>::new();

        for &[i, j, k] in &self.faces {
            // Splits i,j, j,k and k,i into 3 new vertices:
            let mut splits = Vec::new();
            for (u, v) in [(i, j), (j, k), (k, i)] {
                let index = *btree
                    .entry(helpers::ordered_2tuple(u, v))
                    .or_insert_with(|| {
                        // New vertex, tell it its parent is i
                        self.chunks.push(i);
                        self.vertices.push({
                            let x = self.vertices[u];
                            let y = self.vertices[v];
                            (x + y) / 2.
                        });
                        self.vertices.len() - 1
                    });
                splits.push(index);
            }
            let [ij, jk, ki] = splits[0..3] else {
                panic!("This should be impossible")
            };
            new_faces.extend([[i, ij, ki], [ij, j, jk], [ki, jk, k], [ij, jk, ki]]);
        }

        std::mem::swap(&mut self.faces, &mut new_faces);

        self
    }

    fn slerp(mut self) -> Self {
        for vertex in self.vertices.iter_mut() {
            std::mem::swap(vertex, &mut vertex.normalize());
        }
        self
    }

    fn deregulate(mut self) -> Self {
        self
    }

    fn relax(mut self) -> Self {
        self
    }

    fn recell(mut self) -> Self {
        let mut cells = BTreeMap::new();
        for (i, face) in self.faces.iter().enumerate() {
            for &v in face {
                cells.entry(v).or_insert_with(BTreeSet::new).insert(i);
            }
        }

        let mut cell_neighbors = vec![BTreeSet::default(); cells.len()];
        for face in &self.faces {
            cell_neighbors[face[0]].insert(face[1]);
            cell_neighbors[face[1]].insert(face[2]);
            cell_neighbors[face[2]].insert(face[0]);
        }

        self.cells = cells
            .into_values()
            .map(|f| f.into_iter().collect())
            .collect();
        self.cell_neighbors = cell_neighbors;
        self
    }

    fn icosahedron() -> Self {
        let phi = (1.0 + 5.0_f32.sqrt()) / 2.0;
        let du = 1.0 / (phi * phi + 1.0).sqrt();
        let dv = phi * du;

        let vertices = vec![
            Vec3::new(0.0, dv, du),
            Vec3::new(0.0, dv, -du),
            Vec3::new(0.0, -dv, du),
            Vec3::new(0.0, -dv, -du),
            Vec3::new(du, 0.0, dv),
            Vec3::new(-du, 0.0, dv),
            Vec3::new(du, 0.0, -dv),
            Vec3::new(-du, 0.0, -dv),
            Vec3::new(dv, du, 0.0),
            Vec3::new(dv, -du, 0.0),
            Vec3::new(-dv, du, 0.0),
            Vec3::new(-dv, -du, 0.0),
        ];

        let faces: Vec<[usize; 3]> = vec![
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

        let mut cells = BTreeMap::new();
        for (i, face) in faces.iter().enumerate() {
            for &v in face {
                cells.entry(v).or_insert_with(BTreeSet::new).insert(i);
            }
        }
        let cells: Vec<_> = cells
            .into_values()
            .map(|f| f.into_iter().collect())
            .collect();

        let mut cell_neighbors = vec![BTreeSet::default(); cells.len()];
        for face in &faces {
            cell_neighbors[face[0]].insert(face[1]);
            cell_neighbors[face[1]].insert(face[2]);
            cell_neighbors[face[2]].insert(face[0]);
        }

        let chunks = (0..vertices.len()).collect();

        GeometryData {
            vertices,
            faces,
            cells,
            cell_neighbors,
            chunks,
        }
    }

    // Returns the centroid of each cell
    fn cell_centroids(&self) -> Vec<Vec3> {
        self.cells
            .iter()
            .map(|fs| {
                let mut cent = Vec3::ZERO;
                for f in fs {
                    let mut avg = Vec3::ZERO;
                    for v in self.faces[*f] {
                        avg += self.vertices[v];
                    }
                    cent += avg / 3.0;
                }
                cent
            })
            .collect()
    }

    // Returns the normal for each vertex
    // assumes that vertex duplication has been done otherwise results are wierd
    fn flat_normals(&self) -> Vec<Vec3> {
        let centroids = self.cell_centroids();
        let mut normals = vec![Vec3::ZERO; self.vertices.len()];
        for (ci, cell) in self.cells.iter().enumerate() {
            let r = -0.2..0.2;
            let x = random_range(r.clone());
            let y = random_range(r.clone());
            let z = random_range(r);
            for face in cell.iter().map(|c| self.faces[*c]) {
                for v in face {
                    normals[v] = (centroids[ci] + Vec3::new(x, y, z)).normalize();
                }
            }
        }
        normals
    }

    fn mesh(&self) -> Mesh {
        let len = self.vertices.len();
        Mesh::new(
            TriangleList,
            RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
        )
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, self.vertices.clone())
        .with_inserted_indices(Indices::U32(
            self.faces.iter().flatten().map(|&f| f as u32).collect(),
        ))
        .with_inserted_attribute(
            Mesh::ATTRIBUTE_COLOR,
            vec![[random(), random(), random(), 1.0]; len],
        )
        .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, self.flat_normals())
    }
}

pub(crate) fn setup_demo_sphere(
    mut flat_materials: ResMut<Assets<ExtendedMaterial<StandardMaterial, FlatNormalMaterial>>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut commands: Commands,
) {
    let geom = GeometryData::icosahedron()
        .subdivide_n(2)
        .slerp()
        .recell()
        .dual()
        .duplicate();

    // let indices = (0..(geom.cells.len())).collect();

    // let chunker = FloodfillChunker {
    //     min_size: 25,
    //     max_chunks: 5,
    // };

    // let chunk = Chunk::build(indices, &geom, &chunker);
    // let d1 = chunk.depth(1);
    // for item in &d1 {
    //     // dbg!(item);
    // }
    // let mut m: Vec<_> = d1.iter().map(|c| c.local_geometry(&geom).mesh()).collect();

    // println!("DONE!");

    // for m in m {
    commands.spawn((
        Wireframeable,
        Mesh3d(meshes.add(geom.mesh())),
        Transform::IDENTITY.with_scale(Vec3::new(16.0, 16.0, 16.0)),
        // .with_translation(Vec3::new(random(), random(), random())),
        MeshMaterial3d(flat_materials.add(ExtendedMaterial {
            base: StandardMaterial {
                opaque_render_method: OpaqueRendererMethod::Auto,
                ..Default::default()
            },
            extension: FlatNormalMaterial {},
        })),
    ));
    // }
}
