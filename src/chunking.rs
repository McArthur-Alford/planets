use bevy::pbr::{ExtendedMaterial, OpaqueRendererMethod};
use bevy::prelude::*;
use bevy::time::common_conditions::on_timer;
use bevy::utils::HashMap;
use crossbeam::channel::{unbounded, Receiver, Sender};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use crate::camera::CameraTarget;
use crate::flatnormal::FlatNormalMaterial;
use crate::geometry_data::GeometryData;
use crate::octree::{Octree, Point};

pub(crate) type ChunkIndex = Vec<u8>;

const NUM_WORKERS: usize = 16;

#[derive(Debug)]
struct ChunkRequest {
    index: ChunkIndex,
}

struct ChunkResponse {
    index: ChunkIndex,
    cells: Vec<usize>,
    mesh: Mesh,
}

#[derive(Debug)]
pub enum ChunkState {
    Active,
    Inactive,
}

#[derive(Debug, Default)]
pub struct ChunkData {
    pub mesh_handle: Option<Handle<Mesh>>,
    pub entity: Option<Entity>,
    pub cells: Option<Vec<usize>>,
}

#[derive(Component)]
pub struct ChunkManager {
    pub geometry: Arc<GeometryData>,
    pub octree: Arc<Octree>,

    /// Contains all chunk-specific data.
    pub chunk_data: BTreeMap<ChunkIndex, ChunkData>,

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
    fn print_self(&self) {
        println!("CHUNK MANAGER");
        // println!("geometry: {}", (&self.geometry).len());
        println!("octree: {}", &self.octree.height);
        println!("chunk_data: {}", &self.chunk_data.len());
        // println!("pov: {}", &self.pov.len());
        println!("active_requests: {}", &self.active_requests.len());
        // println!("sender: {}", &self.sender.len());
        // println!("receiver: {}", &self.receiver.len());
        println!("workers: {}", &self.workers.len());
        println!("active_chunks: {}", &self.active_chunks.len());
    }

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
                        let (local_geometry, cell_map) = geometry.sub_geometry(&cells);

