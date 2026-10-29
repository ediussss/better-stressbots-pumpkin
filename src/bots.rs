use std::collections::HashSet;
use std::sync::Mutex;

use pumpkin_plugin_api::{
    common::BlockPos, text::TextComponent, Entity, EntityType, Player, Server, World,
};
use pumpkin_plugin_api::wit::pumpkin::plugin::java_packets::{
    CPlayerInfoUpdate, ClientboundPacket, CRemovePlayerInfo, Player as JavaPlayerInfo, PlayerAction,
    PlayerActionAddPlayer,
};
use pumpkin_plugin_api::wit::pumpkin::plugin::uuid::Uuid;
use tracing::info;

pub const MAX_BOTS: usize = 5000;

type Vec3 = (f64, f64, f64);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    None,
    Walk,
    Fly,
}

impl Mode {
    pub fn from_str(value: &str) -> Option<Self> {
        match value.to_ascii_lowercase().as_str() {
            "none" => Some(Self::None),
            "walk" => Some(Self::Walk),
            "fly" => Some(Self::Fly),
            _ => None,
        }
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Walk => "walk",
            Self::Fly => "fly",
        }
    }
}

#[derive(Clone, Copy)]
struct Rng(u64);

impl Rng {
    fn next_f64(&mut self) -> f64 {
        // xorshift64*, good enough for wandering targets.
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        let value = x.wrapping_mul(0x2545_F491_4F6C_DD1D);
        (value >> 11) as f64 / (1_u64 << 53) as f64
    }
}

struct Bot {
    entity: Entity,
    uuid: Uuid,
    name: String,
    spawn: Vec3,
    target: Vec3,
    goto_target: Option<Vec3>,
    mode: Mode,
    stuck: u32,
}

impl Bot {
    fn pick_target(&mut self, radius: f64, mode: Mode, rng: &mut Rng) {
        self.mode = mode;
        let angle = rng.next_f64() * std::f64::consts::TAU;
        // Uniform over the disk instead of clustering near the spawn point.
        let distance = radius.max(0.0) * rng.next_f64().sqrt();
        let target_y = match mode {
            Mode::Walk | Mode::None => self.spawn.1,
            Mode::Fly => (self.spawn.1 + rng.next_f64() * 100.0 + 20.0).clamp(64.0, 250.0),
        };
        self.target = (
            self.spawn.0 + angle.cos() * distance,
            target_y,
            self.spawn.2 + angle.sin() * distance,
        );
    }

    fn tick(&mut self, speed: f64, radius: f64, rng: &mut Rng) {
        if self.mode == Mode::None && self.goto_target.is_none() {
            return;
        }

        let (x, mut y, z) = self.entity.get_position();
        // If a bot ever ends up inside terrain (chunk load, teleport, etc.),
        // push it up to the first clear position instead of clipping.
        {
            let world = self.entity.get_world();
            if body_blocked(&world, x, y, z) {
                let mut ny = y;
                for _ in 0..8 {
                    ny += 1.0;
                    if !body_blocked(&world, x, ny, z) {
                        self.entity.teleport((x, ny, z), world);
                        y = ny;
                        break;
                    }
                }
            }
        }
        let has_goto = self.goto_target.is_some();
        let (tx, ty, tz) = self.goto_target.unwrap_or(self.target);

        let dx = tx - x;
        let dz = tz - z;
        let horizontal = (dx * dx + dz * dz).sqrt();
        if horizontal < 2.0 {
            if has_goto {
                self.goto_target = None;
            } else {
                self.pick_target(radius, self.mode, rng);
            }
            return;
        }

        let nx = x + dx / horizontal * speed;
        let nz = z + dz / horizontal * speed;
        let walk_on_ground =
            self.mode == Mode::Walk || (has_goto && self.mode == Mode::None);

        let world = self.entity.get_world();
        let mut progressed = false;
        if walk_on_ground {
            // Try the direct step first, then slide along X or Z when a wall is
            // in the way. Never teleport into a column taller than a block or
            // into a block that would intersect the bot's body.
            let mut moved = false;
            for (next_x, next_z) in [(nx, nz), (nx, z), (x, nz)] {
                if let Some(next_y) = walk_step(&world, y, next_x, next_z) {
                    self.entity.teleport((next_x, next_y, next_z), world);
                    moved = true;
                    break;
                }
            }
            progressed = moved;
            if !moved {
                if has_goto {
                    // Stuck on the way to a /goto target: bail out so the bot
                    // does not grind against the same wall forever.
                    self.stuck = self.stuck.saturating_add(1);
                    if self.stuck >= 4 {
                        self.goto_target = None;
                        self.stuck = 0;
                        self.pick_target(radius, self.mode, rng);
                    }
                } else {
                    self.stuck = self.stuck.saturating_add(1);
                    if self.stuck >= 2 {
                        self.stuck = 0;
                        // Re-roll further away to escape the obstruction.
                        self.pick_target(radius.max(100.0), self.mode, rng);
                    } else {
                        self.pick_target(radius, self.mode, rng);
                    }
                }
            } else {
                self.stuck = 0;
            }
        } else {
            let dy = ty - y;
            let total = (dx * dx + dy * dy + dz * dz).sqrt();
            if total <= f64::EPSILON {
                return;
            }
            let scale = speed / total;
            let (next_x, next_y, next_z) = (x + dx * scale, y + dy * scale, z + dz * scale);

            if !body_blocked(&world, next_x, next_y, next_z) {
                self.entity.teleport((next_x, next_y, next_z), world);
                progressed = true;
            } else if !body_blocked(&world, x, next_y, z) {
                // Fly straight up or down out of the obstruction.
                self.entity.teleport((x, next_y, z), world);
                progressed = true;
            } else if !has_goto {
                self.pick_target(radius, self.mode, rng);
            }
        }

        // Update stuck counter for fly mode (walk mode handled inline).
        if !walk_on_ground {
            if progressed {
                self.stuck = 0;
            } else {
                self.stuck = self.stuck.saturating_add(1);
                if self.stuck >= 2 && !has_goto {
                    self.stuck = 0;
                    self.pick_target(radius.max(100.0), self.mode, rng);
                }
            }
        }
    }
}

