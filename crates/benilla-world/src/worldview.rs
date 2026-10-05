//! `benilla-worldview`: the engine with no game attached, and the wall that keeps it that way. It
//! boots the engine plugins with a free-fly camera over Elwynn and no server, login, UI or player,
//! so a game concept wired back into the engine breaks it. A crate boundary alone cannot hold that
//! line: two Bevy systems couple through a runtime resource with no symbol crossing between them.

use bevy::camera::{PerspectiveProjection, Projection};
use bevy::core_pipeline::tonemapping::Tonemapping;
use bevy::input::mouse::AccumulatedMouseMotion;
use bevy::prelude::*;
use bevy::render::view::Hdr;
use bevy::window::{CursorGrabMode, PrimaryWindow};

use benilla_assets::coords::wow_to_bevy;

use crate::boot;
use crate::build_id::BuildId;
use crate::terrain_stream::SPAWN_XY;
use crate::thread_qos;

/// Where the viewer opens, in WoW world coords: Northshire, the client's own boot anchor
/// ([`crate::terrain_stream::SPAWN_XY`]), so both binaries stream the same tiles.
const VIEW_START: (f32, f32) = SPAWN_XY;

/// Height above the spawn point the camera opens at, in yards.
const VIEW_START_HEIGHT: f32 = 60.0;

/// `WOW_WORLDVIEW_AT=<x>,<y>[,<z>]` opens the viewer elsewhere (raw WoW coords, `z` defaulting to
/// [`VIEW_OPEN_Z`]); absent or unparseable is `None`, which opens at [`opening_point`]'s choice.
fn view_at_env() -> Option<[f32; 3]> {
    let v = std::env::var("WOW_WORLDVIEW_AT").ok()?;
    let c: Vec<f32> = v.split(',').filter_map(|p| p.trim().parse().ok()).collect();
    match c.len() {
        2 => Some([c[0], c[1], VIEW_OPEN_Z]),
        3 => Some([c[0], c[1], c[2]]),
        _ => {
            warn!("worldview: WOW_WORLDVIEW_AT wants `x,y[,z]` — ignoring it");
            None
        }
    }
}

/// Where the viewer opens and whether that point came from the map's data: `WOW_WORLDVIEW_AT`,
/// else [`VIEW_START`] on the default map (which must remain the default), else the centre of the
/// map's first existing tile, since Northshire's coordinates mean nothing on another map.
fn opening_point(map: u32, tile_centre: impl FnOnce() -> Option<[f32; 2]>) -> ([f32; 3], bool) {
    if let Some(at) = view_at_env() {
        return (at, false);
    }
    let default = [VIEW_START.0, VIEW_START.1, VIEW_OPEN_Z];
    if map == crate::world_map::DEFAULT_MAP_ID {
        return (default, false);
    }
    match tile_centre() {
        Some([x, y]) => ([x, y, VIEW_OPEN_Z], true),
        None => {
            warn!("worldview: map {map} has no terrain tile to open on — opening at the default anchor");
            (default, false)
        }
    }
}

/// The centre of the first existing ADT tile of `wdt`, scanning tile rows (world `x`) then columns.
fn first_tile_centre(wdt: &benilla_formats::WdtFile) -> Option<[f32; 2]> {
    (0..64u32)
        .flat_map(|ty| (0..64u32).map(move |tx| (tx, ty)))
        .find(|&(tx, ty)| {
            wdt.get_tile(tx as usize, ty as usize)
                .is_some_and(|t| t.has_adt)
        })
        .map(|(tx, ty)| {
            let (x, y) = benilla_formats::tile_to_world(tx, ty);
            let half = benilla_formats::TILE_SIZE / 2.0;
            [x - half, y - half]
        })
}

/// The map's WDT, read off the chain by the directory `Map.dbc` names for `map`.
fn read_map_wdt(
    assets: &benilla_assets::WorldAssets,
    catalog: &benilla_assets::MapCatalogRes,
    map: u32,
) -> Option<benilla_formats::WdtFile> {
    use benilla_assets::LockRecover;
    wdt_of(&mut assets.chain.lock_recover(), &catalog.0, map)
}

