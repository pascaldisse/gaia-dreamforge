use std::collections::BTreeMap;

use bytemuck::{Pod, Zeroable};
use crystal::{
    EcsWorld, Environment, Mesh, MeshPart, NumberOrNumbers, Op, QuerySpec, Spawn, Transform,
};
use glam::{EulerRot, Mat3, Mat4, Quat, Vec3};
use kami::{BindPose, Registry, TickContext};
use serde_json::Number;
use transmutation::{
    Bounds, Cluster, Dag, Mesh as ChainMesh, TransmuteParams, Vertex as ChainVertex,
    transmute_default,
};

pub use first_light::FirstLight;

pub const WORLD_SHADER: &str = include_str!("world.wgsl");

#[derive(Clone, Debug)]
pub struct SceneParameters {
    pub fov_y_degrees: f32,
    pub near: f32,
    pub far: f32,
    pub sky_top: String,
    pub sky_horizon: String,
    pub mesh_color: String,
    pub radial_segments: u32,
    pub camera_position: [f32; 3],
    pub camera_yaw: f32,
    pub camera_pitch: f32,
    /// Great Chain cut threshold τ (screen-space error, ~pixels). A cluster is
    /// drawn where `parent_error > τ ≥ error` projected through its group's
    /// shared LOD sphere. Smaller = finer detail held longer. A PARAM (never
    /// hardcode): env `GAIA_NATIVE_CLUSTER_ERROR`.
    pub cluster_error_threshold: f32,
    /// World-clock tick delta (seconds) for the living layer's entropy tick.
    /// A PARAM (never hardcode), default 1/60: env `GAIA_NATIVE_TICK_DT`.
    pub tick_dt: f64,
    /// First Light defaults, overridden per-scene by the `environment` component.
    pub first_light: first_light::FirstLightDefaults,
}

/// A camera pose. `yaw` turns around +Y, `pitch` is negative looking down.
#[derive(Clone, Copy, Debug)]
pub struct Camera {
    pub eye: Vec3,
    pub yaw: f32,
    pub pitch: f32,
    pub fov_y_radians: f32,
    pub near: f32,
    pub far: f32,
}

impl Camera {
    /// Unit forward vector from yaw+pitch. yaw 0 looks down -Z; pitch<0 looks down.
    pub fn direction(&self) -> Vec3 {
        let cos_pitch = self.pitch.cos();
        Vec3::new(
            -self.yaw.sin() * cos_pitch,
            self.pitch.sin(),
            -self.yaw.cos() * cos_pitch,
        )
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct FrameUniform {
    pub view_projection: [f32; 16],
    pub sky_top: [f32; 4],
    pub sky_horizon: [f32; 4],
    /// First Light: direction TOWARD the sun (xyz), w unused.
    pub sun_direction: [f32; 4],
    /// First Light: sun colour (rgb) and intensity (w).
    pub sun_color: [f32; 4],
    /// First Light: ambient colour (rgb) and intensity (w).
    pub ambient: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct Vertex {
    pub position: [f32; 3],
    pub normal: [f32; 3],
    pub color: [f32; 3],
    pub emissive: f32,
}

impl Vertex {
    pub fn layout() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Self>() as u64,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &[
                wgpu::VertexAttribute {
                    format: wgpu::VertexFormat::Float32x3,
                    offset: 0,
                    shader_location: 0,
                },
                wgpu::VertexAttribute {
                    format: wgpu::VertexFormat::Float32x3,
                    offset: 12,
                    shader_location: 1,
                },
                wgpu::VertexAttribute {
                    format: wgpu::VertexFormat::Float32x3,
                    offset: 24,
                    shader_location: 2,
                },
                wgpu::VertexAttribute {
                    format: wgpu::VertexFormat::Float32,
                    offset: 36,
                    shader_location: 3,
                },
            ],
        }
    }
}

/// One transmuted material batch: a Great Chain (the SOLE geometry path) plus
/// the flat colour/emissive its clusters draw with. Geometry stays generic —
/// the chain knows nothing of colour; colour rides the batch, not the vertex
/// stream, so identical geometry across colours never fragments the chain.
pub struct MaterialChain {
    pub dag: Dag,
    pub color: [f32; 3],
    pub emissive: f32,
}

pub struct RenderScene {
    /// Camera derived from the world `spawn`; the moving eye overrides it per request.
    pub camera: Camera,
    pub sky_top: [f32; 4],
    pub sky_horizon: [f32; 4],
    pub first_light: FirstLight,
    /// Per-material transmuted Great Chains. THE geometry path: every draw is a
    /// view-dependent cluster cut over these (the W1/W2 forward per-primitive
    /// path is gone).
    pub chains: Vec<MaterialChain>,
    /// Great Chain cut threshold τ (screen-space error), carried from params.
    pub error_threshold: f32,
    /// The living layer: every entity carrying a `behavior` component. Excluded
    /// from the static `chains`; each gets its OWN chains (baked at the bind
    /// pose) plus a per-entity model transform re-derived every world tick. The
    /// [`Dynamics`] owns the live ECS the tick reads and writes.
    pub dynamics: Dynamics,
}

/// Material batch key: quantised linear colour bits + emissive flag. Ordered so
/// the chain vector is deterministic (byte-identical double builds).
type MatKey = ([u32; 3], u32);

struct MatBucket {
    /// World-space triangle soup (position/normal/uv); transmuted at seal.
    vertices: Vec<ChainVertex>,
    color: [f32; 3],
    emissive: f32,
}

impl RenderScene {
    pub fn from_ecs(mut world: EcsWorld, parameters: &SceneParameters) -> Result<Self, String> {
        let world = &mut world;
        if !(parameters.fov_y_degrees > 0.0 && parameters.fov_y_degrees < 180.0) {
            return Err("GAIA_NATIVE_FOV must be between 0 and 180 degrees".into());
        }
        if parameters.near <= 0.0 || parameters.far <= parameters.near {
            return Err("GAIA_NATIVE_NEAR must be positive and less than GAIA_NATIVE_FAR".into());
        }
        if parameters.radial_segments < 3 {
            return Err("GAIA_NATIVE_RADIAL_SEGMENTS must be at least 3".into());
        }

        let spawn = first_component::<Spawn>(world, "spawn")?;
        let eye = spawn
            .as_ref()
            .and_then(|spawn| vec3(spawn.position.as_ref()))
            .unwrap_or(Vec3::from_array(parameters.camera_position));
        let yaw = spawn
            .as_ref()
            .and_then(|spawn| number(spawn.yaw.as_ref()))
            .unwrap_or(parameters.camera_yaw);
        let camera = Camera {
            eye,
            yaw,
            pitch: parameters.camera_pitch,
            fov_y_radians: parameters.fov_y_degrees.to_radians(),
            near: parameters.near,
            far: parameters.far,
        };

        let environment = first_component::<Environment>(world, "environment")?;
        let sky_top = environment
            .as_ref()
            .and_then(|environment| environment.sky.as_ref())
            .and_then(|sky| sky.top.as_deref())
            .unwrap_or(&parameters.sky_top);
        let sky_horizon = environment
            .as_ref()
            .and_then(|environment| environment.sky.as_ref())
            .and_then(|sky| sky.horizon.as_deref())
            .unwrap_or(&parameters.sky_horizon);
        let sky_top = linear_rgba(sky_top)?;
        let sky_horizon = linear_rgba(sky_horizon)?;
        let first_light = FirstLight::derive(environment.as_ref(), &parameters.first_light)?;
        let default_color = linear_rgb(&parameters.mesh_color)?;

        // Entities carrying a `behavior` component are DYNAMIC: split off from
        // the shared static chains into their own (bind-baked) chains + a live
        // model transform. Generic — the split reads only the `behavior` marker.
        let behavior_id = world.component_id("behavior");
        let render_components = world
            .component_id("transform")
            .zip(world.component_id("mesh"));
        let mut entities = render_components
            .map(|(transform, mesh)| {
                world.query(&QuerySpec {
                    all: vec![transform, mesh],
                    ..Default::default()
                })
            })
            .unwrap_or_default();
        entities.sort_by(|a, b| world.gaia_id_for(*a).cmp(&world.gaia_id_for(*b)));

        // Tessellate every mesh part into world-space triangles. Static parts
        // pool into shared material buckets; each dynamic entity seals its OWN.
        let mut static_buckets = BTreeMap::<MatKey, MatBucket>::new();
        let mut dynamics = Dynamics::new(parameters.cluster_error_threshold);
        for entity in entities {
            let (transform_id, mesh_id) = render_components.expect("render query has components");
            let id = world.gaia_id_for(entity).unwrap_or("<unbound>").to_string();
            let transform: Transform =
                serde_json::from_value(world.get_component(entity, transform_id)?)
                    .map_err(|error| format!("entity {id:?} transform: {error}"))?;
            let mesh: Mesh = serde_json::from_value(world.get_component(entity, mesh_id)?)
                .map_err(|error| format!("entity {id:?} mesh: {error}"))?;
            let parts = parts_of(mesh).map_err(|error| format!("entity {id:?} mesh: {error}"))?;
            let bind_position = vec3(transform.position.as_ref()).unwrap_or(Vec3::ZERO);
            let bind_rotation = vec3(transform.rotation.as_ref()).unwrap_or(Vec3::ZERO);
            let bind_scale = scale(transform.scale.as_ref());
            let entity_model = transform_matrix(bind_position, bind_rotation, bind_scale);

            let is_dynamic = behavior_id
                .map(|behavior| world.get_component(entity, behavior).is_ok())
                .unwrap_or(false);

            if is_dynamic {
                let mut buckets = BTreeMap::<MatKey, MatBucket>::new();
                for (index, part) in parts.iter().enumerate() {
                    append_part(
                        &mut buckets,
                        part,
                        entity_model,
                        default_color,
                        parameters.radial_segments,
                    )
                    .map_err(|error| format!("entity {id:?} mesh part {index}: {error}"))?;
                }
                let chains =
                    seal_buckets(buckets).map_err(|error| format!("entity {id:?}: {error}"))?;
                let bind = BindPose {
                    position: bind_position.as_dvec3().to_array(),
                    rotation: bind_rotation.as_dvec3().to_array(),
                    scale: bind_scale.as_dvec3().to_array(),
                    intensity: 1.0,
                };
                dynamics.push(&id, chains, bind, entity_model);
            } else {
                for (index, part) in parts.iter().enumerate() {
                    append_part(
                        &mut static_buckets,
                        part,
                        entity_model,
                        default_color,
                        parameters.radial_segments,
                    )
                    .map_err(|error| format!("entity {id:?} mesh part {index}: {error}"))?;
                }
            }
        }

        // Seal the shared static buckets; the dynamics take ownership of the ECS
        // (its live tick reads and writes the animated transforms).
        let chains = seal_buckets(static_buckets)?;
        dynamics.install_world(std::mem::take(world), parameters);

        Ok(Self {
            camera,
            sky_top,
            sky_horizon,
            first_light,
            chains,
            error_threshold: parameters.cluster_error_threshold,
            dynamics,
        })
    }