struct Manager {
    bots: Vec<Bot>,
    speed: f64,
    radius: f64,
    target_count: usize,
    mode: Mode,
    spawning: bool,
    tick: u64,
    rng: Rng,
    /// World name and spawn point for the current batch of bots. `None` means
    /// "use the main world's spawn", matching the Paper plugin for console use.
    /// When a player runs `/start`, this is set to that player's world and
    /// position so the bots are immediately in view instead of spawning in
    /// another dimension or far away at the world spawn.
    spawn_origin: Option<(String, Vec3)>,
}

static MANAGER: Mutex<Manager> = Mutex::new(Manager {
    bots: Vec::new(),
    speed: 0.1,
    radius: 500.0,
    target_count: 0,
    mode: Mode::None,
    spawning: false,
    tick: 0,
    rng: Rng(0x9E37_79B9_7F4A_7C15),
    spawn_origin: None,
});

fn default_world(worlds: &[World]) -> Option<&World> {
    worlds
        .iter()
        .find(|world| {
            let name = world.get_name();
            name == "world" || name.contains("overworld")
        })
        .or_else(|| worlds.first())
}

const BOT_HALF_WIDTH: f64 = 0.3;
const BOT_HEIGHT: f64 = 1.8;
/// Maximum height a walking bot can step up in one tick. A full block is 1.0.
const MAX_STEP_UP: f64 = 1.05;

/// Returns the exact Y at which a player's feet rest on top of the highest
/// motion-blocking block in this column. Heightmaps only store a block Y, so
/// partial blocks such as snow layers need their collision shape's top.
/// Returns `None` when the chunk is not loaded.
fn surface_y(world: &World, x: f64, z: f64) -> Option<f64> {
    let block_x = x.floor() as i32;
    let block_z = z.floor() as i32;
    let height = world.get_motion_blocking_height(block_x, block_z);
    if height < world.get_min_y() {
        return None;
    }

    let state = world.get_block_state(BlockPos {
        x: block_x,
        y: height,
        z: block_z,
    });
    let shape_top = state
        .collision_shapes
        .iter()
        .map(|shape| shape.max.1)
        .fold(0.0_f64, f64::max);
    // Match the Paper plugin: never walk on a surface below Y 60.
    Some((f64::from(height) + if shape_top > 0.0 { shape_top } else { 1.0 }).max(60.0))
}

