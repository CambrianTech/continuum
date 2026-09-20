//! `scene::instantiate` — the single seam where a backend-neutral
//! [`SceneDescription`] becomes a live Bevy render graph.
//!
//! This is the ONE injection point folding what used to be scattered across
//! `builder::build_scene` + the ~200-line `commands.rs` Load arm into a single
//! tree walk. A [`SceneDescription`] (produced by the file loader, the
//! [`super::builder_api::SceneBuilder`], or the [`super::birther`]) is walked
//! pre-order; each [`SceneNode`] spawns the ECS entities its payload needs,
//! parented under its tree parent so Bevy's transform propagation composes the
//! hierarchy for free.
//!
//! ## Why a `Result`
//!
//! The Bevy backend does NOT instantiate every payload the description *type*
//! can express (e.g. per-scene light nodes — this backend supplies lighting via
//! a shared global rig; see [`spawn_global_lights`]). Rather than silently drop
//! an unsupported node ([[fallbacks-are-illegal-fail-loud]]), the walk returns
//! `Err` naming the node and why. The caller aborts the load loudly instead of
//! rendering a half-built scene.
//!
//! ## The one regression this file must not reintroduce
//!
//! The avatar's `SceneInstanceReady` observer flips `model_loaded` via
//! `slot_registry.avatar_mut(&identity)`. That lookup key is the node's `id`.
//! So an avatar node's `id` MUST equal both the `objects` map key the caller
//! inserts AND the identity captured by the observer, or `model_loaded` stays
//! false forever and the slot never goes active. Instantiate keeps all three
//! sourced from the single `node.id` so they cannot drift.

use bevy::camera::visibility::RenderLayers;
use bevy::camera::ClearColorConfig;
use bevy::prelude::*;
use bevy::scene::SceneInstanceReady;

use super::animation::AnimationConfig;
use super::avatar::AvatarObject;
use super::builder::{SceneLight, SceneMarker};
use super::description::{
    default_portrait_lights, AnimationProfileKind, AvatarPayload, LightDesc, LightKind,
    NodePayload, PropPayload, SceneDescription, SceneNode,
};
use super::object::{PropSceneObject, SceneObject};
use super::physics::PhysicsBackend;
use super::room::RoomConfig;

use crate::gpu::make_entry;
use crate::gpu::memory_manager::{GpuPriority, GpuSubsystem};
use crate::live::video::bevy_renderer::animation::{
    BreathingAnimation, CameraHeadLock, IdleMotion, ModelPath, SlotId,
};
use crate::live::video::bevy_renderer::api::gpu_manager;
use crate::live::video::bevy_renderer::types::{
    GpuGuards, PendingLoadEntry, PendingLoads, SlotRegistry, SnapshotTracker,
};
use crate::live::video::bevy_renderer::{coordinate, mesh_fixup, skeleton};
use crate::{clog_info, clog_warn};

/// Bevy's forward+ pipeline budgets a fixed number of directional lights across
/// the whole view. The global portrait rig uses three; instantiate refuses to
/// birth a scene whose *own* directional lights alone would blow the budget
/// rather than let Bevy silently clamp them.
const MAX_DIRECTIONAL_LIGHTS: usize = 10;

/// The live handles a caller needs after instantiating a scene: the root entity
/// to store for teardown, the camera to toggle active, and the boxed objects to
/// register in the slot (keyed by node id — see the module doc's regression
/// note). Backend-neutral in spirit; the entities are Bevy's because this IS the
/// Bevy backend's instantiation.
pub struct SceneInstance {
    /// Scene root entity (carries `SceneMarker`); despawning it recursively
    /// tears down the whole scene.
    pub scene_root: Entity,
    /// The scene camera, if the description declared one.
    pub camera_entity: Option<Entity>,
    /// Boxed scene objects keyed by their node id, for the caller to insert into
    /// the slot registry. The key MUST be the node id (== observer identity).
    pub objects: Vec<(String, Box<dyn SceneObject>)>,
}