    /// Advance the world clock one fixed tick: KAMI reads the ECS, emits ops,
    /// they apply to the ECS, and each dynamic entity's model transform is
    /// re-derived from its (now animated) transform. Deterministic in the tick
    /// count — never wall time. Call once per rendered frame.
    pub fn tick(&mut self) {
        self.dynamics.tick();
    }

    /// Select and expand the view-dependent cluster cut into draw vertices — the
    /// ONE geometry path. For each chain, every cluster is drawn where its
    /// group's projected `parent_error > τ ≥ error` (crack-free by the shared
    /// LOD metric); leaves carry error 0, roots carry parent_error ∞, so exactly
    /// one cut covers the surface. Colour/emissive come from the batch.
    pub fn select_vertices(&self, camera: &Camera, viewport_height: u32) -> Vec<Vertex> {
        let half_fov = (camera.fov_y_radians * 0.5).tan().max(1e-6);
        let projection_scale = viewport_height.max(1) as f32 / (2.0 * half_fov);
        let mut out = Vec::<Vertex>::new();
        for chain in &self.chains {
            select_chain(
                chain,
                camera,
                projection_scale,
                self.error_threshold,
                &mut out,
            );
        }
        out
    }

    /// Every leaf triangle's corner positions, world-space, view-independent —
    /// the EXACT geometry (error 0, the world itself). The Embodiment's floor:
    /// a body stands on the world, never on a camera's coarse cut.
    pub fn leaf_positions(&self) -> Vec<[f32; 3]> {
        let mut out = Vec::new();
        for chain in &self.chains {
            if let Some(leaf_ids) = chain.dag.levels.first() {
                for &id in leaf_ids {
                    let cluster = chain.dag.cluster(id);
                    for &index in &cluster.indices {
                        out.push(cluster.vertices[index as usize].position);
                    }
                }
            }
        }
        out
    }

    /// Project the world-space scene through an arbitrary camera pose (the moving eye).
    pub fn frame_uniform(&self, width: u32, height: u32, camera: &Camera) -> FrameUniform {
        let aspect = width as f32 / height.max(1) as f32;
        // Camera-relative view: translate world into the eye frame in one look_to.
        let view = Mat4::look_to_rh(camera.eye, camera.direction(), Vec3::Y);
        let projection =
            Mat4::perspective_rh(camera.fov_y_radians, aspect, camera.near, camera.far);
        FrameUniform {
            view_projection: (projection * view).to_cols_array(),
            sky_top: self.sky_top,
            sky_horizon: self.sky_horizon,
            sun_direction: self.first_light.sun_direction(),
            sun_color: self.first_light.sun_color(),
            ambient: self.first_light.ambient(),
        }
    }
}

/// First Light — the ONE deletable sun+ambient scaffold module.
/// Dies at Rite IV (Lumen Naturae) when the path integrator takes over shading.
pub mod first_light {
    use super::{linear_rgb, vec3};
    use crystal::Environment;
    use glam::Vec3;
    use serde_json::Value;

