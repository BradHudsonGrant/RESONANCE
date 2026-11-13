use std::process::Command;

fn resonance(args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_resonance"))
        .args(args)
        .output()
        .expect("the resonance binary starts")
}

#[test]
fn key_prints_the_camelot_code_and_name_for_each_tag() {
    let output = resonance(&["key", "F#m", "8b", "Db maj"]);

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert_eq!(
        stdout.lines().collect::<Vec<_>>(),
        ["F#m -> 11A (F#m)", "8b -> 8B (C)", "Db maj -> 3B (Db)"]
    );
}

#[test]
fn key_fails_and_names_the_tag_it_could_not_read() {
    let output = resonance(&["key", "Am", "13A"]);

    assert!(!output.status.success());
    assert_eq!(
        String::from_utf8(output.stdout).unwrap().trim(),
        "Am -> 8A (Am)"
    );
    assert!(
        String::from_utf8(output.stderr)
            .expect("the resonance binary starts")
            .contains("\"13A\" is not a key")
    );
}
