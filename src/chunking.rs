use bevy::pbr::{ExtendedMaterial, OpaqueRendererMethod};
use bevy::prelude::*;
use crossbeam::channel::{unbounded, Receiver, Sender};
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::{self, Duration, Instant};

use crate::camera::CameraTarget;
use crate::flatnormal::FlatNormalMaterial;
use crate::geometry_data::GeometryData;
use crate::octree::{Octree, Point};

type ChunkIndex = Vec<u8>;

const NUM_WORKERS: usize = 16;

#[derive(Debug)]
struct ChunkRequest {
    index: ChunkIndex,
}

struct ChunkResponse {
    index: ChunkIndex,
    geometry: Mesh,
}

#[derive(Debug)]
pub enum ChunkState {
    Active,
    Inactive,
}

#[derive(Component)]
pub struct ChunkManager {
    pub geometry: Arc<GeometryData>,
    pub octree: Arc<Octree>,
    pub mesh_handles: BTreeMap<ChunkIndex, Handle<Mesh>>,
    pub mesh_entities: BTreeMap<ChunkIndex, Entity>,
    pub pov: Vec3,

    /// Indices for which we have requested geometry and not yet received a response.
    pub active_requests: BTreeSet<ChunkIndex>,

    /// Communication channels
    pub sender: (Sender<ChunkRequest>, Receiver<ChunkRequest>),
    pub receiver: (Sender<ChunkResponse>, Receiver<ChunkResponse>),

    /// The worker threads themselves
    pub workers: Vec<JoinHandle<()>>,

    /// Desired chunk states
    pub active_chunks: BTreeSet<ChunkIndex>,
}

impl ChunkManager {
    fn spawn_workers(
        sender: &Receiver<ChunkRequest>,
        responder: &Sender<ChunkResponse>,
        octree: Arc<Octree>,
        geometry: Arc<GeometryData>,
        n: usize,
    ) -> Vec<JoinHandle<()>> {
        let mut handles = Vec::new();

        for _ in 0..n {
            let request_receiver = sender.clone();
            let response_sender = responder.clone();

            let geometry = geometry.clone();
            let octree = octree.clone();
            let handle = thread::spawn(move || {
                while let Ok(msg) = request_receiver.recv() {
                    let ChunkRequest { index } = msg;

                    // Build chunk geometry
                    // 1) get which cells belong to that chunk
                    if let Some(cells) = octree.get_cells_for_index(&index) {
                        // 2) build geometry data
                        let local_geometry = geometry.sub_geometry(&cells);

                        // 3) send back
                        let _ = response_sender.send(ChunkResponse {
                            index,
                            geometry: local_geometry.mesh(),
                        });
                    }
                }
            });

            handles.push(handle);
        }

        handles
    }

    pub fn new(geometry: GeometryData) -> Self {
        let capacity = 128;
        let bounds = 1.0;
        let center = Vec3::ZERO;
        let mut octree = Octree::new(capacity, center, bounds, 0, vec![]);

        for (cell_index, &position) in geometry.cell_normals.iter().enumerate() {
            octree.insert(Point {
                position,
                value: cell_index,
            });
        }

        let geometry = Arc::new(geometry);
        let octree = Arc::new(octree);

        let (request_sender, request_recv) = unbounded::<ChunkRequest>();
        let (response_sender, response_recv) = unbounded::<ChunkResponse>();

        let workers = Self::spawn_workers(
            &request_recv,
            &response_sender,
            octree.clone(),
            geometry.clone(),
            NUM_WORKERS,
        );

        Self {
            geometry,
            octree,
            mesh_entities: BTreeMap::new(),
            mesh_handles: BTreeMap::new(),
            active_requests: BTreeSet::new(),
            sender: (request_sender, request_recv),
            receiver: (response_sender, response_recv),
            workers,
            pov: Vec3::ZERO,
            active_chunks: BTreeSet::new(),
        }
    }

    /// If any worker threads have exited or panicked, re-spawn them
    pub fn check_and_respawn_workers(&mut self) {
        let span = info_span!("check_andd_respawn", name = "check_and_respawn").entered();
        let mut still_alive = Vec::new();
        for handle in self.workers.drain(..) {
            if handle.is_finished() {
                let _ = handle.join();
                still_alive.extend(Self::spawn_workers(
                    &self.sender.1,
                    &self.receiver.0,
                    self.octree.clone(),
                    self.geometry.clone(),
                    1,
                ));
            } else {
                still_alive.push(handle);
            }
        }
        self.workers = still_alive;
    }

    pub fn update_pov(&mut self, new_pov: Vec3) {
        let span = info_span!("update_pov", name = "update_pov").entered();
        if self.pov == new_pov {
            return;
        }
        self.pov = new_pov;

        // 1) Octree to find chunk indices near new POV
        let needed_indices = self.octree.get_chunk_indices(new_pov);

        // Create requests for newly needed
        for idx in &needed_indices {
            // if we do not have a mesh handle for it and not requesting
            if !self.mesh_handles.contains_key(idx) && !self.active_requests.contains(idx) {
                // Send request to worker threads
                let _ = self.sender.0.send(ChunkRequest { index: idx.clone() });
                self.active_requests.insert(idx.clone());
            }
        }

        self.active_chunks = needed_indices.iter().cloned().collect();
    }
}