    /// Env-parameterised defaults (never hardcoded at the shading site).
    #[derive(Clone, Debug)]
    pub struct FirstLightDefaults {
        pub sun_color: String,
        pub sun_intensity: f32,
        pub sun_position: [f32; 3],
        pub ambient_color: String,
        pub ambient_intensity: f32,
    }

    /// Resolved directional sun + ambient, ready for the frame uniform.
    #[derive(Clone, Copy, Debug)]
    pub struct FirstLight {
        sun_direction: Vec3,
        sun_color: [f32; 3],
        sun_intensity: f32,
        ambient_color: [f32; 3],
        ambient_intensity: f32,
    }

    impl FirstLight {
        /// Read `environment.sun` / `environment.hemisphere` when present, else defaults.
        pub fn derive(
            environment: Option<&Environment>,
            defaults: &FirstLightDefaults,
        ) -> Result<Self, String> {
            let sun = environment.and_then(|environment| environment.sun.as_ref());
            let hemisphere = environment.and_then(|environment| environment.hemisphere.as_ref());

            let sun_color = string_field(sun, "color").unwrap_or(&defaults.sun_color);
            let sun_color = linear_rgb(sun_color)?;
            let sun_intensity =
                value_number(sun, "intensity").unwrap_or(defaults.sun_intensity as f64) as f32;
            let sun_position =
                value_vec3(sun, "position").unwrap_or(Vec3::from_array(defaults.sun_position));
            let sun_direction = sun_position.normalize_or_zero();

            let ambient_color = string_field(hemisphere, "sky").unwrap_or(&defaults.ambient_color);
            let ambient_color = linear_rgb(ambient_color)?;
            let ambient_intensity = value_number(hemisphere, "intensity")
                .unwrap_or(defaults.ambient_intensity as f64)
                as f32;

            Ok(Self {
                sun_direction,
                sun_color,
                sun_intensity,
                ambient_color,
                ambient_intensity,
            })
        }

        pub fn sun_direction(&self) -> [f32; 4] {
            [
                self.sun_direction.x,
                self.sun_direction.y,
                self.sun_direction.z,
                0.0,
            ]
        }
        pub fn sun_color(&self) -> [f32; 4] {
            [
                self.sun_color[0],
                self.sun_color[1],
                self.sun_color[2],
                self.sun_intensity,
            ]
        }
        pub fn ambient(&self) -> [f32; 4] {
            [
                self.ambient_color[0],
                self.ambient_color[1],
                self.ambient_color[2],
                self.ambient_intensity,
            ]
        }
    }

    fn string_field<'a>(value: Option<&'a Value>, key: &str) -> Option<&'a str> {
        value?.get(key)?.as_str()
    }
    fn value_number(value: Option<&Value>, key: &str) -> Option<f64> {
        value?.get(key)?.as_f64()
    }
    fn value_vec3(value: Option<&Value>, key: &str) -> Option<Vec3> {
        let array = value?.get(key)?.as_array()?;
        let numbers: Vec<serde_json::Number> = array
            .iter()
            .filter_map(|item| item.as_f64().and_then(serde_json::Number::from_f64))
            .collect();
        vec3(Some(&numbers))
    }
}

#[derive(Clone, Copy)]
struct PrimitiveVertex {
    position: Vec3,
    normal: Vec3,
}

fn first_component<T: serde::de::DeserializeOwned>(
    world: &EcsWorld,
    name: &str,
) -> Result<Option<T>, String> {
    let Some(component) = world.component_id(name) else {
        return Ok(None);
    };
    let mut entities = world.query(&QuerySpec {
        all: vec![component],
        ..Default::default()
    });
    entities.sort_by(|a, b| world.gaia_id_for(*a).cmp(&world.gaia_id_for(*b)));
    entities
        .first()
        .map(|entity| {
            serde_json::from_value(world.get_component(*entity, component)?)
                .map_err(|error| format!("component {name:?}: {error}"))
        })
        .transpose()
}

fn parts_of(mesh: Mesh) -> Result<Vec<MeshPart>, String> {
    if let Some(parts) = mesh.parts {
        return Ok(parts);
    }
    if mesh.extra.contains_key("shape") {
        return serde_json::from_value(serde_json::Value::Object(mesh.extra))
            .map(|part| vec![part])
            .map_err(|error| error.to_string());
    }
    Ok(Vec::new())
}

/// Project a cluster's LOD error through its group's SHARED bounds sphere to a
/// screen-space error (~pixels). Error 0 (leaves) stays 0. Distance metric
/// (Rite III); hardware visibility lands later.
fn project_error(error: f32, bounds: &Bounds, camera: &Camera, projection_scale: f32) -> f32 {
    if error <= 0.0 {
        return 0.0;
    }
    let center = Vec3::from_array(bounds.center);
    let distance = ((center - camera.eye).length() - bounds.radius).max(camera.near);
    error * projection_scale / distance
}

/// Expand one chain's view-dependent cut into `out`. `error` side reads the
/// PRODUCING group's sphere (`cluster.group`; None = leaf, error 0); the
/// `parent_error` side reads the CONSUMING group's sphere (`cluster.parent_group`;
/// None = terminal/root, ∞). Draw where `parent_sse > τ ≥ self_sse`.
fn select_chain(
    chain: &MaterialChain,
    camera: &Camera,
    projection_scale: f32,
    tau: f32,
    out: &mut Vec<Vertex>,
) {
    let dag = &chain.dag;
    for cluster in &dag.clusters {
        let self_sse = match cluster.group {
            Some(group) => project_error(
                cluster.error,
                &dag.group(group).bounds,
                camera,
                projection_scale,
            ),
            None => 0.0,
        };
        let parent_sse = match cluster.parent_group {
            Some(group) => project_error(
                cluster.parent_error,
                &dag.group(group).bounds,
                camera,
                projection_scale,
            ),
            None => f32::INFINITY,
        };
        if parent_sse > tau && tau >= self_sse {
            emit_cluster(cluster, chain.color, chain.emissive, out);
        }
    }
}

fn emit_cluster(cluster: &Cluster, color: [f32; 3], emissive: f32, out: &mut Vec<Vertex>) {
    out.reserve(cluster.indices.len());
    for &index in &cluster.indices {
        let vertex = &cluster.vertices[index as usize];
        out.push(Vertex {
            position: vertex.position,
            normal: vertex.normal,
            color,
            emissive,
        });
    }
}

