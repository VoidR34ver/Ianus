//! The Ianus command-line tool.
//!
//! ```text
//! ianus_cli validate <room>...
//! ianus_cli run <room> [--input SCRIPT] [--ticks N] [--seed N]
//!                      [--record FILE] [--expect-hash HEX]
//! ianus_cli replay <room> <replay-file>
//! ```
//!
//! Add `--json` to any command for one machine-readable JSON object on
//! stdout (errors too, as `{"ok":false,"error":...}`). That is the mode for
//! AI agents and scripts.
//!
//! Everything is headless: no window is ever opened. Exit codes: `0` success,
//! `1` the check failed (room has errors, hash differs, replay diverged),
//! `2` bad usage or an unreadable file.
//!
//! Input scripts are comma-separated `direction:ticks` steps, for example
//! `right:120,up+left:30,none:60`. Directions are `left right up down none`,
//! combined with `+`.

use std::io::Write;
use std::path::Path;
use std::time::Duration;

use bevy_app::App;
use bevy_time::{Fixed, Time, TimePlugin, TimeUpdateStrategy};
use ianus::prelude::*;
use ianus::{ReplayMode, finish_recording};
use ianus_sim::Replay;

const USAGE: &str = "\
Usage:
  ianus_cli validate <room>...
  ianus_cli run <room> [--input SCRIPT] [--ticks N] [--seed N] [--record FILE] [--expect-hash HEX]
  ianus_cli replay <room> <replay-file>

Add --json to any command for one JSON object on stdout (for AI agents and scripts).
SCRIPT is steps of direction:ticks, e.g. right:120,up+left:30,none:60.
Exit codes: 0 ok, 1 check failed, 2 bad usage or unreadable file.";

/// Run the tool. Returns the process exit code.
pub fn execute(args: &[String], out: &mut dyn Write, err: &mut dyn Write) -> u8 {
    let json = args.iter().any(|a| a == "--json");
    let args: Vec<String> = args.iter().filter(|a| *a != "--json").cloned().collect();
    let result = match args.first().map(String::as_str) {
        Some("validate") => validate(&args[1..], out, json),
        Some("run") => run(&args[1..], out, json),
        Some("replay") => replay(&args[1..], out, json),
        Some("help" | "--help" | "-h") => {
            let _ = writeln!(out, "{USAGE}");
            Ok(0)
        }
        _ => Err(CliError::Usage("missing or unknown command".into())),
    };
    match result {
        Ok(code) => code,
        Err(CliError::Usage(msg)) => {
            report_error(out, err, json, &msg, true);
            2
        }
        Err(CliError::Failed(msg)) => {
            report_error(out, err, json, &msg, false);
            2
        }
    }
}

fn report_error(out: &mut dyn Write, err: &mut dyn Write, json: bool, msg: &str, usage: bool) {
    if json {
        let _ = writeln!(out, "{{\"ok\":false,\"error\":{}}}", jstr(msg));
    } else if usage {
        let _ = writeln!(err, "error: {msg}\n\n{USAGE}");
    } else {
        let _ = writeln!(err, "error: {msg}");
    }
}

/// A JSON string literal.
fn jstr(s: &str) -> String {
    let mut o = String::from("\"");
    for c in s.chars() {
        match c {
            '"' => o.push_str("\\\""),
            '\\' => o.push_str("\\\\"),
            '\n' => o.push_str("\\n"),
            '\t' => o.push_str("\\t"),
            c if (c as u32) < 0x20 => o.push_str(&format!("\\u{:04x}", c as u32)),
            c => o.push(c),
        }
    }
    o.push('"');
    o
}

enum CliError {
    Usage(String),
    Failed(String),
}

fn read_room(path: &str) -> Result<Room, CliError> {
    let text = std::fs::read_to_string(path)
        .map_err(|e| CliError::Failed(format!("cannot read {path}: {e}")))?;
    Room::from_text(&text).map_err(|e| CliError::Failed(format!("{path}: {e}")))
}

