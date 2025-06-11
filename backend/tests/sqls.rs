use std::{
    io::Write,
    process::{Command, Stdio},
};

fn run_on_database(input: &'static str) -> String {
    let mut cmd = Command::new("sqlite3")
        .arg("./data/database.db")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();

    cmd.stdin
        .take()
        .unwrap()
        .write_all(input.as_bytes())
        .unwrap();

    let output = cmd.wait_with_output().unwrap();
    assert!(output.status.success());

    String::from_utf8(output.stdout).unwrap()
}

#[test]
fn user_views() {
    insta::assert_snapshot!(run_on_database(include_str!(
        "./sqls/user_views.sql"
    )))
}