/// Seal material buckets into transmuted Great Chains. `transmute` is
/// deterministic (BTree ordering + canonical welds), so two builds of one input
/// produce byte-identical chains. Shared by the static pool and every dynamic
/// entity's own chains.
fn seal_buckets(buckets: BTreeMap<MatKey, MatBucket>) -> Result<Vec<MaterialChain>, String> {
    let chain_params = TransmuteParams::default();
    let mut chains = Vec::<MaterialChain>::with_capacity(buckets.len());
    for bucket in buckets.into_values() {
        if bucket.vertices.is_empty() {
            continue;
        }
        let indices: Vec<u32> = (0..bucket.vertices.len() as u32).collect();
        let mesh = ChainMesh::new(bucket.vertices, indices);
        let dag = transmute_default(&mesh, &chain_params)
            .map_err(|error| format!("transmute material chain: {error}"))?;
        chains.push(MaterialChain {
            dag,
            color: bucket.color,
            emissive: bucket.emissive,
        });
    }
    Ok(chains)
}

/// Every LEAF triangle of a chain as draw vertices (finest LOD, view-
/// independent). Dynamic entities are small and move under a per-entity model
/// transform, so they draw their exact geometry rather than a view-dependent
/// cut baked at the bind pose.
pub fn chain_leaf_vertices(chain: &MaterialChain) -> Vec<Vertex> {
    let mut out = Vec::new();
    if let Some(leaf_ids) = chain.dag.levels.first() {
        for &id in leaf_ids {
            emit_cluster(chain.dag.cluster(id), chain.color, chain.emissive, &mut out);
        }
    }
    out
}

/// One dynamic entity: its own material chains (baked at the authored BIND world
/// pose) plus the live model transform that animates them on the GPU. `model` =
/// `M(animated) * M(bind)⁻¹`, applied to the bind-baked world-space vertices.
pub struct DynamicEntity {
    pub gaia_id: String,
    pub chains: Vec<MaterialChain>,
    /// The authored bind transform matrix (world-space) the chains were baked at.
    pub bind_model: Mat4,
    /// Current animated delta transform (identity until the first tick).
    pub model: Mat4,
}

impl DynamicEntity {
    /// Every leaf triangle across all this entity's chains (finest LOD).
    pub fn leaf_vertices(&self) -> Vec<Vertex> {
        let mut out = Vec::new();
        for chain in &self.chains {
            out.extend(chain_leaf_vertices(chain));
        }
        out
    }
}

/// The living layer of a scene: the ECS the world clock ticks, the dynamic
/// entities' render data, and the fixed-dt clock. The tick is closed-form on
/// the tick INDEX (entropy), never wall time — N ticks reproduce byte-identical
/// model transforms across runs.
pub struct Dynamics {
    /// The live ECS: the tick reads `behavior` here and writes animated
    /// `transform`s back (so senses/pose read the moving world).
    world: EcsWorld,
    /// Registered `transform`/`behavior` handles; `None` until `install_world`.
    reg: Option<Registry>,
    /// Rest poses keyed by gaia id — the fixed origin each kind animates around,
    /// so re-reading the transform each tick never compounds.
    binds: BTreeMap<String, BindPose>,
    entities: Vec<DynamicEntity>,
    #[allow(dead_code)]
    error_threshold: f32,
    seed: u64,
    dt: f64,
    clock: u64,
}

impl Dynamics {
    fn new(error_threshold: f32) -> Self {
        Self {
            world: EcsWorld::default(),
            reg: None,
            binds: BTreeMap::new(),
            entities: Vec::new(),
            error_threshold,
            seed: 0,
            dt: 1.0 / 60.0,
            clock: 0,
        }
    }

    fn push(&mut self, id: &str, chains: Vec<MaterialChain>, bind: BindPose, bind_model: Mat4) {
        self.binds.insert(id.to_string(), bind);
        self.entities.push(DynamicEntity {
            gaia_id: id.to_string(),
            chains,
            bind_model,
            model: Mat4::IDENTITY,
        });
    }

    /// Take ownership of the live ECS and lock in the tick's dt. Registration is
    /// idempotent — the loader already registered `transform`/`behavior`, so the
    /// existing ids are reused.
    fn install_world(&mut self, mut world: EcsWorld, parameters: &SceneParameters) {
        self.reg = Some(Registry::register(&mut world));
        self.world = world;
        self.dt = parameters.tick_dt;
        self.clock = 0;
    }

    /// One world tick (Flow of Data): KAMI reads the ECS → emits transform ops →
    /// they apply to the ECS → each entity's model is re-derived from its now-
    /// animated transform. Increments the clock. Deterministic in the count.
    pub fn tick(&mut self) {
        let Some(reg) = self.reg else {
            return;
        };
        if self.entities.is_empty() {
            return;
        }
        let ctx = TickContext {
            seed: self.seed,
            entropy: self.clock,
            dt: self.dt,
        };
        let ops = kami::tick_decorative(&self.world, reg, &self.binds, &ctx);
        for op in &ops {
            let Op::Set(set) = op else {
                continue;
            };
            if let Some(entity) = self.world.entity_for_gaia(&set.id) {
                let _ = self
                    .world
                    .set_component(entity, reg.transform, set.value.clone());
            }
        }
        for de in &mut self.entities {
            let Some(entity) = self.world.entity_for_gaia(&de.gaia_id) else {
                continue;
            };
            let Ok(value) = self.world.get_component(entity, reg.transform) else {
                continue;
            };
            let Ok(transform) = serde_json::from_value::<Transform>(value) else {
                continue;
            };
            let animated = transform_matrix(
                vec3(transform.position.as_ref()).unwrap_or(Vec3::ZERO),
                vec3(transform.rotation.as_ref()).unwrap_or(Vec3::ZERO),
                scale(transform.scale.as_ref()),
            );
            de.model = animated * de.bind_model.inverse();
        }
        self.clock += 1;
    }

    /// The model-transform storage buffer contents: one column-major mat4 per
    /// dynamic entity, in `entities()` order. Byte-identical given the tick
    /// count (the tick-determinism ordeal reads exactly these bytes).
    pub fn model_matrices(&self) -> Vec<[f32; 16]> {
        self.entities
            .iter()
            .map(|entity| entity.model.to_cols_array())
            .collect()
    }

    pub fn entities(&self) -> &[DynamicEntity] {
        &self.entities
    }
}