/// [`read_map_wdt`] on a bare chain: the WDT of the directory `Map.dbc` names for `map`.
fn wdt_of(
    chain: &mut benilla_formats::Chain,
    catalog: &benilla_formats::MapCatalog,
    map: u32,
) -> Option<benilla_formats::WdtFile> {
    let dir = catalog.directory(map)?;
    let bytes = chain
        .read_file(&format!("World\\Maps\\{dir}\\{dir}.wdt"))
        .ok()?;
    let version = benilla_formats::WowVersion::Classic;
    benilla_formats::WdtReader::new(std::io::Cursor::new(bytes), version)
        .read()
        .ok()
}

/// The look-at height when `WOW_WORLDVIEW_AT` names no `z`, clear of the Northshire valley floor.
const VIEW_OPEN_Z: f32 = 100.0;

/// Near plane in yards: the `nearclip` CVar's default, since the viewer has no CVar table.
const NEAR: f32 = crate::view::NEARCLIP_DEFAULT;

/// Vertical FOV in radians (70°), wider than the client camera's [`crate::view::CAM_FOVY`].
const FOVY: f32 = 1.221_730_5;

/// Builds and runs the world viewer; `build` is the launcher shim's compile-time git stamp.
pub fn run(build: BuildId) -> AppExit {
    // The install resolves from the launcher's folder when its stamp names one, as the client's
    // does; this shim has no `dev` feature, so it stamps none and the viewer keeps benilla's.
    benilla_formats::set_project_folder(build.project_dir);
    let mut app = App::new();
    app.insert_resource(build);

    // `WOW_WORLDVIEW_SURVEY=1` turns Bevy's panic on unvalidated system parameters into a
    // warning, so one run names every missing resource.
    if std::env::var("WOW_WORLDVIEW_SURVEY").as_deref() == Ok("1") {
        warn!("worldview: SURVEY mode — unmet dependencies are warnings, not panics");
        app.set_error_handler(bevy::ecs::error::warn);
    }

    // `WOW_WORLDVIEW_CHECK[=seconds]`, the survey as a gate: it records every distinct fault, runs
    // for a bounded time, prints the set and exits non-zero if any; `scripts/gates.sh` runs it.
    let check = check_seconds();
    if let Some(secs) = check {
        app.set_error_handler(record_fault);
        app.insert_resource(CheckDeadline(secs))
            .add_systems(Update, end_check);
    }

    // The `mpq://` source must be registered before `AssetPlugin` (in `DefaultPlugins`) builds.
    match benilla_formats::wow_data() {
        Some(data_dir) => {
            // The install says which build it is; one no build table knows is refused, not guessed
            // at. Printed, not logged: the log plugin is not built yet.
            match benilla_formats::Chain::open(&data_dir)
                .and_then(|chain| benilla_formats::detect_build(&chain))
            {
                Ok(b) => {
                    let [major, minor, patch] = b.version;
                    eprintln!(
                        "benilla-worldview: install is {major}.{minor}.{patch} (build {})",
                        b.build
                    );
                }
                Err(e) => {
                    eprintln!(
                        "benilla-worldview: cannot tell the install's build ({e:#}) — refusing it"
                    );
                    return AppExit::error();
                }
            }
            if let Err(e) = benilla_assets::register_mpq_source(&mut app, &data_dir) {
                eprintln!("benilla-assets: mpq:// source unavailable ({e:#})");
            }
        }
        None => eprintln!(
            "benilla-worldview: no WoW install found — looked in {:?}",
            benilla_formats::candidates()
        ),
    }

    let background = crate::bgwin::background_run();
    app.add_plugins(boot::tuned_default_plugins(Window {
        title: "benilla worldview".into(),
        resolution: std::env::var("WOW_WIN")
            .ok()
            .and_then(|v| {
                let (w, h) = v.split_once('x')?;
                Some(UVec2::new(w.parse().ok()?, h.parse().ok()?))
            })
            // Small for a run that reads no pixels, as in the client.
            .unwrap_or(if crate::bgwin::no_pixel_run() {
                UVec2::new(640, 360)
            } else {
                UVec2::new(1600, 900)
            })
            .into(),
        present_mode: if std::env::var("WOW_NOVSYNC").as_deref() == Ok("1") {
            bevy::window::PresentMode::AutoNoVsync
        } else {
            bevy::window::PresentMode::default()
        },
        // As in the client: an instrumented run opens unfocused, below normal windows.
        focused: !background,
        window_level: if background {
            bevy::window::WindowLevel::AlwaysOnBottom
        } else {
            bevy::window::WindowLevel::Normal
        },
        ..default()
    }))
    .add_plugins(thread_qos::ThreadQosPlugin)
    .add_plugins(crate::bgwin::BgWinPlugin)
    // macOS `Cmd+Q` is wired to `terminate:`, which leaves the event loop with no `AppExit`, and
    // `report_check` needs one for the exit code.
    .add_plugins(crate::mac_quit::MacQuitPlugin);

    // The cut line: the engine's whole plugin group, the same one the client adds.
    app.add_plugins(crate::world_plugins::WorldPlugins);
    stubs(&mut app);

    app.add_plugins(plugin);

    // After `AssetPlugin`: the loaders go into the live `AssetServer`, as in the client.
    benilla_assets::register_asset_loaders(&mut app);

    let exit = app.run();
    match check {
        Some(_) => report_check(exit),
        None => exit,
    }
}

