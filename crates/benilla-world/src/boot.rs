//! The tuned Bevy boot: the `DefaultPlugins` set every benilla binary stands on. The engine tuning
//! is shared; the `Window` is the caller's.

use bevy::app::{PluginGroupBuilder, TaskPoolOptions, TaskPoolPlugin};
use bevy::prelude::*;

use crate::thread_qos;

/// Quiet wgpu/naga. On iOS also `bevy_winit::system`, whose `changed_windows` logs an `error!` for
/// every cursor position the input plugin publishes (winit's iOS `set_cursor_position` is `NotSupported`).
#[cfg(not(target_os = "ios"))]
const LOG_FILTER: &str = "wgpu=error,naga=warn";
#[cfg(target_os = "ios")]
const LOG_FILTER: &str = "wgpu=error,naga=warn,bevy_winit::system=off";

/// The log ring; on iOS also `$BENILLA_HOME/Logs/client.log`, truncated at launch, since no console
/// of a device reaches a tool without root. It sits before the plugin's level filter, which applies
/// to every layer alike.
fn custom_layer(_: &mut App) -> Option<bevy::log::BoxedLayer> {
    #[cfg(target_os = "ios")]
    if let Some(file) = ios_log_file() {
        use tracing_subscriber::Layer;
        let file_layer = tracing_subscriber::fmt::layer()
            .with_writer(move || file.try_clone().expect("log file handle"))
            .with_ansi(false);
        return Some(Box::new(vec![
            crate::log_ring::LogRing.boxed(),
            file_layer.boxed(),
        ]));
    }
    Some(Box::new(crate::log_ring::LogRing))
}

/// Opens (truncating) the client log. `BENILLA_HOME` is set by `crates/benilla/src/main.rs`;
/// benilla-world cannot reach `benilla_app::local_state`, so the same fallback is repeated here.
#[cfg(target_os = "ios")]
fn ios_log_file() -> Option<std::fs::File> {
    let home = std::env::var_os("BENILLA_HOME")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| {
            let h = std::env::var("HOME").unwrap_or_default();
            std::path::PathBuf::from(h).join("Documents/benilla-config")
        });
    let dir = home.join("Logs");
    std::fs::create_dir_all(&dir).ok()?;
    std::fs::File::create(dir.join("client.log")).ok()
}

/// `DefaultPlugins` with benilla's engine tuning applied, around the caller's primary window.
pub fn tuned_default_plugins(primary_window: Window) -> PluginGroupBuilder {
    DefaultPlugins
        .set(WindowPlugin {
            primary_window: Some(primary_window),
            ..default()
        })
        // Deliberately no `AssetPlugin::file_path`: a baked source path resolves only on the build
        // machine. Every shader is embedded (`embedded://<crate>/shaders/…`), so no root is read.
        // Quiet wgpu/naga; the ring keeps the last stderr lines for the crash report (`log_ring`).
        .set(bevy::log::LogPlugin {
            filter: LOG_FILTER.into(),
            custom_layer,
            ..default()
        })
        // Asset loads parse synchronously on the IO pool, and Bevy's default 4 threads saturate on
        // a dense teleport. Workers spawn at default QoS, behind any background build:
        // compute is user-interactive, IO and async compute user-initiated, and
        // `ThreadQosPlugin` promotes the render thread from inside.
        .set(TaskPoolPlugin {
            task_pool_options: TaskPoolOptions {
                io: bevy::app::TaskPoolThreadAssignmentPolicy {
                    min_threads: 2,
                    max_threads: 8,
                    percent: 0.5,
                    on_thread_spawn: Some(std::sync::Arc::new(|| {
                        thread_qos::promote_current_thread(thread_qos::QosClass::UserInitiated)
                    })),
                    on_thread_destroy: None,
                },
                async_compute: bevy::app::TaskPoolThreadAssignmentPolicy {
                    on_thread_spawn: Some(std::sync::Arc::new(|| {
                        thread_qos::promote_current_thread(thread_qos::QosClass::UserInitiated)
                    })),
                    ..TaskPoolOptions::default().async_compute
                },
                compute: bevy::app::TaskPoolThreadAssignmentPolicy {
                    on_thread_spawn: Some(std::sync::Arc::new(|| {
                        thread_qos::promote_current_thread(thread_qos::QosClass::UserInteractive)
                    })),
                    // `WOW_THREADS=1` serialises the frame's systems, a diagnostic: a defect that
                    // survives it is not a race between two systems.
                    max_threads: match std::env::var("WOW_THREADS").ok().as_deref() {
                        Some("1") => 1,
                        _ => TaskPoolOptions::default().compute.max_threads,
                    },
                    ..TaskPoolOptions::default().compute
                },
                ..default()
            },
        })
        // Sound is kira behind our own mixer; `bevy_audio` is off by feature (workspace
        // `Cargo.toml`). Kept though they look idle: gizmos (bowstring, fishing line), sprites (the
        // FrameXML quad pass), picking, TextPlugin (glue text), PostProcessPlugin (glow bloom) and
        // ScenePlugin (avian's collider backend reads `SceneSpawner`); the ForwardDecal family is
        // registered inside `PbrPlugin::build`, so it cannot be disabled on its own.
        //
        // M2/WMO/ADT load through our own `mpq://` loaders; there is no glTF.
        .disable::<bevy::gltf::GltfPlugin>()
        // No bevy AA: no Fxaa/TAA/SMAA/CAS component anywhere (MSAA is core render, unaffected).
        .disable::<bevy::anti_alias::AntiAliasPlugin>()
        // No gamepad input; 1.12's bindings are keyboard/mouse.
        .disable::<bevy::gilrs::GilrsPlugin>()
}