fn validate(args: &[String], out: &mut dyn Write, json: bool) -> Result<u8, CliError> {
    if args.is_empty() {
        return Err(CliError::Usage(
            "validate needs at least one room file".into(),
        ));
    }
    let mut code = 0;
    let mut rooms_json = Vec::new();
    for path in args {
        let room = read_room(path)?;
        let problems = room.validate();
        let errors = problems
            .iter()
            .filter(|p| p.severity == Severity::Error)
            .count();
        let warnings = problems.len() - errors;
        if json {
            let list: Vec<String> = problems
                .iter()
                .map(|p| {
                    let sev = if p.severity == Severity::Error {
                        "error"
                    } else {
                        "warning"
                    };
                    format!(
                        "{{\"severity\":\"{sev}\",\"message\":{}}}",
                        jstr(&p.message)
                    )
                })
                .collect();
            rooms_json.push(format!(
                "{{\"path\":{},\"ok\":{},\"errors\":{errors},\"warnings\":{warnings},\"problems\":[{}]}}",
                jstr(path),
                errors == 0,
                list.join(",")
            ));
        } else {
            for p in &problems {
                let _ = writeln!(out, "{path}: {p}");
            }
            let verdict = if errors == 0 { "ok" } else { "FAILED" };
            let _ = writeln!(
                out,
                "{path}: {verdict} ({errors} error(s), {warnings} warning(s))"
            );
        }
        if errors > 0 {
            code = 1;
        }
    }
    if json {
        let _ = writeln!(
            out,
            "{{\"ok\":{},\"rooms\":[{}]}}",
            code == 0,
            rooms_json.join(",")
        );
    }
    Ok(code)
}

/// Parse `right:120,up+left:30,none:60` into one input per tick.
pub fn parse_script(text: &str) -> Result<Vec<MoveInput>, String> {
    let mut inputs = Vec::new();
    for step in text.split(',').map(str::trim).filter(|s| !s.is_empty()) {
        let (dirs, count) = step
            .split_once(':')
            .ok_or_else(|| format!("step '{step}' should look like right:120"))?;
        let count: usize = count
            .trim()
            .parse()
            .map_err(|_| format!("'{count}' in step '{step}' is not a whole number of ticks"))?;
        let mut input = MoveInput::NONE;
        for d in dirs.split('+').map(|d| d.trim().to_lowercase()) {
            input = input.union(match d.as_str() {
                "left" => MoveInput::LEFT,
                "right" => MoveInput::RIGHT,
                "up" => MoveInput::UP,
                "down" => MoveInput::DOWN,
                "none" => MoveInput::NONE,
                other => {
                    return Err(format!(
                        "unknown direction '{other}' (use left, right, up, down or none)"
                    ));
                }
            });
        }
        if inputs.len() + count > 1_000_000 {
            return Err("script is longer than 1,000,000 ticks".into());
        }
        inputs.extend(std::iter::repeat_n(input, count));
    }
    Ok(inputs)
}

/// A headless app with the room loaded and playing from tick 0.
fn headless_app(path: &str) -> Result<App, CliError> {
    let mut app = App::new();
    app.add_plugins((TimePlugin, IanusPlugin));
    let step = app.world().resource::<Time<Fixed>>().timestep();
    // Exactly one fixed tick per update, whatever the real clock does.
    app.insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_secs_f64(
        step.as_secs_f64(),
    )));
    load_room_file(app.world_mut(), Path::new(path)).map_err(|e| match e {
        LoadError::Invalid(problems) => CliError::Failed(format!(
            "{path} has errors (run `validate` to see them): {}",
            problems
                .iter()
                .filter(|p| p.severity == Severity::Error)
                .map(|p| p.message.as_str())
                .collect::<Vec<_>>()
                .join("; ")
        )),
        other => CliError::Failed(other.to_string()),
    })?;
    if !play(app.world_mut()) {
        return Err(CliError::Failed("room could not enter play mode".into()));
    }
    Ok(app)
}

fn tick(app: &App) -> u64 {
    app.world().resource::<Tick>().0
}

fn take_value<'a>(args: &'a [String], i: &mut usize, flag: &str) -> Result<&'a str, CliError> {
    *i += 1;
    args.get(*i)
        .map(String::as_str)
        .ok_or_else(|| CliError::Usage(format!("{flag} needs a value")))
}