fn point_blocked(world: &World, x: f64, y: f64, z: f64) -> bool {
    let block_x = x.floor() as i32;
    let block_y = y.floor() as i32;
    let block_z = z.floor() as i32;
    let local_x = x - f64::from(block_x);
    let local_y = y - f64::from(block_y);
    let local_z = z - f64::from(block_z);
    let state = world.get_block_state(BlockPos {
        x: block_x,
        y: block_y,
        z: block_z,
    });

    state.collision_shapes.iter().any(|shape| {
        local_x >= shape.min.0
            && local_x <= shape.max.0
            && local_y >= shape.min.1
            && local_y <= shape.max.1
            && local_z >= shape.min.2
            && local_z <= shape.max.2
    })
}

/// Samples the player-sized box, not just the block under the feet. This is
/// what keeps bots from putting their shoulders or head inside a wall.
fn body_blocked(world: &World, x: f64, feet_y: f64, z: f64) -> bool {
    for offset_x in [-BOT_HALF_WIDTH, 0.0, BOT_HALF_WIDTH] {
        for offset_z in [-BOT_HALF_WIDTH, 0.0, BOT_HALF_WIDTH] {
            for offset_y in [0.02, BOT_HEIGHT * 0.5, BOT_HEIGHT - 0.02] {
                if point_blocked(
                    world,
                    x + offset_x,
                    feet_y + offset_y,
                    z + offset_z,
                ) {
                    return true;
                }
            }
        }
    }
    false
}

fn clear_spawn_y(world: &World, x: f64, mut y: f64, z: f64) -> f64 {
    // Safety net for roofs, leaves, and unusual collision shapes.
    for _ in 0..8 {
        if !body_blocked(world, x, y, z) {
            return y;
        }
        y += 1.0;
    }
    y
}

/// A candidate walking step. Rejects unloaded chunks, steps taller than a
/// block, and destinations where the bot's body would intersect terrain.
fn walk_step(world: &World, from_y: f64, x: f64, z: f64) -> Option<f64> {
    let ground = surface_y(world, x, z)?;
    if ground - from_y > MAX_STEP_UP {
        return None;
    }
    if body_blocked(world, x, ground, z) {
        return None;
    }
    Some(ground)
}

/// Paper broadcasts a vanilla join/leave line for fake players. These bots are
/// entities, so there is no tab-list entry, but a matching system message makes
/// it obvious that they spawned and were removed.
fn announce(server: &Server, name: &str, action: &str) {
    // Mirror the vanilla server log line that a real player join/leave produces,
    // in addition to the in-game system message.
    info!("{name} {action}");
    let line = format!("§e{name} {action}");
    for player in server.get_all_players() {
        player.send_system_message(TextComponent::from_legacy_string(&line), false);
    }
}

const TAB_LIST_ADD_FLAGS: u8 = 0x01 | 0x04 | 0x08 | 0x10 | 0x40 | 0x80;

fn tab_list_add_packet(bot: &Bot) -> ClientboundPacket {
    ClientboundPacket::CPlayerInfoUpdate(CPlayerInfoUpdate {
        actions: TAB_LIST_ADD_FLAGS,
        players: vec![JavaPlayerInfo {
            uuid: bot.uuid,
            actions: vec![
                PlayerAction::AddPlayer(PlayerActionAddPlayer {
                    name: bot.name.clone(),
                    properties: vec![],
                }),
                PlayerAction::UpdateGameMode(1),
                PlayerAction::UpdateListed(true),
                PlayerAction::UpdateLatency(0),
                PlayerAction::UpdateListOrder(0),
                PlayerAction::UpdateHat(true),
            ],
        }],
    })
}

fn tab_list_remove_packet(uuid: Uuid) -> ClientboundPacket {
    ClientboundPacket::CRemovePlayerInfo(CRemovePlayerInfo {
        players: vec![uuid],
    })
}

fn send_tab_list_packet(server: &Server, packet: ClientboundPacket) {
    let players = server.get_all_players();
    tracing::info!("Sending tab-list packet to {} players", players.len());
    for player in players {
        let name = player.get_name(); // may not exist; adjust if compile fails
        if let Some(java) = player.as_java() {
            tracing::info!("  -> sending to Java player {}", name);
            java.send_packet(&packet);
        } else {
            tracing::warn!("  -> player {} is not a Java client; skipping", name);
        }
    }
}

/// Add a single bot to every online Java client's tab list.
fn sync_tab_list_add(server: &Server, bot: &Bot) {
    tracing::info!("Adding {} ({}) to tab list", bot.name, bot.uuid);
    send_tab_list_packet(server, tab_list_add_packet(bot));
}