/// The ECS-side inputs instantiate needs that aren't in the `SceneDescription`:
/// which slot + render target + layer the scene renders into, the pending-load
/// tracker the avatar path pushes handles onto, and the physics backend
/// physics-carrying nodes bind through.
pub struct InstantiateParams<'a> {
    pub slot: u8,
    pub render_target: Handle<Image>,
    pub layer: RenderLayers,
    pub pending: &'a mut PendingLoads,
    pub physics: &'a dyn PhysicsBackend,
}

/// Walk a [`SceneDescription`] into a live Bevy graph. See the module docs.
///
/// The description `root` is treated as the scene container: it becomes the
/// `SceneMarker`-tagged root entity regardless of its payload, and its children
/// are instantiated beneath it. Fails loud (no partial scene) on any node this
/// backend cannot instantiate.
pub fn build_scene_from_description(
    commands: &mut Commands,
    asset_server: &AssetServer,
    desc: &SceneDescription,
    params: InstantiateParams,
) -> Result<SceneInstance, String> {
    let backdrop: Color = desc.backdrop.into();
    let root_transform: Transform = desc.root.transform.into();

    let scene_root = commands
        .spawn((
            SceneMarker {
                slot_id: params.slot,
            },
            root_transform,
            Visibility::default(),
            params.layer.clone(),
        ))
        .id();

    let mut walk = SceneWalk {
        commands,
        asset_server,
        slot: params.slot,
        layer: params.layer.clone(),
        render_target: params.render_target,
        backdrop,
        pending: params.pending,
        physics: params.physics,
        scene_root,
        objects: Vec::new(),
        camera_entity: None,
    };

    for child in &desc.root.children {
        walk.spawn_subtree(child, scene_root)?;
    }

    Ok(SceneInstance {
        scene_root,
        camera_entity: walk.camera_entity,
        objects: walk.objects,
    })
}

/// Mutable state threaded through the recursive tree walk. Bundled into a struct
/// so the recursion is `&mut self` rather than a dozen threaded arguments.
struct SceneWalk<'a, 'w, 's> {
    commands: &'a mut Commands<'w, 's>,
    asset_server: &'a AssetServer,
    slot: u8,
    layer: RenderLayers,
    render_target: Handle<Image>,
    backdrop: Color,
    pending: &'a mut PendingLoads,
    physics: &'a dyn PhysicsBackend,
    scene_root: Entity,
    objects: Vec<(String, Box<dyn SceneObject>)>,
    camera_entity: Option<Entity>,
}

