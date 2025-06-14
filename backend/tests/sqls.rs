use std::{
    io::Write,
    process::{Command, Stdio},
};

use backend::database::migration::Migration;

fn run_on_database(input: &'static str) -> String {
    let mut cmd = Command::new("sqlite3")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();

    let mut stdin = cmd.stdin.take().unwrap();
    let mut write_sqlite = move |input: &str| {
        stdin.write_all(input.replace("\t", "").as_bytes()).unwrap();
        stdin.write_all("\n".as_bytes()).unwrap();
    };

    for migration in Migration::all() {
        write_sqlite(migration.sql());
    }
    write_sqlite(input);
    write_sqlite(".quit");

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
