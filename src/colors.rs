use std::collections::BTreeMap;

use bevy::{
    pbr::ExtendedMaterial, prelude::*, render::mesh::VertexAttributeValues,
    utils::tracing::instrument::WithSubscriber,
};
use rand::{random_range, seq::index};

use crate::{
    flatnormal::FlatNormalMaterial,
    goldberg::GoldbergPoly,
    surface::{ChunkSizeLimit, Surface},
};

/// Represents a planets hex colours
#[derive(Component, Default)]
pub(crate) struct HexColors {
    // The color of each cell
    pub(crate) colors: Vec<Color>,
    // A list of indices into changed cells
    pub(crate) changed: Vec<usize>,
}

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
        let samples = index::sample(&mut rng, colors.colors.len(), 250);

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
            colors.changed.push(sample);
        }
    }
}

pub(crate) fn update_mesh_colors(
    mut commands: Commands,
    // mut materials: ResMut<Assets<ExtendedMaterial<StandardMaterial, FlatNormalMaterial>>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut hexes: Query<(&mut HexColors, &Surface, &ChunkSizeLimit)>,
    mut mesh_handles: Query<&Mesh3d>,
) {
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