/// Remove a single bot (by UUID) from every online Java client's tab list.
fn sync_tab_list_remove(server: &Server, uuid: Uuid) {
    send_tab_list_packet(server, tab_list_remove_packet(uuid));
}

/// Re-send the full bot list to every online Java client. Used when a player
/// joins so they see bots that spawned before they connected.
pub fn sync_tab_list_all(server: &Server) {
    let bots: Vec<(Uuid, String)> = {
        let manager = MANAGER.lock().unwrap_or_else(|e| e.into_inner());
        manager.bots.iter().map(|b| (b.uuid, b.name.clone())).collect()
    };
    if bots.is_empty() {
        return;
    }
    // Send every bot in a single packet so a joining player receives the
    // whole list at once instead of one packet per bot.
    let players: Vec<JavaPlayerInfo> = bots
        .into_iter()
        .map(|(uuid, name)| JavaPlayerInfo {
            uuid,
            actions: vec![
                PlayerAction::AddPlayer(PlayerActionAddPlayer {
                    name,
                    properties: vec![],
                }),
                PlayerAction::UpdateGameMode(1),
                PlayerAction::UpdateListed(true),
                PlayerAction::UpdateLatency(0),
                PlayerAction::UpdateListOrder(0),
                PlayerAction::UpdateHat(true),
            ],
        })
        .collect();
    let packet = ClientboundPacket::CPlayerInfoUpdate(CPlayerInfoUpdate {
        actions: TAB_LIST_ADD_FLAGS,
        players,
    });
    send_tab_list_packet(server, packet);
}

/// Mannequins are saveable. If a previous session ended without a clean
/// shutdown, its bot entities can be reloaded from the world and linger forever.
/// Remove any entity that carries this plugin's `StressBot_*` name.
pub fn cleanup_stale(server: &Server) -> usize {
    let known: HashSet<u32> = {
        let manager = MANAGER.lock().unwrap_or_else(|e| e.into_inner());
        manager
            .bots
            .iter()
            .map(|bot| bot.entity.get_id())
            .collect()
    };

    let mut removed = 0;
    for world in server.get_all_worlds() {
        for entity in world.get_entities() {
            if entity.get_type() != EntityType::Mannequin {
                continue;
            }
            if known.contains(&entity.get_id()) {
                continue;
            }
            let Some(name) = entity.get_custom_name() else {
                continue;
            };
            if name.get_text().starts_with("StressBot_") {
                sync_tab_list_remove(server, entity.get_uuid());
                entity.remove();
                removed += 1;
            }
        }
    }
    if removed > 0 {
        info!("Removed {removed} stale stress-bot entities from saved worlds.");
    }
    removed
}

