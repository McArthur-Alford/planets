use std::{
    collections::{BTreeMap, BTreeSet},
    time::{Duration, Instant},
};

use bevy::{
    pbr::ExtendedMaterial, prelude::*, render::mesh::VertexAttributeValues,
    utils::tracing::instrument::WithSubscriber,
};
use rand::{random_range, seq::index};

use crate::{
    chunk_storage::{Body, Chunk, ChunkCells},
    flatnormal::FlatNormalMaterial,
};

/// Represents a planets hex colours
#[derive(Component, Default)]
pub(crate) struct HexColors {
    // The color of each cell
    pub(crate) colors: Vec<Color>,
    // A list of indices into changed cells
    pub(crate) changed: BTreeSet<usize>,
}

#[derive(Component)]
pub(crate) struct NeedsColoring;

pub(crate) fn randomize_colors(mut hexes: Query<(&mut HexColors)>) {
    // Pick a handful of random hexes
    // add them to the changed list, and update the color to be random

    // let dark = [0.57, 0.43, 0.20, 1.0];
    // let bright = [0.98, 0.95, 0.59, 1.0];

    // let dark = [1.0, 0.0, 0.0, 1.0];
    // let bright = [0.0, 1.0, 0.0, 1.0];

    let dark = [1.0, 0.0, 1.0, 1.0];
    let bright = [0.0, 1.0, 1.0, 1.0];

    let mut rng = rand::rng();
    for mut colors in hexes.iter_mut() {
        let samples = index::sample(&mut rng, colors.colors.len(), 10000);

        for sample in samples {
            let t = random_range(0.0..=1.0f32).powi(2);

            // let t = (noisy_bevy::simplex_noise_3d(Vec3::from(*hex) * 4.0) + 1.0) / 2.0;

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

            colors.colors[sample] = Color::from(LinearRgba::from_f32_array([r, g, b, a]));
            colors.changed.insert(sample);
        }
    }
}

#[derive(Component)]
pub struct ColorCooldown(Timer);

pub(crate) fn update_mesh_colors(
    mut commands: Commands,
    // mut materials: ResMut<Assets<ExtendedMaterial<StandardMaterial, FlatNormalMaterial>>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut hexes: Query<(&mut HexColors, &Body)>,
    mut chunks: Query<(
        Entity,
        &Chunk,
        &Mesh3d,
        &ChunkCells,
        Option<&NeedsColoring>,
        Option<&mut ColorCooldown>,
    )>,
) {
    let time = Instant::now();
    for (entity, chunk, mesh3d, chunk_cells, needs_coloring, mut color_cooldown) in
        chunks.iter_mut()
    {
        if let Some(timer) = &mut color_cooldown {
            if !timer.0.finished() {
                continue;
            } else {
                timer.0.reset();
                timer.0.unpause();
            }
        } else {
            commands.entity(entity).insert(ColorCooldown(Timer::new(
                Duration::from_millis(1000),
                TimerMode::Once,
            )));
        }

        if Instant::now().duration_since(time) > Duration::from_millis(3) {
            return;
        }
        let Ok((hex_colors, _body)) = hexes.get_mut(chunk.body) else {
            continue;
        };
        let ChunkCells {
            cells: Some(cells),
            cells_to_local: Some(cells_to_local),
            local_geometry: Some(local_geometry),
        } = chunk_cells
        else {
            continue;
        };

        // TODO cache this instead of recalcing for each chunk pls
        let HexColors { colors, changed } = hex_colors.into_inner();
        let intersection: Vec<usize> = changed.intersection(cells).into_iter().copied().collect();

        if (intersection.len() as f32) < (0.75 * local_geometry.cells.len() as f32)
            && needs_coloring.is_none()
        {
            continue;
        }
        let handle = mesh3d.0.clone_weak();
        let Some(mesh) = meshes.get_mut(&handle) else {
            continue;
        };

        // Gather the colors of the chunk
        let mut new_colors = Vec::new();
        let mut seen = BTreeSet::new();
        for cell in cells {
            if !seen.insert(cells_to_local[cell]) {
                continue;
            }
            let color = colors[*cell].to_linear().to_f32_array();
            let local_cell = cells_to_local[cell];
            let faces = &local_geometry.cells[local_cell];

            let mut seen_verts = BTreeSet::new();
            for f in faces {
                for v in local_geometry.faces[*f] {
                    if !seen_verts.insert(v) {
                        continue;
                    };
                    new_colors.push(color);
                }
            }
        }

        mesh.insert_attribute(
            Mesh::ATTRIBUTE_COLOR,
            VertexAttributeValues::Float32x4(new_colors),
        );

        for i in intersection {
            changed.remove(&i);
        }

        commands.entity(entity).remove::<NeedsColoring>();
    }

    // for (mut hex_colors, surface, limit) in hexes.iter_mut() {
    //     if hex_colors.changed.len() <= 500 {
    //         // We only do the update if enough meshes are changed,
    //         // to avoid updates as much as possible
    //         continue;
    //     }

    //     let mut chunks = BTreeMap::<usize, Vec<usize>>::new();
    //     for changed in &hex_colors.changed {
    //         chunks
    //             .entry(surface.cell_to_chunk[*changed])
    //             .or_insert_with(Vec::new)
    //             .push(*changed);
    //     }

    //     for (chunk, cells) in chunks {
    //         if cells.len() < limit.0 / 4 {
    //             // Skip if its < a quarter of the chunk to save performance
    //             continue;
    //         }

    //         let Some(mesh_handle) = surface.chunks[chunk].mesh else {
    //             continue;
    //         };

    //         let mesh_handle = mesh_handles.get(mesh_handle).unwrap();
    //         let mesh = meshes.get_mut(mesh_handle).unwrap();

    //         let colors = mesh
    //             .attribute_mut(Mesh::ATTRIBUTE_COLOR)
    //             .expect("Mesh should have colors attribute");

    //         let VertexAttributeValues::Float32x4(colors) = colors else {
    //             continue;
    //         };

    //         // while let Some(h) = hex_colors.changed.pop() {
    //         //     let c = hex_colors.colors[h];

    //         //     // Update hex h to have color c
    //         //     // AKA update all vertices of hex h to have color c
    //         //     // AKA find the indices of all vertices
    //         //     // faces[hex_to_face[h]][0..3]
    //         //     for &f in &gold.hex_to_face[h] {
    //         //         for &v in &gold.faces[f as usize] {
    //         //             colors[v as usize] = c.to_linear().to_f32_array();
    //         //         }
    //         //     }
    //         // }
    //         // mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, values);
    //     }
    // }
}
