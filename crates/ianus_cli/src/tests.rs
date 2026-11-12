use super::*;

fn room(name: &str) -> String {
    format!("{}/../../rooms/{name}", env!("CARGO_MANIFEST_DIR"))
}

fn tmp(name: &str) -> String {
    let dir = std::env::temp_dir().join(format!("ianus_cli_test_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir.join(name).to_string_lossy().into_owned()
}

fn cli(args: &[&str]) -> (u8, String, String) {
    let args: Vec<String> = args.iter().map(|s| s.to_string()).collect();
    let (mut out, mut err) = (Vec::new(), Vec::new());
    let code = execute(&args, &mut out, &mut err);
    (
        code,
        String::from_utf8(out).unwrap(),
        String::from_utf8(err).unwrap(),
    )
}

fn hash_of(out: &str) -> String {
    out.lines()
        .find_map(|l| l.strip_prefix("hash: "))
        .expect("no hash line")
        .to_string()
}

const SCRIPT: &str = "right:90,up:45,none:20,left+down:30";

#[test]
fn validate_accepts_the_sample_rooms() {
    for name in ["cellar.room", "chase.room"] {
        let (code, out, _) = cli(&["validate", &room(name)]);
        assert_eq!(code, 0, "{name}: {out}");
        assert!(out.contains("ok (0 error(s)"), "{out}");
    }
}

#[test]
fn validate_fails_on_a_broken_room() {
    let text = std::fs::read_to_string(room("cellar.room")).unwrap();
    // Place an object that was never defined.
    let broken = text.replacen("Player", "Playr", 1);
    let path = tmp("broken.room");
    std::fs::write(&path, broken).unwrap();
    let (code, out, _) = cli(&["validate", &path]);
    // Either the parser or the validator must object; it must never pass.
    assert_ne!(code, 0, "{out}");
}

#[test]
fn missing_file_and_bad_usage_exit_2() {
    assert_eq!(cli(&["validate", "no/such/file.room"]).0, 2);
    assert_eq!(cli(&["frobnicate"]).0, 2);
    assert_eq!(cli(&[]).0, 2);
    assert_eq!(cli(&["run"]).0, 2);
    assert_eq!(cli(&["run", &room("cellar.room"), "--bogus"]).0, 2);
}

#[test]
fn run_is_repeatable() {
    let a = cli(&["run", &room("cellar.room"), "--input", SCRIPT]);
    let b = cli(&["run", &room("cellar.room"), "--input", SCRIPT]);
    assert_eq!(a.0, 0, "{}", a.2);
    assert!(a.1.contains("ticks: 185"), "{}", a.1);
    assert_eq!(hash_of(&a.1), hash_of(&b.1));
}

#[test]
fn run_hash_depends_on_input() {
    let a = cli(&["run", &room("cellar.room"), "--input", "right:60"]);
    let b = cli(&["run", &room("cellar.room"), "--input", "left:60"]);
    assert_ne!(hash_of(&a.1), hash_of(&b.1));
}

#[test]
fn ticks_option_pads_with_idle_ticks() {
    let (code, out, _) = cli(&[
        "run",
        &room("cellar.room"),
        "--input",
        "right:10",
        "--ticks",
        "50",
    ]);
    assert_eq!(code, 0);
    assert!(out.contains("ticks: 50"), "{out}");
}

#[test]
fn expect_hash_passes_and_fails() {
    let (_, out, _) = cli(&["run", &room("cellar.room"), "--input", SCRIPT]);
    let h = hash_of(&out);
    let ok = cli(&[
        "run",
        &room("cellar.room"),
        "--input",
        SCRIPT,
        "--expect-hash",
        &h,
    ]);
    assert_eq!(ok.0, 0, "{}", ok.1);
    let bad = cli(&[
        "run",
        &room("cellar.room"),
        "--input",
        SCRIPT,
        "--expect-hash",
        "1234",
    ]);
    assert_eq!(bad.0, 1, "{}", bad.1);
    assert!(bad.1.contains("MISMATCH"));
}

#[test]
fn recorded_replay_plays_back_to_a_match() {
    let file = tmp("session.replay");
    let (code, out, _) = cli(&[
        "run",
        &room("chase.room"),
        "--input",
        SCRIPT,
        "--seed",
        "7",
        "--record",
        &file,
    ]);
    assert_eq!(code, 0, "{out}");
    let (code, out, _) = cli(&["replay", &room("chase.room"), &file]);
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("MATCH"), "{out}");
}

#[test]
fn tampered_replay_is_reported_as_diverged() {
    let file = tmp("tampered.replay");
    cli(&[
        "run",
        &room("cellar.room"),
        "--input",
        SCRIPT,
        "--record",
        &file,
    ]);
    let mut replay = Replay::from_bytes(&std::fs::read(&file).unwrap()).unwrap();
    replay.final_hash = replay.final_hash.map(|h| h ^ 1);
    std::fs::write(&file, replay.to_bytes()).unwrap();
    let (code, out, _) = cli(&["replay", &room("cellar.room"), &file]);
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("DIVERGED"), "{out}");
}

#[test]
fn script_parser_accepts_and_rejects() {
    let s = parse_script("right:2, up+left:1 ,none:1").unwrap();
    assert_eq!(
        s,
        vec![
            MoveInput::RIGHT,
            MoveInput::RIGHT,
            MoveInput::UP.union(MoveInput::LEFT),
            MoveInput::NONE
        ]
    );
    assert!(parse_script("rihgt:2").is_err());
    assert!(parse_script("right").is_err());
    assert!(parse_script("right:x").is_err());
    assert!(parse_script("right:2000000").is_err());
}

#[test]
fn json_output_is_one_parseable_object_per_command() {
    let (code, out, _) = cli(&["run", &room("cellar.room"), "--input", "right:30", "--json"]);
    assert_eq!(code, 0);
    assert_eq!(out.lines().count(), 1, "{out}");
    assert!(out.starts_with("{\"ok\":true,\"room\":"), "{out}");
    assert!(
        out.contains("\"ticks\":30") && out.contains("\"hash\":\""),
        "{out}"
    );

    let (code, out, _) = cli(&["validate", &room("cellar.room"), "--json"]);
    assert_eq!(code, 0);
    assert!(
        out.contains("\"ok\":true") && out.contains("\"problems\":["),
        "{out}"
    );

    // Errors also come out as JSON on stdout.
    let (code, out, err) = cli(&["validate", "no/such\\file.room", "--json"]);
    assert_eq!(code, 2);
    assert!(out.starts_with("{\"ok\":false,\"error\":\""), "{out}");
    assert!(out.contains("\\\\"), "backslash must be escaped: {out}");
    assert!(err.is_empty());
}

#[test]
fn json_strings_are_escaped() {
    assert_eq!(jstr("a\"b\\c\nd"), "\"a\\\"b\\\\c\\nd\"");
}