                        // 3) send back
                        let _ = response_sender.send(ChunkResponse {
                            index,
                            mesh: local_geometry.mesh(),
                            cells,
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
            chunk_data: BTreeMap::new(),
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
        if self.pov == new_pov {
            return;
        }
        self.pov = new_pov;

        // 1) Octree to find chunk indices near new POV
        let needed_indices = self.octree.get_chunk_indices(1, new_pov, 1.0);

        // Create requests for newly needed
        for idx in &needed_indices {
            dbg!(&idx);
            let have_mesh = self
                .chunk_data
                .get(idx)
                .and_then(|data| data.mesh_handle.as_ref())
                .is_some();

            if !have_mesh && !self.active_requests.contains(idx) {
                // Send request to worker threads
                let _ = self.sender.0.send(ChunkRequest { index: idx.clone() });
                self.active_requests.insert(idx.clone());
            }
        }

        let new_active_chunks = needed_indices.iter().cloned().collect();
        self.active_chunks = new_active_chunks;
    }
}

pub fn cleanup_old_handles(mut query: Query<&mut ChunkManager>) {
    let Ok(mut manager) = query.get_single_mut() else {
        return;
    };
    println!("\n\n");
    manager.print_self();

    manager
        .chunk_data
        .retain(|index, chunk_data| !(chunk_data.entity.is_none()));

    // manager.chunk_data.shrink_to_fit();
    // manager.active_chunks.shrink_to_fit();
    // manager.active_requests.shrink_to_fit();

    manager.print_self();
}

pub fn process_chunk_responses_system(
    mut query: Query<&mut ChunkManager>,
    mut meshes: ResMut<Assets<Mesh>>,
) {
    let time = Instant::now();
    if let Ok(mut manager) = query.get_single_mut() {
        while let Ok(resp) = manager.receiver.1.try_recv() {
            let ChunkResponse { index, cells, mesh } = resp;

            // Mark that we are no longer waiting
            manager.active_requests.remove(&index);

            let chunk_data = manager.chunk_data.entry(index.clone()).or_default();

            // If we already have a mesh handle, skip
            if chunk_data.mesh_handle.is_some() {
                continue;
            }

            // Store the cells
            chunk_data.cells = Some(cells);

            // Convert to mesh + cache
            let new_handle = meshes.add(mesh);
            chunk_data.mesh_handle = Some(new_handle);

            if time.elapsed() > Duration::from_millis(1) {
                break;
            }
        }
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

#[derive(Resource)]
pub struct HexsphereMaterial(pub Handle<ExtendedMaterial<StandardMaterial, FlatNormalMaterial>>);

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
    material: Res<HexsphereMaterial>,
) {
    let Ok(mut manager) = query.get_single_mut() else {
        return;
    };

    let ChunkManager {
        geometry,
        octree,
        chunk_data,
        pov,
        active_requests,
        sender,
        receiver,
        workers,
        active_chunks,
    } = &mut manager.into_inner();

    // Collect which indices currently have spawned entities
    let active_entities: BTreeSet<_> = chunk_data
        .iter()
        .filter_map(|(idx, data)| data.entity.map(|_| idx.clone()))
        .collect();

    // Despawn any chunks that are no longer active
    for idx in active_entities.difference(active_chunks).take(10) {
        if !active_requests.is_empty() {
            break;
        }

        if let Some(data) = chunk_data.get_mut(idx) {
            if let Some(entity) = data.entity.take() {
                commands.entity(entity).despawn();
            }
        }
    }

    // Spawn any new chunks that need to be active
    let mut entities = Vec::new();
    let mut bundles = Vec::new();
    for idx in active_chunks.difference(&active_entities).take(10) {
        if let Some(data) = chunk_data.get_mut(idx) {
            if data.mesh_handle.is_some() && data.entity.is_none() {
                data.entity = Some(commands.spawn_empty().id());
                entities.push(data.entity.unwrap());
                bundles.push((
                    Mesh3d(data.mesh_handle.clone().unwrap()),
                    MeshMaterial3d(material.0.clone()),
                    Transform::from_scale(Vec3::splat(32.0)),
                    Name::new(format!("Chunk {:?}", idx)),
                ));
            }
        }
    }

    commands.insert_or_spawn_batch(entities.into_iter().zip(bundles.into_iter()));
}

pub fn check_workers_system(mut query: Query<&mut ChunkManager>) {
    if let Ok(mut manager) = query.get_single_mut() {
        manager.check_and_respawn_workers();
    }
}

pub fn setup_demo_chunk_manager(
    mut commands: Commands,
    mut flat_materials: ResMut<Assets<ExtendedMaterial<StandardMaterial, FlatNormalMaterial>>>,
) {
    let geom = crate::geometry_data::GeometryData::icosahedron()
        .subdivide_n(8)
        .slerp()
        .recell()
        .dual()
        .duplicate();

    let manager = ChunkManager::new(geom);

    commands.spawn((manager, Name::new("ChunkManager")));
    commands.spawn((Transform::IDENTITY, CameraTarget { radius: 32.0 }));

    let material = create_material(&mut flat_materials);
    commands.insert_resource(HexsphereMaterial(material));
}

pub struct ChunkManagerDemoPlugin;
impl Plugin for ChunkManagerDemoPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, setup_demo_chunk_manager)
            .add_systems(
                FixedUpdate,
                (
                    update_chunk_pov_system,
                    process_chunk_responses_system.after(update_chunk_pov_system),
                    process_chunk_backlog_system.after(process_chunk_responses_system),
                    check_workers_system,
                    // cleanup_old_handles.run_if(on_timer(Duration::from_secs(10))),
                ),
            );
    }
}
