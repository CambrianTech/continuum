//! Live LiveKit — publish a persona's Bevy-rendered avatar into a real LiveKit
//! room over the out-of-process `livekit-bridge`.
//!
//! `avatar_breathe`/`avatar_emote` prove the renderer produces animated frames
//! into a crossbeam channel. This proves the LAST mile: those frames flow through
//! `LiveKitAgentManager::publish_video_frame` → the bridge's lazily-created
//! `LocalVideoTrack` → a real `livekit-server`, where any thin client can
//! subscribe and SEE the avatar. It exercises the exact production path
//! (`spawn_avatar_video_pump`) — no example-only pump.
//!
//! ## Prereqs (three processes)
//!
//! 1. A LiveKit server in dev mode (accepts devkey/secret):
//!      livekit-server --dev
//!    (installed at /opt/homebrew/bin/livekit-server; `--dev` listens on :7880)
//!
//! 2. The bridge, pointed at the same socket dir + server URL:
//!      export CONTINUUM_SOCKET_DIR="$HOME/.continuum/sockets"
//!      export LIVEKIT_URL="ws://localhost:7880"
//!      export LIVEKIT_API_KEY=devkey LIVEKIT_API_SECRET=secret
//!      cargo run -p livekit-bridge --release
//!
//! 3. This example (same CONTINUUM_SOCKET_DIR + LIVEKIT_URL):
//!      export CARGO_TARGET_DIR="$HOME/.continuum/cache/cargo-target"
//!      cargo run --example avatar_livekit --features metal,accelerate -- [identity] [room] [secs]
//!
//! Then join the room with any LiveKit client to watch. Quickest is the `lk` CLI:
//!      lk token create --api-key devkey --api-secret secret \
//!          --join --room <room> --identity viewer --valid-for 24h
//! …paste that token + ws://localhost:7880 into https://meet.livekit.io (custom).
//!
//! Defaults: identity=asha, room=avatar-demo, secs=120. The room name IS the
//! bridge `call_id` (the bridge connects agents to a room named after the call).

use std::sync::Arc;
use std::time::Duration;

use continuum_core::live::avatar::spawn_avatar_video_pump;
use continuum_core::live::transport::bridge_client::LiveKitAgentManager;
use continuum_core::live::video::bevy_renderer::get_or_init;

#[tokio::main]
async fn main() -> Result<(), String> {
    let identity = std::env::args().nth(1).unwrap_or_else(|| "asha".to_string());
    let room = std::env::args()
        .nth(2)
        .unwrap_or_else(|| "avatar-demo".to_string());
    let secs: u64 = std::env::args()
        .nth(3)
        .map(|s| s.parse().map_err(|_| format!("bad secs arg: {s}")))
        .transpose()?
        .unwrap_or(120);

    // Bevy resolves VRM assets relative to CWD (tools/) — mirror the other
    // avatar examples so model paths resolve identically.
    let repo_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .ok_or("cannot locate repo root from CARGO_MANIFEST_DIR")?;
    let tools_dir = repo_root.join("tools");
    std::env::set_current_dir(&tools_dir)
        .map_err(|e| format!("set CWD to {}: {e}", tools_dir.display()))?;

    let manager = Arc::new(LiveKitAgentManager::new());
    println!("🎬 avatar_livekit: identity='{identity}' room='{room}' secs={secs}");
    println!("   bridge/livekit target: {}", manager.url());

    // Boot Bevy up front so the first slot allocation isn't also paying cold
    // engine startup. (get_or_init spins the dedicated render thread.)
    println!("   booting Bevy renderer…");
    let _ = get_or_init();

    // 1) Join the persona to the room via the bridge (publishes its audio track;
    //    the video track is created lazily on the first pumped frame).
    println!("   joining '{identity}' to room '{room}' via bridge…");
    let _agent = manager
        .get_or_create_agent(&room, &identity, Some(&identity))
        .await
        .map_err(|e| format!("get_or_create_agent failed (is livekit-bridge running + connected to a livekit-server?): {e}"))?;

    // 2) Start the real production video pump — allocates a Bevy slot for this
    //    identity and streams its frames into the LiveKit video track.
    // Standalone native call plane for the tee. In the example no native WS client
    // joins it, so `push_avatar_frame` is a harmless no-op — the example only
    // demonstrates the LiveKit publish path.
    let call_manager =
        std::sync::Arc::new(continuum_core::live::transport::call_server::CallManager::new());
    let pump = spawn_avatar_video_pump(
        manager.clone(),
        call_manager,
        room.clone(),
        identity.clone(),
        identity.clone(),
    )
    .await
    .map_err(|e| format!("failed to start video pump: {e}"))?;

    println!("\n✅ Publishing '{identity}' avatar video into room '{room}'.");
    println!("   Watch it — generate a viewer token and open a LiveKit client:");
    println!(
        "     lk token create --api-key devkey --api-secret secret \\\n         --join --room {room} --identity viewer --valid-for 24h"
    );
    println!("   then paste token + {} into https://meet.livekit.io", manager.url());
    println!("\n   streaming for {secs}s (Ctrl-C to stop early)…");

    // 3) Hold the room open so a client can connect and view. If the pump task
    //    dies early (bridge/track failure), surface it loud rather than sitting
    //    on a dead stream.
    let deadline = tokio::time::sleep(Duration::from_secs(secs));
    tokio::pin!(deadline);
    tokio::select! {
        _ = &mut deadline => {
            println!("\n⏲  {secs}s elapsed — tearing down.");
        }
        joined = pump => {
            return Err(format!(
                "video pump exited before the {secs}s window — the bridge likely \
                 dropped the track or the server went away (join result: {joined:?})"
            ));
        }
    }

    // 4) Clean teardown: removing the agent detaches the bridge track; the pump's
    //    next publish then fails loud and the pump drops its slot guard (RAII).
    manager.remove_agent(&room, &identity).await;
    println!("👋 removed '{identity}' from room '{room}'. Done.");
    Ok(())
}