impl Manager {
    fn spawn_one(&mut self, server: &Server) {
        let worlds = server.get_all_worlds();
        let origin = self.spawn_origin.clone();

        let world = match &origin {
            Some((name, _)) => worlds
                .iter()
                .find(|world| world.get_name() == *name)
                .or_else(|| default_world(&worlds)),
            None => default_world(&worlds),
        };
        let Some(world) = world else {
            return;
        };

        let (position, yaw) = match &origin {
            Some((_, (x, y, z))) => {
                // Arrange the batch in a spiral around the player instead of
                // stacking every bot on the player's head. The first bots end
                // up 10 blocks away, and larger batches expand outward.
                let index = self.bots.len() as f64;
                let angle = index * 2.399_963_229_728_653
                    + (self.rng.next_f64() - 0.5) * 0.4;
                let distance = (10.0 + (index * 0.7).sqrt()).min(28.0);
                let spawn_x = x + angle.cos() * distance;
                let spawn_z = z + angle.sin() * distance;
                let spawn_y = if self.mode == Mode::Fly {
                    *y
                } else {
                    surface_y(world, spawn_x, spawn_z).unwrap_or(*y)
                };
                (
                    (
                        spawn_x,
                        clear_spawn_y(world, spawn_x, spawn_y, spawn_z),
                        spawn_z,
                    ),
                    // Face the player so the first thing they see is the
                    // mannequin's front, not the back of its head.
                    (angle.to_degrees() + 90.0) as f32,
                )
            }
            None => {
                let jitter_x = (self.rng.next_f64() - 0.5) * 4.0;
                let jitter_z = (self.rng.next_f64() - 0.5) * 4.0;
                let spawn = world.get_spawn_location();
                let spawn_x = f64::from(spawn.pos.x) + 0.5 + jitter_x;
                let spawn_z = f64::from(spawn.pos.z) + 0.5 + jitter_z;
                let spawn_y = if self.mode == Mode::Fly {
                    f64::from(spawn.pos.y)
                } else {
                    surface_y(world, spawn_x, spawn_z).unwrap_or(f64::from(spawn.pos.y))
                };
                (
                    (
                        spawn_x,
                        clear_spawn_y(world, spawn_x, spawn_y, spawn_z),
                        spawn_z,
                    ),
                    0.0,
                )
            }
        };
        let name = format!("StressBot_{}", self.bots.len());

        if self.bots.is_empty() {
            info!(
                "Spawning bots in world '{}' at ({:.1}, {:.1}, {:.1})",
                world.get_name(),
                position.0,
                position.1,
                position.2
            );
        }

        // Mannequin is Minecraft's player-shaped NPC entity. Unlike a
        // `minecraft:player` entity it does not require a tab-list profile,
        // which the Pumpkin 0.2.1 plugin host cannot send.
        let entity = world.spawn_entity(EntityType::Mannequin, position);
        entity.set_has_gravity(false);
        entity.set_invulnerable(true);
        entity.set_custom_name(TextComponent::text(&name));
        entity.set_custom_name_visible(true);
        entity.set_rotation(yaw, 0.0);

        let uuid = entity.get_uuid();

        let mut bot = Bot {
            entity,
            uuid,
            name,
            spawn: position,
            target: position,
            goto_target: None,
            mode: self.mode,
            stuck: 0,
        };
        bot.pick_target(self.radius, self.mode, &mut self.rng);
        sync_tab_list_add(server, &bot);
        announce(server, &bot.name, "joined the game");
        self.bots.push(bot);
    }

    fn move_all(&mut self) {
        let speed = self.speed;
        let radius = self.radius;
        let mut rng = self.rng;
        for bot in &mut self.bots {
            bot.tick(speed, radius, &mut rng);
        }
        self.rng = rng;
    }
}

pub fn tick(server: &Server) {
    let mut manager = MANAGER.lock().unwrap_or_else(|e| e.into_inner());
    manager.tick = manager.tick.wrapping_add(1);

    if manager.spawning {
        if manager.bots.len() >= manager.target_count {
            manager.spawning = false;
            info!("All {} bots spawned.", manager.target_count);
        } else if manager.tick.is_multiple_of(2) {
            // Same two-tick stagger as the Paper plugin.
            manager.spawn_one(server);
        }
    }

    manager.move_all();
}

pub fn start(server: &Server, origin: Option<(String, Vec3)>) {
    // Chunks around the player are loaded by the time a command runs, so this
    // is the reliable point to sweep up bots left behind by a crashed session.
    cleanup_stale(server);

    let mut manager = MANAGER.lock().unwrap_or_else(|e| e.into_inner());
    manager.spawn_origin = origin;
    manager.spawning = true;

    if manager.bots.len() > manager.target_count {
        let target = manager.target_count;
        let removed: Vec<Bot> = manager.bots.drain(target..).collect();
        manager.spawning = false;
        drop(manager);
        for bot in &removed {
            sync_tab_list_remove(server, bot.uuid);
            bot.entity.remove();
            announce(server, &bot.name, "left the game");
        }
        return;
    }

    if manager.bots.len() >= manager.target_count {
        manager.spawning = false;
        return;
    }

    manager.spawn_one(server);
}

pub fn stop(server: &Server) -> usize {
    let bots = {
        let mut manager = MANAGER.lock().unwrap_or_else(|e| e.into_inner());
        manager.spawning = false;
        std::mem::take(&mut manager.bots)
    };

    let count = bots.len();
    for bot in &bots {
        sync_tab_list_remove(server, bot.uuid);
        bot.entity.remove();
        announce(server, &bot.name, "left the game");
    }

    // Also clear bot entities that survived an earlier crash and were reloaded
    // from the world. Managed bots are already gone, so everything named
    // `StressBot_*` here is stale.
    cleanup_stale(server);

    if count > 0 {
        info!("All bots removed.");
    }
    count
}

