//! Better StressBots for Pumpkin (NPC approximation).
//!
//! The original Paper plugin inserts real `ServerPlayer` objects into the
//! server and fakes their network connection. Pumpkin's plugin API does not
//! expose a player constructor, so this port spawns player-shaped mannequin
//! NPCs and moves them like the original bots.
//!
//! See README.md for the exact differences.

mod bots;
mod commands;

use pumpkin_plugin_api::{
    events::{EventData, EventHandler, EventPriority, PlayerJoinEvent},
    permission::{Permission, PermissionDefault, PermissionLevel},
    scheduler::SchedulerExt,
    Context, Plugin, PluginMetadata, Server,
};
use tracing::info;

const PLUGIN_NAME: &str = "better-stressbots";
/// Visible build marker so it is obvious which WASM is actually loaded.
const BUILD_STAMP: &str = "tablist-hostfix-2026-10-28-r2";
// Pumpkin requires every plugin permission to use the plugin's namespace.
// The Paper plugin's `stresstestbots.use` node therefore becomes
// `better-stressbots:use` here. See README.md.
const PERMISSION: &str = "better-stressbots:use";

struct JoinCleanup;

impl EventHandler<PlayerJoinEvent> for JoinCleanup {
    fn handle(
        &self,
        server: Server,
        event: EventData<PlayerJoinEvent>,
    ) -> EventData<PlayerJoinEvent> {
        // A player's chunks are loaded before this event fires. Sweep again so
        // bots left behind by a hard shutdown disappear as soon as someone
        // can actually see the area, even if it is outside the spawn chunks.
        bots::cleanup_stale(&server);
        // Make sure the joining player sees any bots that spawned earlier in
        // their tab list.
        bots::sync_tab_list_all(&server);
        event
    }
}

pub struct StressBotsPlugin;

impl Plugin for StressBotsPlugin {
    fn new() -> Self {
        Self
    }

    fn metadata(&self) -> PluginMetadata {
        PluginMetadata {
            name: PLUGIN_NAME.into(),
            version: env!("CARGO_PKG_VERSION").into(),
            authors: vec!["ported for Pumpkin".into()],
            description:
                "Server-side stress bots. NPC approximation of Better StressBots for Pumpkin."
                    .into(),
            dependencies: vec![],
            permissions: vec![],
        }
    }

    fn on_load(&self, context: Context) -> pumpkin_plugin_api::Result<()> {
        context.register_permission(&Permission {
            node: PERMISSION.to_string(),
            description: "Allows use of the stress-bot commands".to_string(),
            default: PermissionDefault::Op(PermissionLevel::Two),
            children: vec![],
        })?;

        context.register_command(commands::start_command(), PERMISSION);
        context.register_command(commands::goto_command(), PERMISSION);
        context.register_command(commands::stress_command(), PERMISSION);

        // Another sweep whenever a player joins: their chunks have just been
        // loaded, so this catches bots outside the startup spawn-chunk scan.
        context.register_event_handler(JoinCleanup, EventPriority::Normal, true)?;

        // Remove bots that were saved to the world by an earlier session that
        // did not shut down cleanly. Delay this until the spawn chunks (and
        // therefore their entities) have finished loading.
        context.schedule_delayed_task(100, |server| {
            bots::cleanup_stale(&server);
        });

        // Keep sweeping loaded chunks so stale mannequins are removed as soon
        // as terrain is loaded, without needing another join or /start.
        context.schedule_repeating_task(200, 200, |server| {
            bots::cleanup_stale(&server);
        });

        // The original plugin drives bots from a one-tick repeating task.
        context.schedule_repeating_task(1, 1, |server| {
            bots::tick(&server);
        });

        info!("{PLUGIN_NAME} loaded (NPC approximation mode, build {BUILD_STAMP})");
        Ok(())
    }

    fn on_unload(&self, context: Context) -> pumpkin_plugin_api::Result<()> {
        let server = context.get_server();
        bots::shutdown(&server);

        // Persist the removals right away. The host also saves on shutdown, but
        // forcing a save here means a clean stop cannot restore bots from an
        // earlier autosave when their chunks are still loaded.
        for world in server.get_all_worlds() {
            if let Err(err) = world.save() {
                tracing::warn!("Failed to save world after removing stress bots: {err}");
            }
        }

        info!("{PLUGIN_NAME} unloaded");
        Ok(())
    }
}

pumpkin_plugin_api::register_plugin!(StressBotsPlugin);