impl SceneWalk<'_, '_, '_> {
    /// Instantiate `node` as a child of `parent`, then recurse into its children.
    fn spawn_subtree(&mut self, node: &SceneNode, parent: Entity) -> Result<(), String> {
        let entity = self.spawn_node(node, parent)?;

        // Bind physics onto the freshly-spawned entity (no-op unless a real
        // PhysicsBackend is installed — the base engine's default is inert).
        if let Some(physics) = &node.physics {
            let transform: Transform = node.transform.into();
            self.physics
                .attach(self.commands, entity, &transform, physics);
        }

        for child in &node.children {
            self.spawn_subtree(child, entity)?;
        }
        Ok(())
    }

    /// Spawn the ECS entity for one node's payload, parenting it under `parent`,
    /// and return the entity children should parent under.
    fn spawn_node(&mut self, node: &SceneNode, parent: Entity) -> Result<Entity, String> {
        let transform: Transform = node.transform.into();
        match &node.payload {
            NodePayload::Group => {
                let entity = self
                    .commands
                    .spawn((transform, Visibility::default(), self.layer.clone()))
                    .id();
                self.commands.entity(parent).add_child(entity);
                Ok(entity)
            }
            NodePayload::Camera(cam) => {
                let entity = self
                    .commands
                    .spawn((
                        Camera3d::default(),
                        Camera {
                            order: self.slot as isize,
                            clear_color: ClearColorConfig::Custom(self.backdrop),
                            is_active: false,
                            ..default()
                        },
                        bevy::camera::RenderTarget::Image(self.render_target.clone().into()),
                        bevy::core_pipeline::tonemapping::Tonemapping::None,
                        Msaa::Off,
                        transform,
                        self.layer.clone(),
                        SlotId(self.slot),
                    ))
                    .id();
                // Head-lock is opt-in: only when the payload asks does the camera
                // pin its height to the discovered head world-Y (head_y starts
                // None, filled once bones are discovered). Absent the component,
                // the camera holds its authored transform verbatim.
                if cam.head_lock {
                    self.commands
                        .entity(entity)
                        .insert(CameraHeadLock { head_y: None });
                }
                self.commands.entity(parent).add_child(entity);
                self.camera_entity = Some(entity);
                Ok(entity)
            }
            NodePayload::Avatar(payload) => self.spawn_avatar(node, payload, parent, transform),
            NodePayload::Prop(payload) => self.spawn_prop(node, payload, parent, transform),
            NodePayload::Environment(env) => {
                // The environment is not its own object: it configures the scene
                // root's RoomConfig, and `room::populate_rooms` spawns the glTF
                // backdrop as a child of the root (recursively despawned on
                // teardown). Its node transform is intentionally not applied —
                // the backdrop loads at the scene origin. Children parent under
                // the scene root.
                let asset_path = env.asset.source.clone();
                self.commands.entity(self.scene_root).insert(RoomConfig {
                    asset_path,
                    layer: self.layer.clone(),
                    scene_id: env.scene_id.clone(),
                });
                Ok(self.scene_root)
            }
            NodePayload::Light(light) => Err(unsupported_light(&node.id, light)),
        }
    }

    /// The avatar spawn path — VRM→glb symlink, async scene/gltf loads, the
    /// coordinate-space correction, animation components, and the
    /// `SceneInstanceReady` observer (kept verbatim from the old Load arm).
    fn spawn_avatar(
        &mut self,
        node: &SceneNode,
        payload: &AvatarPayload,
        parent: Entity,
        node_transform: Transform,
    ) -> Result<Entity, String> {
        let identity = node.id.clone();
        let model_path = payload.asset.source.clone();
        let display_name = payload.display_name.clone();

        // Bevy's glTF loader requires a .glb/.gltf extension. VRM files are
        // glTF-compatible — create a sibling .glb symlink (or copy) if needed.
        let load_path = if model_path.ends_with(".vrm") {
            let glb_path = model_path.replacen(".vrm", ".glb", 1);
            if !std::path::Path::new(&glb_path).exists()
                && std::path::Path::new(&model_path).exists()
            {
                let vrm_filename = std::path::Path::new(&model_path)
                    .file_name()
                    .unwrap_or_default();
                #[cfg(unix)]
                {
                    #[cfg(unix)]
                    let _ = std::os::unix::fs::symlink(vrm_filename, &glb_path);
                    // Windows symlinks need privilege; a copy serves the same
                    // purpose (bevy just needs the bytes at the .glb path).
                    #[cfg(windows)]
                    let _ = std::os::windows::fs::symlink_file(vrm_filename, &glb_path)
                        .or_else(|_| std::fs::copy(vrm_filename, &glb_path).map(|_| ()));
                }
                #[cfg(not(unix))]
                {
                    let _ = std::fs::copy(&model_path, &glb_path);
                }
            }
            glb_path
        } else {
            model_path.clone()
        };

        let mut avatar =
            AvatarObject::new(model_path.clone(), display_name.clone(), identity.clone());

        let asset_path = format!("{}#Scene0", load_path);
        let scene_handle: Handle<Scene> = self.asset_server.load(&asset_path);
        let gltf_handle: Handle<bevy::gltf::Gltf> = self.asset_server.load(&load_path);
        clog_info!(
            "🎨 Slot {}: loading '{}' from {}",
            self.slot,
            display_name,
            load_path
        );
        self.pending.scene_handles.push(PendingLoadEntry {
            slot: self.slot,
            handle: scene_handle.clone(),
            path: asset_path,
            logged_final: false,
        });
        self.pending.gltf_handles.push(PendingLoadEntry {
            slot: self.slot,
            handle: gltf_handle.clone(),
            path: load_path.clone(),
            logged_final: false,
        });
        avatar.state.gltf_handle = Some(gltf_handle);

        // Coordinate adapter: translate the model's authored space into our
        // canonical (glTF/Bevy) space so ANY model kind presents face-on and
        // upright, composed with the node's placement transform (correction is
        // applied in the model's own space, then the placement).
        let correction = coordinate::detect_convention(&load_path)
            .correction()
            .to_transform();
        let model_transform = node_transform.mul_transform(correction);

        let animation = match payload.animation {
            AnimationProfileKind::Portrait => AnimationConfig::portrait(self.slot),
            AnimationProfileKind::FullBody => AnimationConfig::full_body(self.slot),
            AnimationProfileKind::Minimal => AnimationConfig::minimal(),
        };

        let avatar_entity = self
            .commands
            .spawn((
                SceneRoot(scene_handle),
                model_transform,
                self.layer.clone(),
                SlotId(self.slot),
                animation,
                ModelPath(load_path.clone()),
                BreathingAnimation::new(self.slot),
                IdleMotion::new(self.slot),
            ))
            .id();
        self.commands.entity(parent).add_child(avatar_entity);

        let layer_for_observer = self.layer.clone();
        let slot_for_observer = self.slot;
        let model_path_for_observer = load_path.clone();
        let identity_for_observer = identity.clone();
        self.commands.entity(avatar_entity).observe(
            move |event: On<SceneInstanceReady>,
                  children_query: Query<&Children>,
                  names: Query<&Name>,
                  mut transforms: Query<&mut Transform>,
                  mesh_handles: Query<&Mesh3d>,
                  mut meshes: ResMut<Assets<Mesh>>,
                  mut cmds: Commands,
                  mut slot_registry: ResMut<SlotRegistry>,
                  mut gpu_guards: ResMut<GpuGuards>,
                  mut snapshots: ResMut<SnapshotTracker>| {
                let root = event.entity;
                let child_count = skeleton::count_descendants(root, &children_query);
                skeleton::propagate_render_layers(
                    root,
                    &layer_for_observer,
                    &children_query,
                    &mut cmds,
                );
                skeleton::dump_bone_names(root, &children_query, &names);
                skeleton::fix_tpose_arms(root, &children_query, &names, &mut transforms);

                // Strip VRoid's degenerate (0,0,0,0) COLOR_0 vertex attribute
                // that renders skin/surfaces black under Bevy's PBR pipeline.
                // See `mesh_fixup` — the material JSON is clean; this is a mesh bug.
                let fixed = mesh_fixup::strip_degenerate_vertex_colors(
                    root,
                    &children_query,
                    &mesh_handles,
                    &mut meshes,
                );
                if fixed > 0 {
                    clog_info!(
                        "🎨 mesh_fixup: stripped degenerate COLOR_0 from {} mesh(es) on slot {}",
                        fixed,
                        slot_for_observer
                    );
                }

                let bones = skeleton::discover_bones(
                    root,
                    slot_for_observer,
                    &model_path_for_observer,
                    &children_query,
                    &names,
                    &transforms,
                );
                cmds.entity(root).insert(bones);

                if let Some(slot_data) = slot_registry.slots.get_mut(&slot_for_observer) {
                    if let Some(avatar) = slot_data.avatar_mut(&identity_for_observer) {
                        avatar.state.model_loaded = true;
                    }
                }

                snapshots.mark_loaded(slot_for_observer);

                let model_bytes = std::fs::metadata(&model_path_for_observer)
                    .map(|m| m.len())
                    .unwrap_or(0);
                if model_bytes > 0 {
                    if let Some(mgr) = gpu_manager() {
                        match mgr.allocate(
                            GpuSubsystem::Rendering,
                            model_bytes,
                            GpuPriority::Interactive,
                        ) {
                            Ok(guard) => {
                                mgr.eviction_registry.register(make_entry(
                                    &format!("render:model:slot{}", slot_for_observer),
                                    &format!("Avatar Model (slot {})", slot_for_observer),
                                    GpuPriority::Interactive,
                                    model_bytes,
                                ));
                                gpu_guards.model_guards.insert(slot_for_observer, guard);
                            }
                            Err(e) => {
                                clog_warn!(
                                    "🎨 GPU: model allocation for slot {} failed ({})",
                                    slot_for_observer,
                                    e
                                );
                            }
                        }
                    }
                }

                clog_info!(
                    "🎨 SceneInstanceReady: slot {}, entity {:?}, {} descendants",
                    slot_for_observer,
                    root,
                    child_count
                );
            },
        );

        avatar.entity = Some(avatar_entity);
        self.objects.push((identity, Box::new(avatar)));
        Ok(avatar_entity)
    }

    /// The prop spawn path — a static glTF/GLB with no skeleton, no animation.
    /// "Loaded" means its entity exists; a tiny observer only propagates render
    /// layers so the scene camera can see it.
    fn spawn_prop(
        &mut self,
        node: &SceneNode,
        payload: &PropPayload,
        parent: Entity,
        transform: Transform,
    ) -> Result<Entity, String> {
        let source = &payload.asset.source;
        let asset_path = if source.contains('#') {
            source.clone()
        } else {
            format!("{}#Scene0", source)
        };
        let scene_handle: Handle<Scene> = self.asset_server.load(&asset_path);

        let layer_for_observer = self.layer.clone();
        let prop_id = node.id.clone();
        let prop_entity = self
            .commands
            .spawn((
                SceneRoot(scene_handle.clone()),
                transform,
                self.layer.clone(),
            ))
            .observe(
                move |event: On<SceneInstanceReady>,
                      children_query: Query<&Children>,
                      mut cmds: Commands| {
                    let root = event.entity;
                    skeleton::propagate_render_layers(
                        root,
                        &layer_for_observer,
                        &children_query,
                        &mut cmds,
                    );
                    clog_info!("🪑 Prop '{}' ready", prop_id);
                },
            )
            .id();
        self.commands.entity(parent).add_child(prop_entity);

        let mut prop = PropSceneObject::new(source.clone());
        prop.entity = Some(prop_entity);
        prop.handle = Some(scene_handle);
        self.objects.push((node.id.clone(), Box::new(prop)));
        Ok(prop_entity)
    }
}

