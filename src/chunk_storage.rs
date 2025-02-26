use crate::{
    camera::CameraTarget,
    chunking::HexsphereMaterial,
    colors::{HexColors, NeedsColoring},
    flatnormal::{FlatNormalMaterial, ATTRIBUTE_BLEND_COLOR},
    geometry_data::GeometryData,
    octree::Octree,
    Wireframeable,
};
use bevy::{
    pbr::{ExtendedMaterial, OpaqueRendererMethod},
    prelude::*,
    tasks::{block_on, futures_lite::future, AsyncComputeTaskPool, Task},
};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

pub type ChunkIndex = Vec<u8>;

#[derive(Component)]
pub struct Body {
    pub geometry: Arc<GeometryData>,
    pub octree: Arc<Octree>,
}

impl Body {
    pub fn new(geometry: GeometryData) -> Self {
        let capacity = 16;
        let bounds = 1.0;
        let center = Vec3::ZERO;

        let mut octree = Octree::new(capacity, center, bounds, 0, vec![]);
        // Insert geometry points into the octree
        for (cell_index, &position) in geometry.cell_normals.iter().enumerate() {
            octree.insert(crate::octree::Point {
                position,
                value: cell_index,
            });
        }

        Self {
            geometry: Arc::new(geometry),
            octree: Arc::new(octree),
        }
    }
}

#[derive(Default)]
pub struct ChunkData {
    pub mesh_handle: Option<Handle<Mesh>>,
    pub entity: Option<Entity>,
    pub cells: Option<Vec<usize>>,
    pub cells_to_vert: Option<BTreeMap<usize, usize>>,
}

#[derive(Component)]
pub struct ChunkCells {
    pub cells: Option<BTreeSet<usize>>,
    pub cells_to_local: Option<BTreeMap<usize, usize>>,
    pub local_geometry: Option<GeometryData>,
}

#[derive(Component, Default)]
pub struct ChunkStorage(pub BTreeMap<ChunkIndex, ChunkData>);

#[derive(Component, Default)]
pub struct ChunkRefs(pub BTreeMap<ChunkIndex, ChunkRef>);

#[derive(Clone)]
pub enum ChunkRef {
    Active(Entity),
    Cleanup(Entity),
}

#[derive(Component, Default)]
pub struct POV(pub Vec3, pub f32);

#[derive(Component)]
pub struct Chunk {
    pub body: Entity,
    pub index: ChunkIndex,
}

#[derive(Component, Default)]
#[component(storage = "SparseSet")]
pub struct NeedsMesh;

#[derive(Component)]
#[component(storage = "SparseSet")]
pub struct GeneratingMesh(
    pub Task<Option<(Vec<usize>, GeometryData, BTreeMap<usize, usize>, Mesh)>>,
);

#[derive(Component, Default)]
#[component(storage = "SparseSet")]
pub struct AwaitingDeletion(Vec<ChunkIndex>);

pub struct ChunkingPlugin;

impl Plugin for ChunkingPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, setup_bodies).add_systems(
            FixedUpdate,
            (
                calculate_povs,
                despawn_chunks.after(spawn_ready_chunks),
                generate_meshes.after(calculate_povs),
                poll_mesh_tasks.after(generate_meshes),
                spawn_ready_chunks.after(poll_mesh_tasks),
            ),
        );
    }
}

fn create_material(
    flat_materials: &mut ResMut<Assets<ExtendedMaterial<StandardMaterial, FlatNormalMaterial>>>,
) -> Handle<ExtendedMaterial<StandardMaterial, FlatNormalMaterial>> {
    let extended_material = ExtendedMaterial {
        base: StandardMaterial {
            opaque_render_method: OpaqueRendererMethod::Auto,
            ..Default::default()
        },
        extension: FlatNormalMaterial {},
    };
    flat_materials.add(extended_material)
}