fn append_part(
    buckets: &mut BTreeMap<MatKey, MatBucket>,
    part: &MeshPart,
    entity_model: Mat4,
    default_color: [f32; 3],
    default_segments: u32,
) -> Result<(), String> {
    let position = vec3(part.position.as_ref()).unwrap_or(Vec3::ZERO);
    let rotation = vec3(part.rotation.as_ref()).unwrap_or(Vec3::ZERO);
    let model = entity_model * transform_matrix(position, rotation, scale(part.scale.as_ref()));
    let determinant = Mat3::from_mat4(model).determinant();
    if !determinant.is_finite() || determinant.abs() < f32::EPSILON {
        return Err("transform scale must be finite and non-zero".into());
    }
    let normal_matrix = Mat3::from_mat4(model).inverse().transpose();
    let segments = part.radial_segments.unwrap_or(default_segments).max(3);
    let primitive = match part.shape.as_deref().unwrap_or("box") {
        "box" => box_triangles(dimensions(part.size.as_ref(), [1.0, 1.0, 1.0])?),
        "sphere" => sphere_triangles(
            positive(number(part.radius.as_ref()).unwrap_or(0.5), "radius")?,
            segments,
        ),
        "cylinder" => {
            let radius = positive(number(part.radius.as_ref()).unwrap_or(0.5), "radius")?;
            let top = non_negative(
                number(part.radius_top.as_ref()).unwrap_or(radius),
                "radiusTop",
            )?;
            let bottom = non_negative(
                number(part.radius_bottom.as_ref()).unwrap_or(radius),
                "radiusBottom",
            )?;
            let height = positive(number(part.height.as_ref()).unwrap_or(1.0), "height")?;
            frustum_triangles(top, bottom, height, segments, true)
        }
        "cone" => {
            let radius = positive(number(part.radius.as_ref()).unwrap_or(0.5), "radius")?;
            let height = positive(number(part.height.as_ref()).unwrap_or(1.0), "height")?;
            frustum_triangles(0.0, radius, height, segments, true)
        }
        shape => return Err(format!("unsupported W1 primitive {shape:?}")),
    };

    let emissive = part.emissive.is_some();
    let color = match part.emissive.as_deref().or(part.color.as_deref()) {
        Some(color) => linear_rgb(color)?,
        None => default_color,
    };
    let emissive = f32::from(emissive);
    let key: MatKey = (
        [color[0].to_bits(), color[1].to_bits(), color[2].to_bits()],
        emissive.to_bits(),
    );
    let bucket = buckets.entry(key).or_insert_with(|| MatBucket {
        vertices: Vec::new(),
        color,
        emissive,
    });
    for triangle in primitive {
        for vertex in triangle {
            let world_position = model.transform_point3(vertex.position);
            let normal = (normal_matrix * vertex.normal).normalize_or_zero();
            bucket.vertices.push(ChainVertex::new(
                world_position.to_array(),
                normal.to_array(),
                [0.0, 0.0],
            ));
        }
    }
    Ok(())
}

fn box_triangles(size: Vec3) -> Vec<[PrimitiveVertex; 3]> {
    let half = size * 0.5;
    let faces = [
        (
            Vec3::X * half.x,
            Vec3::NEG_Z * half.z,
            Vec3::Y * half.y,
            Vec3::X,
        ),
        (
            Vec3::NEG_X * half.x,
            Vec3::Z * half.z,
            Vec3::Y * half.y,
            Vec3::NEG_X,
        ),
        (
            Vec3::Y * half.y,
            Vec3::X * half.x,
            Vec3::NEG_Z * half.z,
            Vec3::Y,
        ),
        (
            Vec3::NEG_Y * half.y,
            Vec3::X * half.x,
            Vec3::Z * half.z,
            Vec3::NEG_Y,
        ),
        (
            Vec3::Z * half.z,
            Vec3::X * half.x,
            Vec3::Y * half.y,
            Vec3::Z,
        ),
        (
            Vec3::NEG_Z * half.z,
            Vec3::NEG_X * half.x,
            Vec3::Y * half.y,
            Vec3::NEG_Z,
        ),
    ];
    let mut triangles = Vec::with_capacity(12);
    for (center, u, v, normal) in faces {
        let a = PrimitiveVertex {
            position: center - u - v,
            normal,
        };
        let b = PrimitiveVertex {
            position: center + u - v,
            normal,
        };
        let c = PrimitiveVertex {
            position: center + u + v,
            normal,
        };
        let d = PrimitiveVertex {
            position: center - u + v,
            normal,
        };
        triangles.extend([[a, b, c], [a, c, d]]);
    }
    triangles
}

fn sphere_triangles(radius: f32, segments: u32) -> Vec<[PrimitiveVertex; 3]> {
    let stacks = (segments / 2).max(2);
    let mut triangles = Vec::with_capacity((segments * stacks * 2) as usize);
    let point = |latitude: u32, longitude: u32| {
        let theta = std::f32::consts::PI * latitude as f32 / stacks as f32;
        let phi = std::f32::consts::TAU * longitude as f32 / segments as f32;
        let normal = Vec3::new(
            theta.sin() * phi.sin(),
            theta.cos(),
            theta.sin() * phi.cos(),
        );
        PrimitiveVertex {
            position: normal * radius,
            normal,
        }
    };
    for latitude in 0..stacks {
        for longitude in 0..segments {
            let next = longitude + 1;
            let a = point(latitude, longitude);
            let b = point(latitude + 1, longitude);
            let c = point(latitude + 1, next);
            let d = point(latitude, next);
            if latitude > 0 {
                triangles.push([a, b, c]);
            }
            if latitude + 1 < stacks {
                triangles.push([a, c, d]);
            }
        }
    }
    triangles
}

fn frustum_triangles(
    top_radius: f32,
    bottom_radius: f32,
    height: f32,
    segments: u32,
    capped: bool,
) -> Vec<[PrimitiveVertex; 3]> {
    let mut triangles = Vec::with_capacity((segments * 4) as usize);
    let half = height * 0.5;
    let slope = (bottom_radius - top_radius) / height;
    let ring = |angle: f32, radius: f32, y: f32| {
        let radial = Vec3::new(angle.sin(), 0.0, angle.cos());
        PrimitiveVertex {
            position: radial * radius + Vec3::Y * y,
            normal: Vec3::new(radial.x, slope, radial.z).normalize(),
        }
    };
    for segment in 0..segments {
        let a = std::f32::consts::TAU * segment as f32 / segments as f32;
        let b = std::f32::consts::TAU * (segment + 1) as f32 / segments as f32;
        let bottom_a = ring(a, bottom_radius, -half);
        let bottom_b = ring(b, bottom_radius, -half);
        let top_a = ring(a, top_radius, half);
        let top_b = ring(b, top_radius, half);
        triangles.push([bottom_a, bottom_b, top_b]);
        if top_radius > 0.0 {
            triangles.push([bottom_a, top_b, top_a]);
        }
        if capped && top_radius > 0.0 {
            let center = PrimitiveVertex {
                position: Vec3::Y * half,
                normal: Vec3::Y,
            };
            let mut edge_a = top_a;
            let mut edge_b = top_b;
            edge_a.normal = Vec3::Y;
            edge_b.normal = Vec3::Y;
            triangles.push([center, edge_a, edge_b]);
        }
        if capped && bottom_radius > 0.0 {
            let center = PrimitiveVertex {
                position: Vec3::NEG_Y * half,
                normal: Vec3::NEG_Y,
            };
            let mut edge_a = bottom_a;
            let mut edge_b = bottom_b;
            edge_a.normal = Vec3::NEG_Y;
            edge_b.normal = Vec3::NEG_Y;
            triangles.push([center, edge_b, edge_a]);
        }
    }
    triangles
}

