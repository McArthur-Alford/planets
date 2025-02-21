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
    let Ok((camera_transform, mut pov, projection)) = pov_query.get_single_mut() else {
        return;
    };

    if pov.0.distance_squared(camera_transform.translation) < 0.0001 {
        return;
    }

    let Projection::Perspective(persp) = projection else {
        return;
    };

    pov.0 = camera_transform.translation;

    let needed_pos = camera_transform.translation.normalize()
        + camera_transform.translation.normalize() * (persp.fov / 5.0);

    for (body_entity, body, mut chunk_refs) in body_query.iter_mut() {
        let needed_indices = body.octree.get_chunk_indices(needed_pos);
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
        dbg!(obsolete_indices.len());

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

fn despawn_chunks(
    mut commands: Commands,
    chunk_query: Query<(Entity, &Chunk, &AwaitingDeletion)>,
    has_mesh: Query<Option<&Mesh3d>>,
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
            storage.0.remove(&chunk.index);
        }
    }
}