/// `WOW_WORLDVIEW_CHECK` in seconds: unset is off, bare or unparseable is [`CHECK_SECS_DEFAULT`].
fn check_seconds() -> Option<f32> {
    let v = std::env::var("WOW_WORLDVIEW_CHECK").ok()?;
    Some(v.trim().parse().unwrap_or(CHECK_SECS_DEFAULT))
}

/// Long enough on a warm cache for every engine system to run and validate its parameters.
const CHECK_SECS_DEFAULT: f32 = 10.0;

/// Wall-clock seconds the check runs for.
#[derive(Resource)]
struct CheckDeadline(f32);

/// Every distinct fault the check saw, by the ECS construct that raised it. A `static` because
/// Bevy's error handler is a bare `fn` pointer and cannot capture.
static FAULTS: std::sync::Mutex<std::collections::BTreeSet<String>> =
    std::sync::Mutex::new(std::collections::BTreeSet::new());

/// The check's error handler: records and warns, so one run names every fault.
fn record_fault(error: bevy::ecs::error::BevyError, ctx: bevy::ecs::error::ErrorContext) {
    let entry = format!("{} `{}`: {error}", ctx.kind(), ctx.name());
    warn!("worldview: {entry}");
    FAULTS
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .insert(entry);
}

fn end_check(
    time: Res<Time<bevy::time::Real>>,
    deadline: Res<CheckDeadline>,
    mut exit: MessageWriter<AppExit>,
) {
    if time.elapsed_secs() >= deadline.0 {
        exit.write(AppExit::Success);
    }
}

/// Prints the check's verdict and makes it the process exit code.
fn report_check(exit: AppExit) -> AppExit {
    let faults = FAULTS
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if faults.is_empty() {
        println!("WORLDVIEW_CHECK ok — the engine booted and ran with no game attached");
        return exit;
    }
    for f in faults.iter() {
        println!("WORLDVIEW_CHECK fault {f}");
    }
    println!(
        "WORLDVIEW_CHECK {} fault(s) — a game concept is wired into the engine, or an engine \
         fact is parked on the game side.",
        faults.len()
    );
    // A run with no install (`WOW_DATA=`) faults for a different reason, so it says which.
    if benilla_formats::wow_data().is_none() {
        println!(
            "WORLDVIEW_CHECK ran with NO INSTALL: a fault here is a system taking a resource that \
             only exists when there is client data as a hard `Res`/`ResMut`. Take it as `Option` \
             and return, or insert it ahead of the no-data bail."
        );
    }
    AppExit::error()
}
/// What the engine still needs told: only whether there is a world. Coupling that never panics is
/// invisible to this binary: an ordering edge onto an unregistered system is dropped, an
/// `Option<Res<…>>` sees `None`, and a query on a component nobody spawns matches nothing.
fn stubs(app: &mut App) {
    // The viewer's world is always live; whatever composes the engine writes `WorldLive`.
    app.insert_resource(crate::schedule::WorldLive(true));
}

