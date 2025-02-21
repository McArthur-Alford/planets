use crate::{
    camera::CameraTarget, chunking::HexsphereMaterial, flatnormal::FlatNormalMaterial,
    geometry_data::GeometryData, octree::Octree,
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
pub struct POV(pub Vec3);

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
pub struct GeneratingMesh(pub Task<Option<(Vec<usize>, Mesh)>>);

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

fn setup_bodies(
    mut commands: Commands,
    mut flat_materials: ResMut<Assets<ExtendedMaterial<StandardMaterial, FlatNormalMaterial>>>,
) {
    let geom = crate::geometry_data::GeometryData::icosahedron()
        .subdivide_n(9)
        .slerp()
        .recell()
        .dual()
        .duplicate();

    let body = Body::new(geom);

    commands.spawn((Transform::IDENTITY, CameraTarget { radius: 32.0 }));
    commands.spawn((
        body,
        ChunkStorage::default(),
        ChunkRefs::default(),
        Name::new("Planet"),
        Transform::default().with_translation(Vec3::ZERO),
    ));

    let material = create_material(&mut flat_materials);
    commands.insert_resource(HexsphereMaterial(material));
}

fn calculate_povs(
    mut commands: Commands,
    mut pov_query: Query<(&Transform, &mut POV, &Projection)>,
    mut body_query: Query<(Entity, &Body, &mut ChunkRefs)>,
) {
    if pov_query.is_empty() {
        return;
    }

    let (pov_transform, mut pov, projection) = pov_query.single_mut();
    if (pov.0 - pov_transform.translation).length() < 0.01 {
        return;
    }
    let Projection::Perspective(projection) = projection else {
        return;
    };
    pov.0 = pov_transform.translation.clone();

    for (body_entity, body, mut chunk_refs) in body_query.iter_mut() {
        let pov = pov_transform.translation.normalize()
            + pov_transform.translation.normalize() * projection.fov / 5.0;

        let needed_indices = body.octree.get_chunk_indices(pov);

        let needed_set: BTreeSet<ChunkIndex> = needed_indices.into_iter().collect();

        let existing_set: BTreeSet<ChunkIndex> = chunk_refs.0.keys().cloned().collect();

        let mut awaiting: BTreeMap<ChunkIndex, Vec<ChunkIndex>> = BTreeMap::new();
        for obsolete_index in existing_set.difference(&needed_set) {
            // crawl up the needed set to find any potential parent of this node
            // note that node is *our* parent
            for i in (0..obsolete_index.len()).rev() {
                let key: ChunkIndex = obsolete_index[0..=i].to_vec();
                if needed_set.contains(&key) && !existing_set.contains(&key) {
                    awaiting
                        .entry(obsolete_index.clone())
                        .or_insert_with(Vec::new)
                        .push(key);
                    break;
                }
            }
        }

        for missing_index in &needed_set {
            let entry = chunk_refs
                .0
                .entry(missing_index.clone())
                .or_insert_with(|| {
                    ChunkRef::Active(
                        commands
                            .spawn((
                                Chunk {
                                    body: body_entity,
                                    index: missing_index.clone(),
                                },
                                NeedsMesh, // Mark that we must generate a mesh
                                Name::new(format!("Chunk {:?}", missing_index)),
                            ))
                            .id(),
                    )
                });

            *entry = match *entry {
                ChunkRef::Active(entity) => ChunkRef::Active(entity),
                ChunkRef::Cleanup(entity) => {
                    // it was already being cleaned up but got reinstated
                    // so remove the awaiting deletion + pending set from it
                    commands
                        .entity(entity)
                        .remove::<AwaitingDeletion>()
                        .remove::<GeneratingMesh>()
                        .insert(NeedsMesh);
                    ChunkRef::Active(entity)
                }
            };

            // also, crawl up the existing set to find any potential parent of this node
            // Note that nodes children include us
            for i in (0..missing_index.len()).rev() {
                let key: ChunkIndex = missing_index[0..i].to_vec();
                let Some(value) = existing_set.get(&key) else {
                    continue;
                };
                if !chunk_refs
                    .0
                    .get(value)
                    .is_none_or(|x| matches!(x, ChunkRef::Cleanup(_)))
                    && !needed_set.contains(&key)
                {
                    awaiting
                        .entry(key)
                        .or_insert_with(Vec::new)
                        .push(missing_index.clone());
                    break;
                }
            }
        }

        for obsolete_index in existing_set.difference(&needed_set) {
            if let Some(chunk_entity) = chunk_refs.0.get(obsolete_index).cloned() {
                let ChunkRef::Active(entity) = chunk_entity else {
                    continue;
                };
                chunk_refs
                    .0
                    .insert(obsolete_index.clone(), ChunkRef::Cleanup(entity));
                commands
                    .entity(entity)
                    .insert(AwaitingDeletion(
                        awaiting.remove(obsolete_index).unwrap_or_default(),
                    ))
                    .remove::<NeedsMesh>()
                    .remove::<GeneratingMesh>();
            }
        }
    }
}

fn despawn_chunks(
    mut commands: Commands,
    chunk_query: Query<(Entity, &Chunk, &AwaitingDeletion)>,
    needs_mesh: Query<Option<&Mesh3d>>,
    mut body_query: Query<(&mut ChunkRefs, &mut ChunkStorage)>,
) {
    // If all the things that replaced us (potentially 1) have meshes,
    // or they no longer exist, then we can delete ourself.
    // This way, chunks never despawn and leave empty loading holes.

    for (chunk_entity, chunk, AwaitingDeletion(pending)) in chunk_query.iter().take(100) {
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
            if let Ok(None) = needs_mesh.get(*cr) {
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
        if i > 60 {
            return;
        }
        // Look up the body to get geometry / octree
        let Ok((body, storage)) = body_query.get(chunk.body) else {
            continue; // or handle error
        };

        // If we already have a mesh in storage, no need to generate again
        // if let Some(chunk_data) = storage.0.get(&chunk.index) {
        //     if chunk_data.mesh_handle.is_some() {
        //         // Already has mesh, remove `NeedsMesh`.
        //         commands.entity(chunk_entity).remove::<NeedsMesh>();
        //         continue;
        //     }
        // }

        let index_clone = chunk.index.clone();
        let geometry = body.geometry.clone();
        let octree = body.octree.clone();

        let task = thread_pool.spawn(async move {
            let Some(cells) = octree.get_cells_for_index(&index_clone) else {
                return None;
            };

            let local_geometry = geometry.sub_geometry(&cells);
            let mesh = local_geometry.mesh();

            Some((cells, mesh))
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
        if let Some(Some((cells, mesh))) = block_on(future::poll_once(&mut gen_mesh.0)) {
            let index = chunk.index.clone();
            if let Ok(mut storage) = body_query.get_mut(chunk.body) {
                let entry = storage.0.entry(index).or_default();
                entry.cells = Some(cells);
                entry.mesh_handle = Some(meshes.add(mesh));
            }
        }
        commands.entity(chunk_entity).remove::<GeneratingMesh>();
    }
}

fn spawn_ready_chunks(
    mut commands: Commands,
    mut body_query: Query<&mut ChunkStorage>,
    chunk_query: Query<(Entity, &Chunk), (With<NeedsMesh>, Without<GeneratingMesh>)>,
    material: Res<HexsphereMaterial>,
) {
    for (chunk_entity, chunk) in chunk_query.iter().take(100) {
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
                    ))
                    .remove::<NeedsMesh>();
                });
            }
        }
        storage.0.remove(&chunk.index);
    }
}