fn calculate_povs(
    mut commands: Commands,
    mut pov_query: Query<(&Transform, &mut POV, &Projection)>,
    mut body_query: Query<(Entity, &Body, &mut ChunkRefs, &Transform)>,
) {
    let Ok((camera_transform, mut pov, projection)) = pov_query.get_single_mut() else {
        return;
    };

    let Projection::Perspective(persp) = projection else {
        return;
    };

    if pov.0.distance_squared(camera_transform.translation) < 0.0001
        && (pov.1 - persp.fov).abs() < 0.0001
    {
        return;
    }

    pov.0 = camera_transform.translation;
    pov.1 = persp.fov;

    for (body_entity, body, mut chunk_refs, transform) in body_query.iter_mut() {
        let cell_count = body.geometry.cells.len();
        let needed_indices = body.octree.get_chunk_indices(
            cell_count,
            (camera_transform.translation - transform.translation).normalize(),
            persp.fov.sqrt(),
        );
        let needed_indices: BTreeSet<_> = needed_indices.into_iter().collect();

        let existing_set: BTreeSet<_> = chunk_refs.0.keys().cloned().collect();

        for index in &needed_indices {
            let entity = match chunk_refs.0.get(index) {
                Some(ChunkRef::Active(entity)) => *entity,
                Some(ChunkRef::Cleanup(entity)) => {
                    commands
                        .entity(*entity)
                        .remove::<AwaitingDeletion>()
                        .remove::<GeneratingMesh>()
                        .insert(NeedsMesh);
                    *entity
                }
                None => commands
                    .spawn((
                        Chunk {
                            body: body_entity,
                            index: index.clone(),
                        },
                        NeedsMesh,
                        Name::new(format!("Chunk {:?}", index)),
                    ))
                    .id(),
            };
            chunk_refs.0.insert(index.clone(), ChunkRef::Active(entity));
        }

        let obsolete_indices = existing_set
            .difference(&needed_indices)
            .cloned()
            .collect::<Vec<_>>();

        // Any chunk is potentially being replaced by several others,
        // we mark those in the replacing map
        let mut replacing = BTreeMap::<ChunkIndex, Vec<ChunkIndex>>::new();

        for index in &needed_indices {
            // Crawl up, see if there is any obsolete parent
            for i in (0..index.len()).rev() {
                let parent_index = index[0..i].to_vec();
                // if the parent chunk exists already and is obsolete
                if obsolete_indices.contains(&parent_index) {
                    replacing
                        .entry(parent_index)
                        .or_insert_with(Vec::new)
                        .push(index.clone());
                }
            }
        }
        for index in &obsolete_indices {
            // Crawl up, see if there is any brand new parent
            for i in (0..index.len()).rev() {
                let parent_index = index[0..i].to_vec();
                // There is a parent that is currently needed!
                if needed_indices.contains(&parent_index) {
                    replacing
                        .entry(index.clone())
                        .or_insert_with(Vec::new)
                        .push(parent_index);
                    break;
                }
            }
        }

        for index in obsolete_indices {
            match chunk_refs.0.get(&index).cloned() {
                Some(ChunkRef::Active(entity)) => {
                    // Switch from Active -> Cleanup
                    chunk_refs
                        .0
                        .insert(index.clone(), ChunkRef::Cleanup(entity));
                    commands
                        .entity(entity)
                        .insert(AwaitingDeletion(
                            replacing.remove(&index).unwrap_or_default(),
                        ))
                        .remove::<NeedsMesh>()
                        .remove::<GeneratingMesh>();
                }
                Some(ChunkRef::Cleanup(entity)) => {
                    // The thing was already being cleaned up, but it might now have a new set of things
                    // it is depending on
                    commands
                        .entity(entity)
                        .insert(AwaitingDeletion(
                            replacing.remove(&index).unwrap_or_default(),
                        ))
                        .remove::<NeedsMesh>()
                        .remove::<GeneratingMesh>();
                }
                None => todo!(),
            }
        }
    }
}

pub(crate) fn despawn_chunks(
    mut commands: Commands,
    chunk_query: Query<(Entity, &Chunk, &AwaitingDeletion)>,
    has_mesh: Query<Option<&Mesh3d>>,
    mut body_query: Query<(&mut ChunkRefs, &mut ChunkStorage)>,
) {
    // If all the things that replaced us (potentially 1) have meshes,
    // or they no longer exist, then we can delete ourself.
    // This way, chunks never despawn and leave empty loading holes.

    for (chunk_entity, chunk, AwaitingDeletion(pending)) in chunk_query.iter() {
        let Ok((mut chunk_refs, mut storage)) = body_query.get_mut(chunk.body) else {
            commands.entity(chunk_entity).despawn_recursive();
            continue;
        };

        let mut can_delete = true;
        for index in pending {
            let cr = match chunk_refs.0.get(index) {
                Some(ChunkRef::Active(cr)) => cr,
                Some(ChunkRef::Cleanup(cr)) => cr,
                None => continue,
            };
            if let Ok(None) = has_mesh.get(*cr) {
                can_delete = false;
                break;
            }
        }

        if can_delete {
            chunk_refs.0.remove(&chunk.index);

            storage.0.remove(&chunk.index);

            commands.entity(chunk_entity).despawn_recursive();
        }
    }
}