fn run(args: &[String], out: &mut dyn Write, json: bool) -> Result<u8, CliError> {
    let mut room_path = None;
    let mut script = Vec::new();
    let mut ticks: Option<u64> = None;
    let mut seed = 0u64;
    let mut record = None;
    let mut expect: Option<u64> = None;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--input" => {
                script =
                    parse_script(take_value(args, &mut i, "--input")?).map_err(CliError::Usage)?;
            }
            "--ticks" => {
                ticks = Some(
                    take_value(args, &mut i, "--ticks")?
                        .parse()
                        .map_err(|_| CliError::Usage("--ticks must be a whole number".into()))?,
                );
            }
            "--seed" => {
                seed = take_value(args, &mut i, "--seed")?
                    .parse()
                    .map_err(|_| CliError::Usage("--seed must be a whole number".into()))?;
            }
            "--record" => record = Some(take_value(args, &mut i, "--record")?.to_string()),
            "--expect-hash" => {
                let v = take_value(args, &mut i, "--expect-hash")?;
                let v = v.trim_start_matches("0x");
                expect = Some(u64::from_str_radix(v, 16).map_err(|_| {
                    CliError::Usage("--expect-hash must be a hexadecimal number".into())
                })?);
            }
            flag if flag.starts_with("--") => {
                return Err(CliError::Usage(format!("unknown option {flag}")));
            }
            path if room_path.is_none() => room_path = Some(path.to_string()),
            _ => return Err(CliError::Usage("run takes exactly one room file".into())),
        }
        i += 1;
    }
    let room_path = room_path.ok_or_else(|| CliError::Usage("run needs a room file".into()))?;
    let total = ticks.unwrap_or(script.len() as u64);
    if total > 1_000_000 {
        return Err(CliError::Usage("at most 1,000,000 ticks per run".into()));
    }

    let mut app = headless_app(&room_path)?;
    start_recording(app.world_mut(), seed)
        .map_err(|e| CliError::Failed(format!("restart failed: {e}")))?;
    while tick(&app) < total {
        let t = tick(&app) as usize;
        let held = script.get(t).copied().unwrap_or(MoveInput::NONE);
        app.world_mut().resource_mut::<MoveIntent>().set_held(held);
        let before = tick(&app);
        app.update();
        if tick(&app) > before + 1 {
            return Err(CliError::Failed(
                "more than one tick ran in one step".into(),
            ));
        }
    }
    let replay = finish_recording(app.world_mut())
        .ok_or_else(|| CliError::Failed("recording was lost".into()))?;
    let hash = replay.final_hash.unwrap_or_default();

    let mut replay_file = None;
    if let Some(file) = record {
        std::fs::write(&file, replay.to_bytes())
            .map_err(|e| CliError::Failed(format!("cannot write {file}: {e}")))?;
        replay_file = Some(file);
    }
    let matches = expect.map(|want| want == hash);
    let code = u8::from(matches == Some(false));
    if json {
        let mut o = format!(
            "{{\"ok\":{},\"room\":{},\"ticks\":{},\"seed\":{seed},\"hash\":\"{hash:016x}\"",
            code == 0,
            jstr(&room_path),
            tick(&app)
        );
        if let Some(f) = &replay_file {
            o += &format!(",\"replay_file\":{}", jstr(f));
        }
        if let Some(m) = matches {
            o += &format!(",\"hash_matches_expected\":{m}");
        }
        let _ = writeln!(out, "{o}}}");
    } else {
        let _ = writeln!(out, "room: {room_path}");
        let _ = writeln!(out, "ticks: {}", tick(&app));
        let _ = writeln!(out, "seed: {seed}");
        let _ = writeln!(out, "hash: {hash:016x}");
        if let Some(f) = &replay_file {
            let _ = writeln!(out, "replay written: {f}");
        }
        match (matches, expect) {
            (Some(false), Some(want)) => {
                let _ = writeln!(out, "MISMATCH: expected {want:016x}");
            }
            (Some(true), _) => {
                let _ = writeln!(out, "hash matches the expected value");
            }
            _ => {}
        }
    }
    Ok(code)
}

fn replay(args: &[String], out: &mut dyn Write, json: bool) -> Result<u8, CliError> {
    let [room_path, file] = args else {
        return Err(CliError::Usage(
            "replay needs a room file and a replay file".into(),
        ));
    };
    let bytes =
        std::fs::read(file).map_err(|e| CliError::Failed(format!("cannot read {file}: {e}")))?;
    let rep = Replay::from_bytes(&bytes).map_err(|e| CliError::Failed(format!("{file}: {e}")))?;
    let length = rep.len() as u64;

    let mut app = headless_app(room_path)?;
    start_playback(app.world_mut(), rep)
        .map_err(|e| CliError::Failed(format!("restart failed: {e}")))?;
    // The tick that notices the end of the replay is one past its length.
    let mut guard = 0u64;
    let matched = loop {
        app.update();
        if let ReplayMode::Finished { matched } = app.world().resource::<ReplayMode>() {
            break *matched;
        }
        guard += 1;
        if guard > length + 10 {
            return Err(CliError::Failed("playback never finished".into()));
        }
    };
    let code = u8::from(matched == Some(false));
    if json {
        let m = match matched {
            Some(v) => v.to_string(),
            None => "null".into(),
        };
        let _ = writeln!(
            out,
            "{{\"ok\":{},\"replay_file\":{},\"ticks\":{length},\"matched\":{m}}}",
            code == 0,
            jstr(file)
        );
    } else {
        let _ = writeln!(out, "replay: {file} ({length} ticks)");
        let line = match matched {
            Some(true) => "result: MATCH, the replay reproduced the recorded state",
            Some(false) => "result: DIVERGED, the final state differs from the recording",
            None => "result: finished, but the file has no hash to compare",
        };
        let _ = writeln!(out, "{line}");
    }
    Ok(code)
}

#[cfg(test)]
mod tests;