fn transform_matrix(position: Vec3, rotation: Vec3, scale: Vec3) -> Mat4 {
    Mat4::from_scale_rotation_translation(
        scale,
        Quat::from_euler(EulerRot::XYZ, rotation.x, rotation.y, rotation.z),
        position,
    )
}

fn dimensions(value: Option<&Vec<Number>>, default: [f32; 3]) -> Result<Vec3, String> {
    let size = vec3(value).unwrap_or(Vec3::from_array(default));
    if !size.is_finite() || size.min_element() <= 0.0 {
        return Err("box size must contain three positive finite numbers".into());
    }
    Ok(size)
}

fn vec3(value: Option<&Vec<Number>>) -> Option<Vec3> {
    let value = value?;
    (value.len() == 3).then(|| {
        Vec3::new(
            number(value.first()).unwrap_or(0.0),
            number(value.get(1)).unwrap_or(0.0),
            number(value.get(2)).unwrap_or(0.0),
        )
    })
}

fn scale(value: Option<&NumberOrNumbers>) -> Vec3 {
    match value {
        Some(NumberOrNumbers::Number(value)) => Vec3::splat(number(Some(value)).unwrap_or(1.0)),
        Some(NumberOrNumbers::Numbers(value)) => vec3(Some(value)).unwrap_or(Vec3::ONE),
        None => Vec3::ONE,
    }
}

fn number(value: Option<&Number>) -> Option<f32> {
    value.and_then(Number::as_f64).map(|value| value as f32)
}

fn positive(value: f32, name: &str) -> Result<f32, String> {
    if value.is_finite() && value > 0.0 {
        Ok(value)
    } else {
        Err(format!("{name} must be positive and finite"))
    }
}

fn non_negative(value: f32, name: &str) -> Result<f32, String> {
    if value.is_finite() && value >= 0.0 {
        Ok(value)
    } else {
        Err(format!("{name} must be non-negative and finite"))
    }
}

fn linear_rgba(hex: &str) -> Result<[f32; 4], String> {
    let [r, g, b] = linear_rgb(hex)?;
    Ok([r, g, b, 1.0])
}

fn linear_rgb(hex: &str) -> Result<[f32; 3], String> {
    let hex = hex
        .strip_prefix('#')
        .ok_or_else(|| format!("color {hex:?} must start with #"))?;
    let bytes = match hex.len() {
        3 => [
            u8::from_str_radix(&hex[0..1].repeat(2), 16),
            u8::from_str_radix(&hex[1..2].repeat(2), 16),
            u8::from_str_radix(&hex[2..3].repeat(2), 16),
        ],
        6 => [
            u8::from_str_radix(&hex[0..2], 16),
            u8::from_str_radix(&hex[2..4], 16),
            u8::from_str_radix(&hex[4..6], 16),
        ],
        _ => return Err(format!("color #{hex} must contain 3 or 6 hex digits")),
    };
    let bytes = bytes.map(|value| value.map_err(|_| format!("invalid hex color #{hex}")));
    let [r, g, b] = [bytes[0].clone()?, bytes[1].clone()?, bytes[2].clone()?];
    Ok([srgb_to_linear(r), srgb_to_linear(g), srgb_to_linear(b)])
}