fn generate_meshes(
    mut commands: Commands,
    query: Query<(Entity, &Chunk), (With<NeedsMesh>, Without<GeneratingMesh>)>,
    has_mesh: Query<(), With<Mesh3d>>,
    generating: Query<(), With<GeneratingMesh>>,
    body_query: Query<(&Body, &ChunkStorage)>,
) {
    let mut i = generating.iter().len();

    let thread_pool = AsyncComputeTaskPool::get(); // or use bevy's default

    for (chunk_entity, chunk) in query.iter() {
        if has_mesh.get(chunk_entity).is_ok() {
            commands.entity(chunk_entity).remove::<NeedsMesh>();
            continue;
        }
        if i > 256 {
            return;
        }
        // Look up the body to get geometry / octree
        let Ok((body, storage)) = body_query.get(chunk.body) else {
            continue; // or handle error
        };

        // If we already have a mesh in storage, no need to generate again
        if let Some(chunk_data) = storage.0.get(&chunk.index) {
            if chunk_data.mesh_handle.is_some() {
                continue;
            }
        }

        let index_clone = chunk.index.clone();
        let geometry = body.geometry.clone();
        let octree = body.octree.clone();

        let task = thread_pool.spawn(async move {
            let Some(cells) = octree.get_cells_for_index(&index_clone) else {
                return None;
            };

            let (mut local_geometry, mut cell_map) = geometry.sub_geometry(&cells);
            if local_geometry.cells.len() > 256 {
                local_geometry = local_geometry.simplify();
                for v in cell_map.values_mut() {
                    // all original cells point into the ONE simple cell
                    *v = 0;
                }
            } else {
                local_geometry = local_geometry.duplicate();
            }
            let mut mesh = local_geometry.mesh();
            mesh.insert_attribute(
                ATTRIBUTE_BLEND_COLOR,
                vec![[1.0, 0.0, 0.0, 1.0]; local_geometry.vertices.len()],
            );

            Some((cells, local_geometry, cell_map, mesh))
        });

        commands
            .entity(chunk_entity)
            // .remove::<NeedsMesh>()
            .insert(GeneratingMesh(task));
        i += 1;
    }
}

fn poll_mesh_tasks(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut query: Query<(Entity, &Chunk, &mut GeneratingMesh)>,
    mut body_query: Query<&mut ChunkStorage>,
) {
    for (chunk_entity, chunk, mut gen_mesh) in query.iter_mut() {
        if !gen_mesh.0.is_finished() {
            continue;
        }
        if let Some(Some((cells, local_geometry, cells_to_local, mesh))) =
            block_on(future::poll_once(&mut gen_mesh.0))
        {
            let index = chunk.index.clone();
            if let Ok(mut storage) = body_query.get_mut(chunk.body) {
                let entry = storage.0.entry(index).or_default();
                entry.cells = Some(cells);
                entry.mesh_handle = Some(meshes.add(mesh));
                commands.entity(chunk_entity).insert(ChunkCells {
                    cells: entry.cells.clone().map(|i| i.into_iter().collect()),
                    cells_to_local: Some(cells_to_local),
                    local_geometry: Some(local_geometry),
                });
            }
        }
        commands.entity(chunk_entity).remove::<GeneratingMesh>();
    }
}

pub fn spawn_ready_chunks(
    mut commands: Commands,
    mut body_query: Query<&mut ChunkStorage>,
    chunk_query: Query<(Entity, &Chunk), (With<NeedsMesh>, Without<GeneratingMesh>)>,
    material: Res<HexsphereMaterial>,
) {
    for (chunk_entity, chunk) in chunk_query.iter() {
        let Ok(mut storage) = body_query.get_mut(chunk.body) else {
            continue;
        };

        if let Some(chunk_data) = storage.0.get(&chunk.index) {
            if chunk_data.entity.is_some() {
                continue;
            }

            if let Some(mesh_handle) = &chunk_data.mesh_handle {
                commands.get_entity(chunk_entity).map(|mut e| {
                    e.insert((
                        Mesh3d(mesh_handle.clone()),
                        MeshMaterial3d(material.0.clone()),
                        Transform::from_scale(Vec3::splat(32.0)),
                        Wireframeable,
                        NeedsColoring,
                    ))
                    .remove::<NeedsMesh>();
                });
            }
            storage.0.remove(&chunk.index);
        }
    }
}

fn setup_bodies(
    mut commands: Commands,
    mut flat_materials: ResMut<Assets<ExtendedMaterial<StandardMaterial, FlatNormalMaterial>>>,
) {
    let geom = crate::geometry_data::GeometryData::icosahedron()
        .subdivide_n(8)
        .slerp()
        .recell()
        .dual();

    let body = Body::new(geom);

    commands.spawn((
        HexColors {
            colors: vec![Color::srgba(1.0, 0.0, 0.0, 1.0); body.geometry.cells.len()],
            ..Default::default()
        },
        body,
        ChunkStorage::default(),
        ChunkRefs::default(),
        Name::new("Planet"),
        Transform::default()
            .with_translation(Vec3::ZERO)
            .with_scale(Vec3::splat(32.)),
        CameraTarget { radius: 32.0 },
    ));

    let material = create_material(&mut flat_materials);
    commands.insert_resource(HexsphereMaterial(material));
}
