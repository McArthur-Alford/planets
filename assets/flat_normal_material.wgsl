/// Code source:
/// https://github.com/DGriffin91/bevy_glowy_orb_tutorial/blob/flat_normal_material/assets/flat_normal_material.wgsl

// https://github.com/bevyengine/bevy/blob/v0.14.2/assets/shaders/extended_material.wgsl

#import bevy_pbr::{
    mesh_functions,
    view_transformations::position_world_to_clip,
    pbr_fragment::pbr_input_from_standard_material,
    pbr_functions::alpha_discard,
}

#ifdef PREPASS_PIPELINE
#import bevy_pbr::{
    prepass_io::{VertexOutput, FragmentOutput},
    pbr_deferred_functions::deferred_output,
}
#else
#import bevy_pbr::{
    forward_io::{VertexOutput, FragmentOutput},
    pbr_functions::{apply_pbr_lighting, main_pass_post_lighting_processing},
}
#endif

struct Vertex {
    @builtin(instance_index) instance_index: u32,
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(5) color: vec4<f32>,
    @location(10) blend_color: vec4<f32>,
};

@vertex
fn vertex(
    in: Vertex,
    @builtin(vertex_index) index: u32,
) -> VertexOutput {
    var out: VertexOutput;

    let position = vec4<f32>(
        in.position,
        1.0
    );

    var world_from_local = mesh_functions::get_world_from_local(in.instance_index);
    out.world_position = mesh_functions::mesh_position_local_to_world(world_from_local, position);
    out.position = position_world_to_clip(out.world_position.xyz);

#ifdef MESH_PIPELINE
    out.world_normal = mesh_functions::mesh_normal_local_to_world(
        in.normal,
        in.instance_index
    );
#endif

#ifdef VERTEX_COLORS
    out.color = in.color;
#endif
   
    return out;
}

@fragment
fn fragment(
    in: VertexOutput,
    @builtin(front_facing) is_front: bool,
) -> FragmentOutput {
    // generate a PbrInput struct from the StandardMaterial bindings
    var pbr_input = pbr_input_from_standard_material(in, is_front);

    // // Compute flat normals from world space surface gradients.
    // pbr_input.N = normalize(cross(dpdy(in.world_position.xyz), dpdx(in.world_position.xyz)));
    // pbr_input.world_normal = pbr_input.N;
    //
    // Or... just normala colours :)
    pbr_input.material.base_color = in.color;

    // we can optionally modify the input before lighting and alpha_discard is applied
    // pbr_input.material.base_color.b = pbr_input.material.base_color.r;

    // alpha discard
    pbr_input.material.base_color = alpha_discard(pbr_input.material, pbr_input.material.base_color);

#ifdef PREPASS_PIPELINE
    // in deferred mode we can't modify anything after that, as lighting is run in a separate fullscreen shader.
    let out = deferred_output(in, pbr_input);
#else
    var out: FragmentOutput;
    // apply lighting
    out.color = apply_pbr_lighting(pbr_input);

    // apply in-shader post processing (fog, alpha-premultiply, and also tonemapping, debanding if the camera is non-hdr)
    // note this does not include fullscreen postprocessing effects like bloom.
    out.color = main_pass_post_lighting_processing(pbr_input, out.color*0.4);

    // out.color = vec4(pbr_input.N, 1.0); // Render Normals
    // out.color = in.color;
#endif

    return out;
}