pub fn shutdown(server: &Server) {
    let bots = {
        let mut manager = MANAGER.lock().unwrap_or_else(|e| e.into_inner());
        manager.spawning = false;
        std::mem::take(&mut manager.bots)
    };
    for bot in &bots {
        sync_tab_list_remove(server, bot.uuid);
        bot.entity.remove();
        announce(server, &bot.name, "left the game");
    }
}

pub fn set_target_count(count: i64) {
    let count = count.clamp(0, MAX_BOTS as i64) as usize;
    let mut manager = MANAGER.lock().unwrap_or_else(|e| e.into_inner());
    manager.target_count = count;
}

pub fn set_speed(speed: f64) {
    let mut manager = MANAGER.lock().unwrap_or_else(|e| e.into_inner());
    manager.speed = speed.max(0.0);
}

pub fn set_radius(radius: f64) {
    let mut manager = MANAGER.lock().unwrap_or_else(|e| e.into_inner());
    manager.radius = radius.max(0.0);
    let mode = manager.mode;
    let radius = manager.radius;
    let mut rng = manager.rng;
    for bot in &mut manager.bots {
        // The Paper plugin re-rolls the wander target but leaves an active
        // /goto destination untouched.
        bot.pick_target(radius, mode, &mut rng);
    }
    manager.rng = rng;
}

pub fn set_mode(mode: Mode) {
    let mut manager = MANAGER.lock().unwrap_or_else(|e| e.into_inner());
    manager.mode = mode;
    let radius = manager.radius;
    let mut rng = manager.rng;
    for bot in &mut manager.bots {
        bot.pick_target(radius, mode, &mut rng);
        bot.goto_target = None;
    }
    manager.rng = rng;
}

pub fn chat(server: &Server, message: &str) {
    let names: Vec<String> = {
        let manager = MANAGER.lock().unwrap_or_else(|e| e.into_inner());
        manager.bots.iter().map(|bot| bot.name.clone()).collect()
    };

    for name in names {
        server.broadcast(&format!("<{name}> {message}"));
    }
}

pub fn goto_location(server: &Server, target: Vec3, world_name: &str) {
    let mut manager = MANAGER.lock().unwrap_or_else(|e| e.into_inner());
    let radius = manager.radius;
    let mut rng = manager.rng;
    for bot in &mut manager.bots {
        bot.goto_target = None;
        if bot.entity.get_world().get_name() == world_name {
            bot.goto_target = Some(target);
        } else {
            // Walking between dimensions is impossible; move the bot across
            // dimensions immediately and re-anchor its wander point there so
            // it does not try to walk back toward coordinates in another world.
            if let Some(world) = server.get_world_by_name(world_name) {
                bot.entity.teleport(target, world);
                bot.spawn = target;
                bot.pick_target(radius, bot.mode, &mut rng);
            }
        }
    }
    manager.rng = rng;
}

pub fn teleport_all(player: &Player) -> usize {
    let position = player.get_position();
    let mut manager = MANAGER.lock().unwrap_or_else(|e| e.into_inner());
    let radius = manager.radius;
    let mut rng = manager.rng;

    for (index, bot) in manager.bots.iter_mut().enumerate() {
        let angle = index as f64 * 2.399_963_229_728_653
            + (rng.next_f64() - 0.5) * 0.4;
        let distance = (10.0 + (index as f64 * 0.7).sqrt()).min(28.0);
        let x = position.0 + angle.cos() * distance;
        let z = position.2 + angle.sin() * distance;
        let world = player.get_world();
        let y = if bot.mode == Mode::Walk {
            surface_y(&world, x, z).unwrap_or(position.1)
        } else {
            position.1
        };
        let y = clear_spawn_y(&world, x, y, z);

        let target = (x, y, z);
        bot.entity.teleport(target, world);
        bot.entity
            .set_rotation((angle.to_degrees() + 90.0) as f32, 0.0);
        bot.spawn = target;
        bot.goto_target = None;
        // Keep wandering around the player after being pulled in.
        bot.pick_target(radius, bot.mode, &mut rng);
    }
    manager.rng = rng;

    manager.bots.len()
}

pub fn bot_count() -> usize {
    MANAGER
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .bots
        .len()
}

pub fn target_count() -> usize {
    MANAGER
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .target_count
}

pub fn speed() -> f64 {
    MANAGER
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .speed
}

pub fn radius() -> f64 {
    MANAGER
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .radius
}

pub fn mode() -> Mode {
    MANAGER
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .mode
}
