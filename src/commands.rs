use pumpkin_plugin_api::{
    command::{
        Arg, ArgumentType, Command, CommandError, CommandNode, CommandSender, CommandSuggestion,
        CommandSuggestions, ConsumedArgs, StringType, SuggestionRequest,
    },
    commands::{CommandHandler, CommandSuggestionHandler},
    text::TextComponent,
    Server,
};

use crate::bots::{self, Mode, MAX_BOTS};

const PREFIX: &str = "§6[StressTest] §f";
const ERR: &str = "§c";
const ACCENT: &str = "§a";
const MUTED: &str = "§7";
const ARG_NAME: &str = "args";

fn legacy(text: &str) -> TextComponent {
    TextComponent::from_legacy_string(text)
}

fn send(sender: &CommandSender, text: &str) {
    sender.send_message(legacy(text));
}

fn prefixed(sender: &CommandSender, text: &str) {
    send(sender, &format!("{PREFIX}{text}"));
}

fn error(sender: &CommandSender, text: &str) {
    send(sender, &format!("{ERR}{text}"));
}

fn fmt_num(value: f64) -> String {
    format!("{value:?}")
}

fn string_arg(args: &ConsumedArgs, key: &str) -> Option<String> {
    match args.get_value(key) {
        Arg::Simple(value) | Arg::Msg(value) => Some(value),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// /start [amount] [none|walk|fly]
// ---------------------------------------------------------------------------

struct StartRoot;

impl CommandHandler for StartRoot {
    fn handle(
        &self,
        sender: CommandSender,
        server: Server,
        _args: ConsumedArgs,
    ) -> Result<i32, CommandError> {
        handle_start(&sender, &server, None);
        Ok(1)
    }
}

struct StartArgs;

impl CommandHandler for StartArgs {
    fn handle(
        &self,
        sender: CommandSender,
        server: Server,
        args: ConsumedArgs,
    ) -> Result<i32, CommandError> {
        handle_start(&sender, &server, string_arg(&args, "args"));
        Ok(1)
    }
}

fn handle_start(sender: &CommandSender, server: &Server, raw: Option<String>) {
    if let Some(raw) = raw {
        for token in raw.split_whitespace() {
            if let Some(mode) = Mode::from_str(token) {
                bots::set_mode(mode);
                continue;
            }
            match token.parse::<i64>() {
                Ok(count) => bots::set_target_count(count),
                Err(_) => {
                    error(sender, "Usage: /start [count] [none|walk|fly]");
                    return;
                }
            }
        }
    }

    let target = bots::target_count();
    if target == 0 {
        error(
            sender,
            "Set a count first: /stress count <n>  or  /start <n>",
        );
        return;
    }

    let sender_origin = sender.as_player().map(|player| {
        let position = player.get_position();
        (player.get_world().get_name(), position)
    });
    let used_sender_position = sender_origin.is_some();
    let origin = sender_origin.or_else(|| {
        // Console-started stress tests should still be visible: spawn at the
        // first online player instead of a possibly distant world spawn.
        server.get_all_players().into_iter().next().map(|player| {
            let position = player.get_position();
            (player.get_world().get_name(), position)
        })
    });
    let used_player_position = origin.is_some();

    bots::start(server, origin);
    prefixed(
        sender,
        &format!(
            "Spawning {ACCENT}{target}{MUTED} bots | speed={} | radius={} | mode={}",
            fmt_num(bots::speed()),
            fmt_num(bots::radius()),
            bots::mode().as_str()
        ),
    );
    if used_sender_position {
        prefixed(sender, "Bots are spawning in a ring around you.");
    } else if used_player_position {
        prefixed(sender, "Bots are spawning around the first online player.");
    }
}

// ---------------------------------------------------------------------------
// /goto [x y z [world]]
// ---------------------------------------------------------------------------

struct GotoRoot;

impl CommandHandler for GotoRoot {
    fn handle(
        &self,
        sender: CommandSender,
        server: Server,
        _args: ConsumedArgs,
    ) -> Result<i32, CommandError> {
        handle_goto(&sender, &server, None);
        Ok(1)
    }
}

struct GotoArgs;

impl CommandHandler for GotoArgs {
    fn handle(
        &self,
        sender: CommandSender,
        server: Server,
        args: ConsumedArgs,
    ) -> Result<i32, CommandError> {
        handle_goto(&sender, &server, string_arg(&args, "args"));
        Ok(1)
    }
}

fn handle_goto(sender: &CommandSender, server: &Server, raw: Option<String>) {
    let target;
    let world;

    match raw {
        None => {
            let Some(player) = sender.as_player() else {
                error(sender, "Usage: /goto <x> <y> <z> [world]");
                return;
            };
            target = player.get_position();
            world = player.get_world();
        }
        Some(raw) => {
            let tokens: Vec<&str> = raw.split_whitespace().collect();
            if tokens.len() < 3 {
                error(sender, "Usage: /goto [x y z [world]]");
                return;
            }

            let parsed = (
                tokens[0].parse::<f64>(),
                tokens[1].parse::<f64>(),
                tokens[2].parse::<f64>(),
            );
            let (Ok(x), Ok(y), Ok(z)) = parsed else {
                error(sender, "Usage: /goto <x> <y> <z> [world]");
                return;
            };
            target = (x, y, z);

            if tokens.len() >= 4 {
                let name = tokens[3..].join(" ");
                match server.get_world_by_name(&name) {
                    Some(found) => world = found,
                    None => {
                        error(sender, &format!("Unknown world: {name}"));
                        return;
                    }
                }
            } else if let Some(player) = sender.as_player() {
                world = player.get_world();
            } else {
                let Some(first) = server.get_all_worlds().into_iter().next() else {
                    error(sender, "No worlds are loaded.");
                    return;
                };
                world = first;
            }
        }
    }

    let count = bots::bot_count();
    bots::goto_location(server, target, &world.get_name());
    prefixed(
        sender,
        &format!(
            "Sending {ACCENT}{count}{MUTED} bots to §f{}§7, §f{}§7, §f{}",
            target.0.trunc() as i64,
            target.1.trunc() as i64,
            target.2.trunc() as i64
        ),
    );
}

// ---------------------------------------------------------------------------
// /stress <count|speed|radius|mode|chat|tp|goto|status|stop|start>
// ---------------------------------------------------------------------------

struct StressRoot;

impl CommandHandler for StressRoot {
    fn handle(
        &self,
        sender: CommandSender,
        _server: Server,
        _args: ConsumedArgs,
    ) -> Result<i32, CommandError> {
        send_status(&sender);
        Ok(1)
    }
}

struct StressSubcommand(&'static str);

impl CommandHandler for StressSubcommand {
    fn handle(
        &self,
        sender: CommandSender,
        server: Server,
        args: ConsumedArgs,
    ) -> Result<i32, CommandError> {
        let rest = string_arg(&args, ARG_NAME).unwrap_or_default();
        if rest.is_empty() {
            handle_stress(&sender, &server, self.0);
        } else {
            handle_stress(&sender, &server, &format!("{} {rest}", self.0));
        }
        Ok(1)
    }
}

#[derive(Clone, Copy)]
enum SuggestionKind {
    Count,
    Speed,
    Radius,
    Mode,
    Chat,
    Start { base: usize },
    Goto { base: usize },
}

struct StressSuggestions(SuggestionKind);

/// Start of the token the cursor is currently inside.
///
/// Pumpkin reports `remaining` as everything the greedy argument has consumed
/// since it started, not just the token under the cursor (`"10 "` while typing
/// `/stress start 10 `). Deriving the token from the cursor keeps filtering and
/// argument indexing correct.
fn current_token_start(request: &SuggestionRequest) -> usize {
    let end = (request.cursor as usize).min(request.input.len());
    request.input[..end]
        .rfind(char::is_whitespace)
        .map_or(0, |index| index + 1)
}

fn argument_index(request: &SuggestionRequest, base: usize) -> usize {
    request.input[..current_token_start(request)]
        .split_whitespace()
        .count()
        .saturating_sub(base)
}

fn suggestion_list<I>(request: &SuggestionRequest, values: I) -> CommandSuggestions
where
    I: IntoIterator<Item = String>,
{
    let end = (request.cursor as usize).min(request.input.len());
    let prefix = request.input[current_token_start(request)..end].to_ascii_lowercase();
    let values = values
        .into_iter()
        .filter(|value| value.to_ascii_lowercase().starts_with(&prefix))
        .map(|value| CommandSuggestion {
            value,
            tooltip: None,
        })
        .collect();

    CommandSuggestions {
        start: request.start,
        length: request.remaining.len() as u32,
        values,
    }
}

fn fixed_suggestions(request: &SuggestionRequest, values: &[&str]) -> CommandSuggestions {
    suggestion_list(request, values.iter().map(|value| (*value).to_string()))
}

impl CommandSuggestionHandler for StressSuggestions {
    fn suggest(
        &self,
        _sender: CommandSender,
        server: Server,
        request: SuggestionRequest,
    ) -> CommandSuggestions {
        match self.0 {
            SuggestionKind::Count => {
                fixed_suggestions(&request, &["10", "25", "50", "100", "500"])
            }
            SuggestionKind::Start { base } => {
                let values: &[&str] = match argument_index(&request, base) {
                    0 => &["10", "25", "50", "100"],
                    1 => &["none", "walk", "fly"],
                    _ => &[],
                };
                fixed_suggestions(&request, values)
            }
            SuggestionKind::Speed => fixed_suggestions(
                &request,
                &["0.05", "0.1", "0.2", "0.5", "1.0"],
            ),
            SuggestionKind::Radius => fixed_suggestions(
                &request,
                &["25", "50", "100", "250", "500", "1000"],
            ),
            SuggestionKind::Mode => fixed_suggestions(&request, &["none", "walk", "fly"]),
            SuggestionKind::Chat => {
                fixed_suggestions(&request, &["Hello!", "Test message", "Stress test"])
            }
            SuggestionKind::Goto { base } => match argument_index(&request, base) {
                0 => fixed_suggestions(&request, &["<x>"]),
                1 => fixed_suggestions(&request, &["<y>"]),
                2 => fixed_suggestions(&request, &["<z>"]),
                _ => suggestion_list(
                    &request,
                    server
                        .get_all_worlds()
                        .into_iter()
                        .map(|world| world.get_name()),
                ),
            },
        }
    }
}

fn handle_stress(sender: &CommandSender, server: &Server, raw: &str) {
    let raw = raw.trim();
    if raw.is_empty() {
        send_status(sender);
        return;
    }

    let (subcommand, rest) = match raw.find(char::is_whitespace) {
        Some(index) => (&raw[..index], raw[index..].trim_start()),
        None => (raw, ""),
    };

    match subcommand.to_ascii_lowercase().as_str() {
        "start" => handle_start(sender, server, (!rest.is_empty()).then(|| rest.to_string())),
        "stop" => {
            bots::stop(server);
            prefixed(sender, &format!("{ERR}All bots removed."));
        }
        "count" => {
            if rest.is_empty() {
                error(sender, "Usage: /stress count <number>");
                return;
            }
            match rest.parse::<i64>() {
                Ok(count) => {
                    bots::set_target_count(count);
                    prefixed(
                        sender,
                        &format!(
                            "Count set to {ACCENT}{}{MUTED}. Use §f/start{MUTED} to spawn the bots in!",
                            bots::target_count()
                        ),
                    );
                }
                Err(_) => error(sender, "Invalid number."),
            }
        }
        "speed" => {
            if rest.is_empty() {
                error(sender, "Usage: /stress speed <number>");
                return;
            }
            match rest.parse::<f64>() {
                Ok(speed) => {
                    bots::set_speed(speed);
                    prefixed(
                        sender,
                        &format!(
                            "Speed set to: {ACCENT}{}{MUTED} blocks/tick.",
                            fmt_num(bots::speed())
                        ),
                    );
                }
                Err(_) => error(sender, "Invalid number."),
            }
        }
        "radius" => {
            if rest.is_empty() {
                error(sender, "Usage: /stress radius <number>");
                return;
            }
            match rest.parse::<f64>() {
                Ok(radius) => {
                    bots::set_radius(radius);
                    prefixed(
                        sender,
                        &format!("Radius set to: {ACCENT}{}{MUTED} blocks.", fmt_num(bots::radius())),
                    );
                }
                Err(_) => error(sender, "Invalid number."),
            }
        }
        "mode" => {
            if rest.is_empty() {
                error(sender, "Usage: /stress mode <none|fly|walk>");
                return;
            }
            match Mode::from_str(rest) {
                Some(mode) => {
                    bots::set_mode(mode);
                    prefixed(sender, &format!("Mode → {ACCENT}{}", mode.as_str()));
                }
                None => error(sender, "Mode must be 'none', 'fly', or 'walk'."),
            }
        }
        "chat" => {
            if rest.is_empty() {
                error(sender, "Usage: /stress chat <message>");
                return;
            }
            bots::chat(server, rest);
            prefixed(sender, &format!("All bots sent: {MUTED}{rest}"));
        }
        "tp" | "teleport" => {
            let Some(player) = sender.as_player() else {
                error(sender, "Must be a player.");
                return;
            };
            let count = bots::teleport_all(&player);
            prefixed(
                sender,
                &format!("Teleported {ACCENT}{count}{MUTED} bots to you."),
            );
        }
        "goto" => handle_goto(sender, server, (!rest.is_empty()).then(|| rest.to_string())),
        "status" => send_status(sender),
        _ => error(
            sender,
            "Unknown subcommand. Try: start, stop, count, speed, radius, mode, chat, tp, goto, status",
        ),
    }
}

fn send_status(sender: &CommandSender) {
    let line = "§7──────────────────────";
    prefixed(sender, line);
    prefixed(
        sender,
        &format!(
            "Bots: {ACCENT}{}{MUTED} / target {ACCENT}{}{MUTED} (cap {MAX_BOTS})",
            bots::bot_count(),
            bots::target_count()
        ),
    );
    send(
        sender,
        &format!(
            "{MUTED}  Speed: §f{}{MUTED}  Radius: §f{}{MUTED}  Mode: §f{}",
            fmt_num(bots::speed()),
            fmt_num(bots::radius()),
            bots::mode().as_str()
        ),
    );
    send(
        sender,
        &format!("{MUTED}  /stress count|speed|radius|mode|chat|tp|goto → configure"),
    );
    send(
        sender,
        &format!("{MUTED}  /start [n] [mode] → spawn  |  /stress stop → remove all"),
    );
    send(
        sender,
        &format!("{MUTED}  /goto [x y z] → send bots to location"),
    );
    send(
        sender,
        &format!("{MUTED}  NPC mode: not in /list count; tab-list entries need a patched host."),
    );
    prefixed(sender, line);
}

// ---------------------------------------------------------------------------
// Registration
// ---------------------------------------------------------------------------

fn greedy_args_with(kind: SuggestionKind) -> CommandNode {
    CommandNode::argument(ARG_NAME, &ArgumentType::String(StringType::Greedy))
        .suggest(StressSuggestions(kind))
}

pub fn start_command() -> Command {
    Command::new(
        &["start".to_string()],
        "Spawn stress bots using the current settings",
    )
    .execute(StartRoot)
    .then(greedy_args_with(SuggestionKind::Start { base: 1 }).execute(StartArgs))
}

pub fn goto_command() -> Command {
    Command::new(
        &["goto".to_string()],
        "Send all stress bots to a location",
    )
    .execute(GotoRoot)
    .then(greedy_args_with(SuggestionKind::Goto { base: 1 }).execute(GotoArgs))
}

pub fn stress_command() -> Command {
    let simple = |name: &'static str| {
        CommandNode::literal(name).execute(StressSubcommand(name))
    };
    let with_value = |name: &'static str, kind: SuggestionKind| {
        CommandNode::literal(name)
            .execute(StressSubcommand(name))
            .then(greedy_args_with(kind).execute(StressSubcommand(name)))
    };

    Command::new(&["stress".to_string()], "Control the stress bots")
        .execute(StressRoot)
        .then(simple("status"))
        .then(with_value("start", SuggestionKind::Start { base: 2 }))
        .then(simple("stop"))
        .then(with_value("count", SuggestionKind::Count))
        .then(with_value("speed", SuggestionKind::Speed))
        .then(with_value("radius", SuggestionKind::Radius))
        .then(with_value("mode", SuggestionKind::Mode))
        .then(with_value("chat", SuggestionKind::Chat))
        .then(simple("tp"))
        .then(simple("teleport"))
        .then(with_value("goto", SuggestionKind::Goto { base: 2 }))
}