fn srgb_to_linear(channel: u8) -> f32 {
    let value = channel as f32 / 255.0;
    if value <= 0.04045 {
        value / 12.92
    } else {
        ((value + 0.055) / 1.055).powf(2.4)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crystal::{ComponentDescriptor, EcsWorld, FieldSpec};
    use serde_json::json;
    use std::collections::BTreeMap;

    fn buffer_component(world: &mut EcsWorld, name: &str) -> crystal::ComponentId {
        world
            .register_component(ComponentDescriptor {
                name: name.into(),
                fields: BTreeMap::<String, FieldSpec>::new(),
                enableable: false,
                buffer: true,
                default: None,
            })
            .unwrap()
    }

    fn test_parameters() -> SceneParameters {
        SceneParameters {
            fov_y_degrees: 60.0,
            near: 0.1,
            far: 4000.0,
            sky_top: "#20152f".into(),
            sky_horizon: "#9a627d".into(),
            mesh_color: "#9aa0a6".into(),
            radial_segments: 24,
            camera_position: [0.0, 2.0, 22.0],
            camera_yaw: 0.0,
            camera_pitch: 0.0,
            cluster_error_threshold: 1.0,
            tick_dt: 1.0 / 60.0,
            first_light: first_light::FirstLightDefaults {
                sun_color: "#ffe2b0".into(),
                sun_intensity: 1.1,
                sun_position: [60.0, 90.0, 30.0],
                ambient_color: "#8fb3ff".into(),
                ambient_intensity: 0.32,
            },
        }
    }

    #[test]
    fn from_ecs_derives_world_space_box_vertices_and_spawn_camera() {
        let mut world = EcsWorld::default();
        let transform = buffer_component(&mut world, "transform");
        let mesh = buffer_component(&mut world, "mesh");
        let spawn = buffer_component(&mut world, "spawn");

        let spawn_entity = world
            .create_entity(vec![(spawn, json!({"position": [0, 2, 10], "yaw": 0}))])
            .unwrap();
        world.bind_gaia_id("known_spawn", spawn_entity).unwrap();

        // A 2×2×2 box centred at world (3, 0, -4): corners span [2,-1,-5]..[4,1,-3].
        let box_entity = world
            .create_entity(vec![
                (transform, json!({"position": [3, 0, -4]})),
                (
                    mesh,
                    json!({"parts": [{"shape": "box", "size": [2, 2, 2], "color": "#804020"}]}),
                ),
            ])
            .unwrap();
        world.bind_gaia_id("known_box", box_entity).unwrap();

        let scene = RenderScene::from_ecs(world, &test_parameters()).unwrap();

        // One box = one material chain; 12 tris ≤ shard budget → a single leaf.
        assert_eq!(scene.chains.len(), 1);
        assert_eq!(scene.chains[0].dag.leaf_tri_sum(), 12);

        // Camera reads the spawn pose verbatim.
        assert_eq!(scene.camera.eye, Vec3::new(0.0, 2.0, 10.0));
        assert_eq!(scene.camera.yaw, 0.0);

        // The Great Chain draw path expands the cut back to the box: 6 faces ×
        // 2 triangles × 3 vertices, world-space (a single leaf is always drawn).
        let vertices = scene.select_vertices(&scene.camera, 640);
        assert_eq!(vertices.len(), 36);

        // World-space AABB matches the authored box exactly (no camera-relative bake).
        let mut min = Vec3::splat(f32::INFINITY);
        let mut max = Vec3::splat(f32::NEG_INFINITY);
        for vertex in &vertices {
            let position = Vec3::from_array(vertex.position);
            min = min.min(position);
            max = max.max(position);
        }
        assert!(
            (min - Vec3::new(2.0, -1.0, -5.0)).length() < 1e-5,
            "min {min:?}"
        );
        assert!(
            (max - Vec3::new(4.0, 1.0, -3.0)).length() < 1e-5,
            "max {max:?}"
        );
    }

    #[test]
    fn first_light_reads_environment_sun_over_defaults() {
        let mut world = EcsWorld::default();
        let environment = buffer_component(&mut world, "environment");
        let env_entity = world
            .create_entity(vec![(
                environment,
                json!({"sun": {"color": "#ff0000", "intensity": 2.0, "position": [0, 10, 0]}}),
            )])
            .unwrap();
        world.bind_gaia_id("env", env_entity).unwrap();

        let scene = RenderScene::from_ecs(world, &test_parameters()).unwrap();
        let sun_color = scene.first_light.sun_color();
        // #ff0000 → linear red 1.0, others 0.0; intensity carried in w.
        assert!((sun_color[0] - 1.0).abs() < 1e-6);
        assert!(sun_color[1] < 1e-6 && sun_color[2] < 1e-6);
        assert!((sun_color[3] - 2.0).abs() < 1e-6);
        // Sun at +Y → direction toward sun is +Y.
        let direction = scene.first_light.sun_direction();
        assert!((direction[1] - 1.0).abs() < 1e-6);
    }

    // ---- Rite III ordeals: the Great Chain is THE geometry path ----

    use crystal::load_world_dir;
    use std::path::{Path, PathBuf};

    fn naruko_world() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../worlds/naruko")
    }

    fn naruko_scene() -> RenderScene {
        let mut world = EcsWorld::default();
        load_world_dir(naruko_world(), &mut world).expect("load the Naruko realm");
        RenderScene::from_ecs(world, &test_parameters()).expect("transmute the realm")
    }

    fn mat_key(color: &str, emissive: bool) -> MatKey {
        let rgb = linear_rgb(color).unwrap();
        let emissive = f32::from(emissive);
        (
            [rgb[0].to_bits(), rgb[1].to_bits(), rgb[2].to_bits()],
            emissive.to_bits(),
        )
    }

    /// Two independent transmutations of the realm produce identical Great
    /// Chains — same cluster count, byte-identical serialization (FORMAT.md
    /// determinism invariant). Cluster count is READ from the build, never
    /// hardcoded (it grows as the realm does).
    #[test]
    fn naruko_chain_is_deterministic_and_double_builds_byte_identical() {
        let first = naruko_scene();
        let second = naruko_scene();
        assert_eq!(
            first.chains.len(),
            second.chains.len(),
            "chain count stable"
        );
        assert!(!first.chains.is_empty(), "the realm has geometry");

        let mut total_clusters = 0usize;
        for (a, b) in first.chains.iter().zip(&second.chains) {
            assert_eq!(a.color, b.color, "chain material order stable");
            let bytes_a = transmutation::serialize(&a.dag).expect("serialize chain A");
            let bytes_b = transmutation::serialize(&b.dag).expect("serialize chain B");
            assert_eq!(bytes_a, bytes_b, "double build must be byte-identical");
            total_clusters += a.dag.clusters.len();
        }
        eprintln!(
            "[ordeal] Naruko Great Chain: {} chains, {} clusters",
            first.chains.len(),
            total_clusters
        );
        assert!(
            total_clusters >= first.chains.len(),
            "each chain has ≥1 cluster"
        );
    }

    /// Draw-parity band assert: the WHOLE draw path (static cut ∪ the living
    /// layer) still carries every signature material of the keyart. The lantern
    /// rose and the lit beacon now ride the DYNAMIC pass (they carry behaviors),
    /// so the union — not the static cut alone — must preserve them.
    #[test]
    fn naruko_selected_cut_preserves_every_material_band() {
        let scene = naruko_scene();
        let vertices = scene.select_vertices(&scene.camera, 640);
        assert!(!vertices.is_empty(), "the cut drew geometry");

        let key = |v: &Vertex| -> MatKey {
            (
                [
                    v.color[0].to_bits(),
                    v.color[1].to_bits(),
                    v.color[2].to_bits(),
                ],
                v.emissive.to_bits(),
            )
        };
        let static_present: std::collections::BTreeSet<MatKey> = vertices.iter().map(key).collect();
        let mut present = static_present.clone();
        // The living layer draws its own bind-baked leaves — fold them in.
        for entity in scene.dynamics.entities() {
            present.extend(entity.leaf_vertices().iter().map(key));
        }

        for (label, color, emissive) in [
            ("pier brown", "#4a3626", false),
            ("lantern rose", "#ff9db0", true),
            ("warm window", "#ffb46b", true),
            ("lit lamp", "#f3e9ff", true),
        ] {
            assert!(
                present.contains(&mat_key(color, emissive)),
                "the draw path lost the {label} band ({color}, emissive={emissive})"
            );
        }

        // The dynamic materials must NOT leak into the static cut (split clean).
        assert!(
            !static_present.contains(&mat_key("#ff9db0", true)),
            "lantern rose must have left the static chains (it is dynamic)"
        );
        assert!(
            !static_present.contains(&mat_key("#f3e9ff", true)),
            "lit beacon must have left the static chains (it is dynamic)"
        );

        // Sky gradient endpoints intact (linear sRGB of the night preset).
        assert_eq!(scene.sky_top, linear_rgba("#2a1a3e").unwrap());
        assert_eq!(scene.sky_horizon, linear_rgba("#d98ba8").unwrap());
    }

    /// DYNAMIC SPLIT correctness: entities carrying a `behavior` are excluded
    /// from the static chains and rendered by the dynamic pass, with NO triangle
    /// lost or duplicated. Naruko carries the lantern (bob) + beacon (pulse).
    #[test]
    fn dynamic_split_is_clean_and_leaf_parity_holds() {
        let scene = naruko_scene();
        assert_eq!(
            scene.dynamics.entities().len(),
            2,
            "the realm breath: lantern + beacon are the two dynamic entities"
        );

        // Every chain's leaf triangles == its transmute input (loss-free), on
        // BOTH sides of the split. Their sum is the undivided realm's triangle
        // count — nothing dropped by routing an entity to the dynamic pass.
        let mut static_leaf = 0usize;
        for chain in &scene.chains {
            assert_eq!(
                chain.dag.leaf_tri_sum(),
                chain.dag.input_tri_count as usize,
                "static chain is loss-free"
            );
            static_leaf += chain.dag.leaf_tri_sum();
        }
        let mut dyn_leaf = 0usize;
        let mut dyn_keys = std::collections::BTreeSet::<MatKey>::new();
        for entity in scene.dynamics.entities() {
            for chain in &entity.chains {
                assert_eq!(
                    chain.dag.leaf_tri_sum(),
                    chain.dag.input_tri_count as usize,
                    "dynamic chain is loss-free"
                );
                dyn_leaf += chain.dag.leaf_tri_sum();
                dyn_keys.insert((
                    [
                        chain.color[0].to_bits(),
                        chain.color[1].to_bits(),
                        chain.color[2].to_bits(),
                    ],
                    chain.emissive.to_bits(),
                ));
            }
        }
        assert!(dyn_leaf > 0, "dynamic entities carry geometry");
        let _ = &dyn_keys; // (materials may repeat across entities; parity is by triangle)

        // INDEPENDENT total: rebuild the SAME realm with every `behavior`
        // stripped — now everything is static. static_leaf + dyn_leaf must equal
        // this undivided leaf count exactly: the split neither drops (would fall
        // short) nor duplicates (would exceed) a single behavior-entity triangle.
        let undivided = {
            let mut world = EcsWorld::default();
            load_world_dir(naruko_world(), &mut world).expect("load the Naruko realm");
            if let Some(behavior) = world.component_id("behavior") {
                let carriers = world.query(&QuerySpec {
                    all: vec![behavior],
                    ..Default::default()
                });
                for entity in carriers {
                    world.remove_component(entity, behavior).unwrap();
                }
            }
            let all_static = RenderScene::from_ecs(world, &test_parameters()).unwrap();
            assert!(
                all_static.dynamics.entities().is_empty(),
                "stripping behaviors leaves no dynamic entities"
            );
            all_static
                .chains
                .iter()
                .map(|c| c.dag.leaf_tri_sum())
                .sum::<usize>()
        };
        assert_eq!(
            static_leaf + dyn_leaf,
            undivided,
            "static + dynamic leaves == the undivided realm's leaves (no loss, no dup)"
        );

        // RENDER vertex-count parity: static finest cut + every dynamic entity's
        // leaves == (static_leaf + dyn_leaf) triangles == the whole realm's leaves.
        let mut render_scene = naruko_scene();
        render_scene.error_threshold = 0.0;
        let static_verts = render_scene
            .select_vertices(&render_scene.camera, 640)
            .len();
        let dyn_verts: usize = render_scene
            .dynamics
            .entities()
            .iter()
            .map(|e| e.leaf_vertices().len())
            .sum();
        assert_eq!(
            static_verts,
            static_leaf * 3,
            "static finest cut == static leaves"
        );
        assert_eq!(
            dyn_verts,
            dyn_leaf * 3,
            "dynamic pass draws every dynamic leaf"
        );
        assert_eq!(
            (static_verts + dyn_verts) / 3,
            static_leaf + dyn_leaf,
            "static + dynamic == total realm leaves"
        );
        eprintln!(
            "[ordeal] dynamic split: {} static chains ({static_leaf} leaves) + {} dynamic entities ({dyn_leaf} leaves) = {} total",
            scene.chains.len(),
            scene.dynamics.entities().len(),
            static_leaf + dyn_leaf
        );
    }

    /// TICK DETERMINISM: the clock counts ticks, never wall time. Two runs of N
    /// ticks produce byte-identical model-transform buffers at every step.
    #[test]
    fn tick_determinism_byte_identical_model_buffer() {
        let run = || {
            let mut scene = naruko_scene();
            let mut bytes = Vec::new();
            for _ in 0..300u64 {
                scene.tick();
                for m in scene.dynamics.model_matrices() {
                    bytes.extend_from_slice(bytemuck::bytes_of(&m));
                }
            }
            bytes
        };
        let a = run();
        let b = run();
        assert_eq!(a.len(), b.len(), "model-buffer stream length");
        assert_eq!(a, b, "model transforms must be byte-identical across runs");
        eprintln!(
            "[ordeal] tick determinism: 2 runs × 300 ticks, {} bytes, byte-identical",
            a.len()
        );
    }

    /// BOB math matches KAMI's ordeal formula THROUGH the full pipeline:
    /// data → `tick_decorative` → ECS transform → model transform. The lantern's
    /// model y-translation must equal `sin(t·speed+phase)·amplitude` at every tick.
    #[test]
    fn bob_matches_kami_through_the_pipeline() {
        let dt = 1.0f64 / 60.0;
        let bob = kami::Decorative::Bob {
            speed: 0.8,
            phase: 0.0,
            amplitude: 0.12,
        };
        // The lantern's authored bind pose (transform position [-7.5, 0, 20]).
        let bind = kami::BindPose {
            position: [-7.5, 0.0, 20.0],
            ..kami::BindPose::default()
        };
        let mut scene = naruko_scene();
        let mut worst = 0.0f64;
        for k in 0..240u64 {
            scene.tick(); // after this call clock==k+1, model reflects t = k*dt
            let t = k as f64 * dt;
            let want_dy = bob.eval(t, bind).position[1]; // == sin(t*0.8)*0.12
            let lantern = scene
                .dynamics
                .entities()
                .iter()
                .find(|e| e.gaia_id == "naruko_lantern")
                .expect("the lantern is a dynamic entity");
            // model = M(animated) * M(bind)⁻¹ = pure y-translation for a bob.
            let got_dy = lantern.model.to_cols_array()[13] as f64;
            worst = worst.max((got_dy - want_dy).abs());
            // f32 storage floor at |y| ≤ 0.12 → ulp ≈ 1.4e-8; 1e-5 is ample.
            assert!(
                (got_dy - want_dy).abs() <= 1e-5,
                "tick {k}: model dy {got_dy} != kami bob {want_dy}"
            );
            // and the bob never exceeds the authored amplitude (a sway, not a buoy)
            assert!(got_dy.abs() <= 0.12 + 1e-6, "bob within amplitude 0.12");
        }
        eprintln!("[ordeal] bob pipeline parity: 240 ticks vs kami eval, worst err={worst:.3e}");
    }

    /// At τ → 0 the cut selects the finest LOD everywhere: the emitted triangle
    /// count equals the summed leaf triangles of every chain (geometry parity —
    /// leaves are the loss-free shardized input).
    #[test]
    fn finest_threshold_reproduces_leaf_geometry() {
        let mut scene = naruko_scene();
        scene.error_threshold = 0.0;
        let leaf_tris: usize = scene.chains.iter().map(|c| c.dag.leaf_tri_sum()).sum();
        let vertices = scene.select_vertices(&scene.camera, 640);
        assert_eq!(vertices.len(), leaf_tris * 3, "finest cut == all leaves");
    }
}
