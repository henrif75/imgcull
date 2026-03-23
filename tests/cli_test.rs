use std::process::Command;

fn imgcull_cmd() -> Command {
    Command::new(env!("CARGO_BIN_EXE_imgcull"))
}

#[test]
fn score_subcommand_with_paths_is_recognized() {
    let output = imgcull_cmd()
        .args(["score", "photo.jpg"])
        .output()
        .expect("failed to run imgcull");
    assert!(output.status.success(), "score subcommand should succeed");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("photo.jpg"),
        "should echo the path: {stdout}"
    );
}

#[test]
fn describe_subcommand_with_paths_is_recognized() {
    let output = imgcull_cmd()
        .args(["describe", "photo.jpg"])
        .output()
        .expect("failed to run imgcull");
    assert!(
        output.status.success(),
        "describe subcommand should succeed"
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("photo.jpg"),
        "should echo the path: {stdout}"
    );
}

#[test]
fn init_subcommand_is_recognized() {
    let output = imgcull_cmd()
        .arg("init")
        .output()
        .expect("failed to run imgcull");
    assert!(output.status.success(), "init subcommand should succeed");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("Init"),
        "should print init message: {stdout}"
    );
}

#[test]
fn help_shows_all_subcommands() {
    let output = imgcull_cmd()
        .arg("--help")
        .output()
        .expect("failed to run imgcull");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("score"), "help should mention score");
    assert!(stdout.contains("describe"), "help should mention describe");
    assert!(stdout.contains("init"), "help should mention init");
}
