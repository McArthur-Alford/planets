/// Code source:
/// https://github.com/DGriffin91/bevy_glowy_orb_tutorial/blob/flat_normal_material/src/main.rs
use bevy::{
    app::Plugin,
    asset::Asset,
    pbr::{ExtendedMaterial, MaterialExtension, MaterialPlugin, StandardMaterial},
    reflect::TypePath,
    render::render_resource::{AsBindGroup, ShaderRef},
};

#[derive(Asset, TypePath, AsBindGroup, Debug, Clone)]
pub(crate) struct FlatNormalMaterial {}

// https://github.com/bevyengine/bevy/blob/v0.14.2/examples/shader/extended_material.rs
impl MaterialExtension for FlatNormalMaterial {
    fn fragment_shader() -> ShaderRef {
        "flat_normal_material.wgsl".into()
    }
}

pub(crate) struct FlatNormalMaterialPlugin;

impl Plugin for FlatNormalMaterialPlugin {
    fn build(&self, app: &mut bevy::prelude::App) {
        app.add_plugins(MaterialPlugin::<
            ExtendedMaterial<StandardMaterial, FlatNormalMaterial>,
        >::default());
    }
}