/// The fail-loud message for a per-scene light node — this backend supplies
/// lighting globally (see [`spawn_global_lights`]), so an authored per-scene
/// light is refused rather than silently dropped.
fn unsupported_light(id: &str, light: &LightDesc) -> String {
    format!(
        "scene light node '{id}' ({:?}) is not instantiated by the Bevy backend: \
         lighting is supplied by a shared global rig (see \
         scene::instantiate::spawn_global_lights). Remove the light node, or add \
         per-scene light support to this backend.",
        light.kind
    )
}

/// Spawn the shared global light rig from the single [`default_portrait_lights`]
/// data source, visible on every avatar layer. Bevy budgets directional lights
/// across the whole view, so the rig is global (shared across all slots) rather
/// than per-scene — over-budget directional lights fail loud instead of being
/// silently clamped.
pub fn spawn_global_lights(commands: &mut Commands, max_slots: u8) {
    let all_layers: Vec<usize> = (1..=(max_slots as usize)).collect();
    let layers = RenderLayers::from_layers(&all_layers);

    let rig = default_portrait_lights();
    let directional_count = rig
        .iter()
        .filter(|(l, _)| matches!(l.kind, LightKind::Directional))
        .count();
    if directional_count > MAX_DIRECTIONAL_LIGHTS {
        clog_warn!(
            "🔦 global light rig declares {} directional lights, over the {} budget — \
             Bevy will clamp; trim default_portrait_lights()",
            directional_count,
            MAX_DIRECTIONAL_LIGHTS
        );
    }

    for (light, xf) in rig {
        let color: Color = light.color.into();
        let transform: Transform = xf.into();
        match light.kind {
            LightKind::Ambient => {
                commands.spawn(AmbientLight {
                    color,
                    brightness: light.intensity,
                    affects_lightmapped_meshes: false,
                });
            }
            LightKind::Directional => {
                commands.spawn((
                    DirectionalLight {
                        illuminance: light.intensity,
                        shadows_enabled: false,
                        color,
                        ..default()
                    },
                    transform,
                    layers.clone(),
                    SceneLight,
                ));
            }
            LightKind::Point { range } => {
                commands.spawn((
                    PointLight {
                        intensity: light.intensity,
                        range,
                        shadows_enabled: false,
                        color,
                        ..default()
                    },
                    transform,
                    layers.clone(),
                    SceneLight,
                ));
            }
            LightKind::Spot {
                range,
                inner_angle,
                outer_angle,
            } => {
                commands.spawn((
                    SpotLight {
                        intensity: light.intensity,
                        range,
                        inner_angle,
                        outer_angle,
                        shadows_enabled: false,
                        color,
                        ..default()
                    },
                    transform,
                    layers.clone(),
                    SceneLight,
                ));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::live::video::bevy_renderer::scene::description::{
        AssetRef, ColorDesc, LightDesc, SceneNode,
    };

    // what this catches: a per-scene light node silently vanishing instead of
    // failing loud — the message must name the node id and point at the global
    // rig so the author knows why their light didn't render.
    #[test]
    fn unsupported_light_names_the_node_and_the_rig() {
        let msg = unsupported_light(
            "key",
            &LightDesc {
                kind: LightKind::Directional,
                color: ColorDesc::WHITE,
                intensity: 30000.0,
            },
        );
        assert!(msg.contains("'key'"), "must name the node: {msg}");
        assert!(
            msg.contains("spawn_global_lights"),
            "must point at the rig: {msg}"
        );
    }

    // what this catches: the default portrait rig drifting over Bevy's
    // directional-light budget — the global rig must stay within budget or
    // avatars silently lose lights.
    #[test]
    fn default_rig_is_within_the_directional_budget() {
        let directional = default_portrait_lights()
            .iter()
            .filter(|(l, _)| matches!(l.kind, LightKind::Directional))
            .count();
        assert!(
            directional <= MAX_DIRECTIONAL_LIGHTS,
            "portrait rig has {directional} directional lights, over budget {MAX_DIRECTIONAL_LIGHTS}"
        );
    }

    // what this catches: an Avatar node payload losing the fields the spawn path
    // reads (asset source, display name) — a compile-time guard that the payload
    // shape instantiate depends on is intact.
    #[test]
    fn avatar_node_carries_the_fields_instantiate_reads() {
        let node = SceneNode::leaf(
            "asha",
            NodePayload::Avatar(AvatarPayload {
                asset: AssetRef::path("models/avatars/asha.vrm"),
                display_name: "Asha".to_string(),
                animation: AnimationProfileKind::Portrait,
            }),
        );
        assert_eq!(node.id, "asha");
        match node.payload {
            NodePayload::Avatar(p) => {
                assert_eq!(p.asset.source, "models/avatars/asha.vrm");
                assert_eq!(p.display_name, "Asha");
            }
            _ => panic!("expected avatar payload"),
        }
    }
}