/// The viewer itself: the free-fly camera and its controller.
fn plugin(app: &mut App) {
    app.add_systems(
        Startup,
        spawn_view_camera
            .after(benilla_assets::AssetSet::Open)
            .after(crate::world_map::WorldMapLoad),
    )
    .add_systems(Update, fly.in_set(crate::schedule::WorldStage::Input))
    .add_systems(
        Update,
        settle_start_height.run_if(resource_exists::<SettleStart>),
    );

    // `WOW_WORLDVIEW_SHOT=<png>` (at `WOW_WORLDVIEW_SHOT_AT` seconds, default 20) writes one frame
    // and exits, with no subject gate: the client's live shot needs a player the engine lacks.
    if std::env::var("WOW_WORLDVIEW_SHOT").is_ok() {
        app.add_systems(Update, shoot_and_exit);
    }

    // The counters print on the frame the shot fires, and the failure tally runs from the start.
    app.init_resource::<LoadFailures>().add_systems(
        Update,
        (
            note_failures::<benilla_assets::AdtTile>("adt"),
            note_failures::<benilla_assets::WdtIndex>("wdt"),
            note_failures::<benilla_assets::M2Model>("m2"),
            note_failures::<benilla_assets::WmoModel>("wmo"),
            note_failures::<Image>("image"),
        ),
    );
    if std::env::var("WOW_WORLDVIEW_SHOT").is_ok() {
        app.add_systems(
            Update,
            print_counters.in_set(crate::lighting::LightingConsumeSet),
        );
    }
}

