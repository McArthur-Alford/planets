/// Code source:
/// https://github.com/DGriffin91/bevy_glowy_orb_tutorial/blob/flat_normal_material/src/main.rs
use bevy::{
    app::Plugin,
    asset::Asset,
    color::LinearRgba,
    pbr::{
        ExtendedMaterial, MaterialExtension, MaterialExtensionKey, MaterialExtensionPipeline,
        MaterialPlugin, StandardMaterial,
    },
    prelude::*,
    reflect::TypePath,
    render::{
        self,
        mesh::{MeshVertexAttribute, MeshVertexBufferLayoutRef},
        render_resource::{
            self, AsBindGroup, RenderPipelineDescriptor, ShaderRef, SpecializedMeshPipelineError,
            VertexFormat,
        },
    },
};

#[derive(Asset, TypePath, AsBindGroup, Debug, Clone)]
pub(crate) struct FlatNormalMaterial {}

// https://github.com/bevyengine/bevy/blob/v0.14.2/examples/shader/extended_material.rs

pub const ATTRIBUTE_BLEND_COLOR: MeshVertexAttribute =
    MeshVertexAttribute::new("BlendColor", 988540917, VertexFormat::Float32x4);

impl MaterialExtension for FlatNormalMaterial {
    fn fragment_shader() -> ShaderRef {
        "flat_normal_material.wgsl".into()
    }

    fn vertex_shader() -> ShaderRef {
        "flat_normal_material.wgsl".into()
    }

    fn specialize(
        pipeline: &MaterialExtensionPipeline,
        descriptor: &mut RenderPipelineDescriptor,
        layout: &MeshVertexBufferLayoutRef,
        key: MaterialExtensionKey<Self>,
    ) -> Result<(), SpecializedMeshPipelineError> {
        let vertex_layout = layout.0.get_layout(&[
            Mesh::ATTRIBUTE_POSITION.at_shader_location(0),
            Mesh::ATTRIBUTE_NORMAL.at_shader_location(1),
            Mesh::ATTRIBUTE_COLOR.at_shader_location(5),
            ATTRIBUTE_BLEND_COLOR.at_shader_location(10),
        ])?;
        descriptor.vertex.buffers = vec![vertex_layout];
        Ok(())
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