pub fn process_chunk_responses_system(
    mut query: Query<&mut ChunkManager>,
    mut meshes: ResMut<Assets<Mesh>>,
) {
    let span = info_span!("process_chunk_response_system").entered();
    let time = Instant::now();
    if let Ok(mut manager) = query.get_single_mut() {
        while let Ok(resp) = manager.receiver.1.try_recv() {
            let span = info_span!("response").entered();
            let ChunkResponse {
                index,
                geometry: mesh,
            } = resp;

            // Mark that we are no longer waiting
            manager.active_requests.remove(&index);
            if manager.mesh_handles.contains_key(&index) {
                continue;
            }

            // Convert to mesh + cache
            let new_handle = meshes.add(mesh);
            manager
                .mesh_handles
                .insert(index.clone(), new_handle.clone());

            if time.elapsed() > Duration::from_millis(1) {
                break;
            }
        }
    }
}

fn create_material(
    flat_materials: &mut ResMut<Assets<ExtendedMaterial<StandardMaterial, FlatNormalMaterial>>>,
) -> MeshMaterial3d<ExtendedMaterial<StandardMaterial, FlatNormalMaterial>> {
    let span = info_span!("create_material", name = "create_material").entered();
    let extended_material = ExtendedMaterial {
        base: StandardMaterial {
            opaque_render_method: OpaqueRendererMethod::Auto,
            ..Default::default()
        },
        extension: FlatNormalMaterial {},
    };
    MeshMaterial3d(flat_materials.add(extended_material))
}

pub(crate) fn update_chunk_pov_system(
    mut query: Query<&mut ChunkManager>,
    camera_query: Query<(&Transform, &Projection), With<Camera>>,
) {
    if let Ok((camera_transform, projection)) = camera_query.get_single() {
        if let Ok(mut manager) = query.get_single_mut() {
            if let Projection::Perspective(projection) = projection {
                manager.update_pov(
                    camera_transform.translation.normalize()
                        + camera_transform.translation.normalize() * projection.fov / 5.0,
                );
            }
        }
    }
}

pub fn process_chunk_backlog_system(
    mut commands: Commands,
    mut query: Query<&mut ChunkManager>,
    mut flat_materials: ResMut<Assets<ExtendedMaterial<StandardMaterial, FlatNormalMaterial>>>,
) {
    let Ok(mut manager) = query.get_single_mut() else {
        return;
    };

    let material = create_material(&mut flat_materials);

    let active_entities: BTreeSet<_> = manager.mesh_entities.keys().cloned().collect();

    let ChunkManager {
        mesh_entities,
        active_chunks,
        active_requests,
        mesh_handles,
        ..
    } = &mut manager.into_inner();

    let time = Instant::now();

    for idx in active_entities.difference(&active_chunks) {
        if active_requests.len() > 0 {
            break;
        }

        let Some(entity) = mesh_entities.remove(idx) else {
            continue;
        };
        commands.entity(entity).despawn_recursive();
        if time.elapsed() > Duration::from_millis(1) {
            break;
        }
    }

    for idx in active_chunks.difference(&active_entities) {
        match mesh_handles.get(idx) {
            Some(handle) => {
                if mesh_entities.contains_key(idx) {
                    continue;
                }
                let entity = commands
                    .spawn((
                        Mesh3d(handle.clone()),
                        material.clone(),
                        Transform::from_scale(Vec3::splat(32.0)),
                        Name::new(format!("Chunk {:?}", idx)),
                    ))
                    .id();
                mesh_entities.insert(idx.clone(), entity);
            }
            _ => {}
        }
        if time.elapsed() > Duration::from_millis(1) {
            break;
        }
    }
}

pub fn check_workers_system(mut query: Query<&mut ChunkManager>) {
    if let Ok(mut manager) = query.get_single_mut() {
        manager.check_and_respawn_workers();
    }
}

pub fn setup_demo_chunk_manager(mut commands: Commands) {
    let geom = crate::geometry_data::GeometryData::icosahedron()
        .subdivide_n(9)
        .slerp()
        .recell()
        .dual()
        .duplicate();

    let manager = ChunkManager::new(geom);

    commands.spawn((manager, Name::new("ChunkManager")));
    commands.spawn((Transform::IDENTITY, CameraTarget { radius: 32.0 }));
}

pub struct ChunkManagerDemoPlugin;
impl Plugin for ChunkManagerDemoPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, setup_demo_chunk_manager)
            .add_systems(
                FixedUpdate,
                (
                    update_chunk_pov_system,
                    process_chunk_responses_system,
                    process_chunk_backlog_system,
                    check_workers_system,
                ),
            );
    }
}