/// Every asset the engine's loaders refused, by kind and path: the count and the first error.
#[derive(Resource, Default)]
struct LoadFailures(std::collections::BTreeMap<(&'static str, String), (u32, String)>);

fn note_failures<A: bevy::asset::Asset>(
    kind: &'static str,
) -> impl FnMut(MessageReader<bevy::asset::AssetLoadFailedEvent<A>>, ResMut<LoadFailures>) {
    move |mut events, mut fails| {
        for e in events.read() {
            let slot = fails
                .0
                .entry((kind, e.path.to_string()))
                .or_insert_with(|| (0, e.error.to_string().chars().take(160).collect()));
            slot.0 += 1;
        }
    }
}

/// The seconds `WOW_WORLDVIEW_SHOT` fires at.
fn shot_at() -> f32 {
    std::env::var("WOW_WORLDVIEW_SHOT_AT")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(20.0)
}

/// What the counters read, as one parameter.
#[derive(bevy::ecs::system::SystemParam)]
struct CounterParams<'w, 's> {
    census: crate::world_census::WorldCensus<'w, 's>,
    placements: Option<Res<'w, crate::terrain_stream::Placements>>,
    adt: Res<'w, Assets<benilla_assets::AdtTile>>,
    wmo: Res<'w, Assets<benilla_assets::WmoModel>>,
    m2: Res<'w, Assets<benilla_assets::M2Model>>,
    liquids: Query<'w, 's, (), With<crate::liquid::LiquidSurface>>,
    light: Option<Res<'w, crate::lighting::WowLighting>>,
    map: Option<Res<'w, crate::world_map::CurrentMap>>,
    cam: Query<'w, 's, &'static GlobalTransform, With<crate::view::WorldCamera>>,
    fails: Res<'w, LoadFailures>,
    streamer: Option<Res<'w, crate::terrain_stream::TerrainStreamer>>,
}

/// Prints the run's counters once, on the frame the shot fires: what was asked for, what arrived
/// and what failed, so a run is judged by numbers and not by the picture.
fn print_counters(time: Res<Time>, mut done: Local<bool>, p: CounterParams) {
    if *done || time.elapsed_secs() < shot_at() {
        return;
    }
    *done = true;
    let r = p.census.take();
    let (furnished, requested) = r.tiles.unwrap_or((0, 0));
    let (mut tiles_loaded, mut chunks, mut layer_slots) = (0usize, 0usize, 0usize);
    let mut distinct_layers = std::collections::BTreeSet::new();
    for (_, tile) in p.adt.iter() {
        tiles_loaded += 1;
        for c in tile.chunks.iter().filter(|c| c.indices.len() >= 3) {
            chunks += 1;
            layer_slots += c.layer_textures.len().min(4);
            distinct_layers.extend(
                c.layer_textures
                    .iter()
                    .take(4)
                    .map(|t| t.to_ascii_lowercase()),
            );
        }
    }
    let (m2_inst, wmo_inst, unspawned, no_entity, props_pending) = p
        .placements
        .as_ref()
        .map_or((0, 0, 0, 0, 0), |p| p.counts());
    let m2_empty = p.m2.iter().filter(|(_, m)| m.submeshes.is_empty()).count();
    let m2_submeshes: usize = p.m2.iter().map(|(_, m)| m.submeshes.len()).sum();
    let wmo_groups: usize = p.wmo.iter().map(|(_, m)| m.group_nav.len()).sum();
    let map = p.map.as_ref().map_or(u32::MAX, |m| m.0);
    let failed_tiles = p.fails.0.keys().filter(|(k, _)| *k == "adt").count();
    println!("WV_COUNTERS map={map}");
    println!(
        "WV_COUNTERS tiles requested={requested} furnished={furnished} adt_loaded={tiles_loaded} \
         adt_failed={failed_tiles}"
    );
    println!(
        "WV_COUNTERS terrain chunks={chunks} layer_slots={layer_slots} distinct_layer_textures={}",
        distinct_layers.len()
    );
    println!(
        "WV_COUNTERS placements unspawned={unspawned} spawned_with_no_entity={no_entity} \
         wmo_props_pending={props_pending} | m2 models_with_no_submesh={m2_empty} \
         model_submeshes={m2_submeshes}"
    );
    println!(
        "WV_COUNTERS m2 models={} instances={m2_inst} | wmo roots={} groups={wmo_groups} \
         instances={wmo_inst} | liquid_surfaces={} | submeshes={} drawn={} images={} meshes={}",
        p.m2.len(),
        p.wmo.len(),
        p.liquids.iter().count(),
        r.submeshes,
        r.drawn,
        r.images,
        r.meshes
    );
    if let Some(l) = p.light.as_ref() {
        let d = benilla_formats::Atmosphere::DEFAULT;
        println!(
            "WV_COUNTERS light fog_end={:.1} fog_color={:?} ambient={:?} fallback_atmosphere={}",
            l.fog_end,
            l.fog_color,
            l.ambient,
            l.fog_color == d.fog_color && l.ambient == d.ambient
        );
    }
    if let Some(c) = p.cam.iter().next() {
        let wow = benilla_assets::coords::bevy_to_wow(c.translation());
        let ground = p
            .streamer
            .as_ref()
            .and_then(|s| crate::terrain_stream::terrain_height_under(s, &p.adt, c.translation()));
        println!(
            "WV_COUNTERS camera wow=[{:.1}, {:.1}, {:.1}] ground_under={ground:?} sky={:?}",
            wow[0], wow[1], wow[2], r.sky
        );
    }
    let mut by_class: std::collections::BTreeMap<String, (usize, u32)> = Default::default();
    for ((k, _), (n, _)) in &p.fails.0 {
        let e = by_class.entry((*k).to_string()).or_default();
        e.0 += 1;
        e.1 += n;
    }
    let misses = benilla_assets::load_misses::texture_misses();
    for (k, why, _, n) in &misses {
        let e = by_class.entry(format!("{k} {why}")).or_default();
        e.0 += 1;
        e.1 += n;
    }
    println!("WV_COUNTERS failed (distinct paths, attempts) by class: {by_class:?}");
    for ((k, path), (n, err)) in p.fails.0.iter().take(15) {
        println!("WV_COUNTERS fail {k} x{n} {path}: {err}");
    }
    for (k, why, path, n) in misses.iter().take(15) {
        println!("WV_COUNTERS fail {k} {why} x{n} {path}");
    }
}

/// Fires the `WOW_WORLDVIEW_SHOT` screenshot once and exits two seconds later: the write is
/// asynchronous, and an immediate exit loses the PNG.
fn shoot_and_exit(
    time: Res<Time>,
    mut commands: Commands,
    mut fired_at: Local<Option<f32>>,
    mut exit: MessageWriter<AppExit>,
) {
    let at = shot_at();
    match *fired_at {
        None if time.elapsed_secs() >= at => {
            let path = std::env::var("WOW_WORLDVIEW_SHOT").unwrap_or_default();
            info!("worldview: writing {path}");
            commands
                .spawn(bevy::render::view::screenshot::Screenshot::primary_window())
                .observe(bevy::render::view::screenshot::save_to_disk(path));
            *fired_at = Some(time.elapsed_secs());
        }
        Some(t) if time.elapsed_secs() >= t + 2.0 => {
            exit.write(AppExit::Success);
        }
        _ => {}
    }
}

/// The viewer's minimal free-fly camera, not a twin of the client's `FlyCam`.
#[derive(Component)]
struct ViewCam {
    yaw: f32,
    pitch: f32,
    speed: f32,
}

/// The opening point came from the map's data, not from the user: the eye's height is a guess until
/// the terrain under it loads, and [`settle_start_height`] then puts it a fixed height over ground.
#[derive(Resource)]
struct SettleStart;

/// Puts the eye [`VIEW_START_HEIGHT`] over the terrain under it once that tile has loaded.
fn settle_start_height(
    mut commands: Commands,
    streamer: Option<Res<crate::terrain_stream::TerrainStreamer>>,
    tiles: Res<Assets<benilla_assets::AdtTile>>,
    mut cam: Query<&mut Transform, With<ViewCam>>,
) {
    let (Some(streamer), Ok(mut xf)) = (streamer, cam.single_mut()) else {
        return;
    };
    let Some(ground) =
        crate::terrain_stream::terrain_height_under(&streamer, &tiles, xf.translation)
    else {
        return;
    };
    info!(
        "worldview: opening point settled {VIEW_START_HEIGHT} yd over the ground at z={ground:.1}"
    );
    xf.translation.y = ground + VIEW_START_HEIGHT;
    commands.remove_resource::<SettleStart>();
}

#[allow(clippy::too_many_arguments)]
fn spawn_view_camera(
    mut commands: Commands,
    msaa: Res<crate::view::MsaaSetting>,
    map: Option<Res<crate::world_map::CurrentMap>>,
    catalog: Option<Res<benilla_assets::MapCatalogRes>>,
    assets: Option<Res<benilla_assets::WorldAssets>>,
) {
    let map = map.map_or(crate::world_map::DEFAULT_MAP_ID, |m| m.0);
    let (point, derived) = opening_point(map, || {
        let wdt = read_map_wdt(assets.as_deref()?, catalog.as_deref()?, map)?;
        first_tile_centre(&wdt)
    });
    if derived {
        info!(
            "worldview: map {map} opens at its first tile's centre [{:.1}, {:.1}]",
            point[0], point[1]
        );
        commands.insert_resource(SettleStart);
    }
    let start = wow_to_bevy(point);
    let far = crate::view::CAM_FAR;
    commands.spawn((
        Camera3d::default(),
        crate::view::WorldCamera,
        msaa.level(),
        Projection::from(PerspectiveProjection {
            far,
            near: NEAR,
            fov: FOVY,
            ..default()
        }),
        Hdr,
        Tonemapping::None,
        crate::ffx_glow::FfxGlow::WORLD,
        Transform::from_translation(start + Vec3::new(0.0, VIEW_START_HEIGHT, VIEW_START_HEIGHT))
            .looking_at(start, Vec3::Y),
        ViewCam {
            yaw: 0.0,
            pitch: -0.5,
            speed: 100.0,
        },
    ));
}

/// WASD, Space and C fly, a right or left drag looks, Ctrl boosts, the wheel sets speed.
fn fly(
    time: Res<Time>,
    keys: Res<ButtonInput<KeyCode>>,
    buttons: Res<ButtonInput<MouseButton>>,
    motion: Res<AccumulatedMouseMotion>,
    mut wheel: MessageReader<bevy::input::mouse::MouseWheel>,
    mut window: Query<&mut bevy::window::CursorOptions, With<PrimaryWindow>>,
    mut cam: Query<(&mut Transform, &mut ViewCam)>,
) {
    let Ok((mut xf, mut cam)) = cam.single_mut() else {
        return;
    };

    let looking = buttons.pressed(MouseButton::Right) || buttons.pressed(MouseButton::Left);
    if let Ok(mut cursor) = window.single_mut() {
        let want = if looking {
            CursorGrabMode::Locked
        } else {
            CursorGrabMode::None
        };
        if cursor.grab_mode != want {
            cursor.grab_mode = want;
            cursor.visible = !looking;
        }
    }
    if looking {
        const LOOK: f32 = 0.003;
        cam.yaw -= motion.delta.x * LOOK;
        cam.pitch = (cam.pitch - motion.delta.y * LOOK).clamp(-1.54, 1.54);
    }
    xf.rotation = Quat::from_euler(EulerRot::YXZ, cam.yaw, cam.pitch, 0.0);

    for ev in wheel.read() {
        cam.speed = (cam.speed * (1.0 + ev.y * 0.1)).clamp(1.0, 2000.0);
    }

    let mut dir = Vec3::ZERO;
    if keys.pressed(KeyCode::KeyW) {
        dir += *xf.forward();
    }
    if keys.pressed(KeyCode::KeyS) {
        dir += *xf.back();
    }
    if keys.pressed(KeyCode::KeyA) {
        dir += *xf.left();
    }
    if keys.pressed(KeyCode::KeyD) {
        dir += *xf.right();
    }
    if keys.pressed(KeyCode::Space) {
        dir += Vec3::Y;
    }
    if keys.pressed(KeyCode::KeyC) {
        dir -= Vec3::Y;
    }
    if dir != Vec3::ZERO {
        let boost = if keys.pressed(KeyCode::ControlLeft) || keys.pressed(KeyCode::ControlRight) {
            5.0
        } else {
            1.0
        };
        xf.translation += dir.normalize() * cam.speed * boost * time.delta_secs();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use benilla_formats::{load_map_catalog, open_chain, MapCatalog};

    /// Map `id` resolved through `Map.dbc` to its directory and WDT, and the WDT's opening tile.
    fn resolve(
        chain: &mut benilla_formats::Chain,
        catalog: &MapCatalog,
        id: u32,
    ) -> (String, [f32; 2]) {
        let dir = catalog.directory(id).expect("Map.dbc row").to_string();
        let wdt = wdt_of(chain, catalog, id).expect("the map's WDT reads");
        let centre = first_tile_centre(&wdt).expect("an ADT map has a tile");
        let (tx, ty) = benilla_formats::world_to_tile(centre[0], centre[1]);
        assert!(
            wdt.get_tile(tx as usize, ty as usize)
                .is_some_and(|t| t.has_adt),
            "{dir}: the opening point {centre:?} lies in tile ({tx}, {ty}), which has no ADT"
        );
        (dir, centre)
    }

    #[test]
    fn the_default_map_resolves_to_azeroth_and_its_wdt_on_1_12_1() {
        let data = benilla_formats::wow_data_or_skip!();
        let mut chain = open_chain(&data).expect("open chain");
        let catalog = load_map_catalog(&mut chain).expect("Map.dbc");
        let (dir, _) = resolve(&mut chain, &catalog, crate::world_map::DEFAULT_MAP_ID);
        assert_eq!(dir, "Azeroth");
    }

    #[test]
    fn maps_0_and_530_resolve_to_their_directories_and_wdts_on_2_4_3() {
        let data = benilla_formats::wow_data_tbc_or_skip!();
        let mut chain = open_chain(&data).expect("open chain");
        let catalog = load_map_catalog(&mut chain).expect("Map.dbc");
        assert_eq!(resolve(&mut chain, &catalog, 0).0, "Azeroth");
        assert_eq!(resolve(&mut chain, &catalog, 530).0, "Expansion01");
    }

    #[test]
    fn the_default_map_opens_at_the_anchor_and_another_map_at_its_first_tile() {
        if view_at_env().is_some() {
            return; // an explicit `WOW_WORLDVIEW_AT` overrides the choice under test
        }
        let anchor = [VIEW_START.0, VIEW_START.1, VIEW_OPEN_Z];
        let (p, derived) = opening_point(crate::world_map::DEFAULT_MAP_ID, || {
            panic!("the default map never reads a tile")
        });
        assert_eq!((p, derived), (anchor, false));
        let (p, derived) = opening_point(530, || Some([228.0, 2633.0]));
        assert_eq!((p, derived), ([228.0, 2633.0, VIEW_OPEN_Z], true));
        // A map with no tile (a WMO-only one) falls back to the anchor and says so.
        assert_eq!(opening_point(489, || None), (anchor, false));
    }
}
