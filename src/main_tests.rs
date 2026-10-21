use super::{confirm, needs_confirmation, without_sudo};
use fmql::sql::parse_sql;
use std::ffi::OsString;

fn ask(input: &str, interactive: bool) -> (Result<bool, String>, String) {
    let mut output = Vec::new();
    let answer = confirm("Go?", &mut input.as_bytes(), interactive, &mut output)
        .map_err(|err| err.to_string());
    (answer, String::from_utf8(output).unwrap())
}

#[test]
fn confirm_accepts_only_yes() {
    for (input, expected) in [
        ("y\n", true),
        ("YES\n", true),
        (" yes \n", true),
        ("\n", false),
        ("n\n", false),
        ("yep\n", false),
        ("", false),
    ] {
        let (answer, prompt) = ask(input, true);
        assert_eq!(answer, Ok(expected), "{input:?}");
        assert_eq!(prompt, "Go? [y/N] ");
    }
}

#[test]
fn confirm_refuses_without_a_terminal() {
    let (answer, prompt) = ask("y\n", false);
    assert_eq!(
        answer,
        Err("Go? Pass --yes to confirm without a terminal".to_string())
    );
    assert_eq!(prompt, "");
}

#[test]
fn only_recursive_updates_as_root_need_confirmation() {
    let recursive = parse_sql("WITH RECURSIVE UPDATE /tmp SET owner = 'root'").unwrap();
    let shallow = parse_sql("UPDATE /tmp SET owner = 'root'").unwrap();
    let select = parse_sql("WITH RECURSIVE SELECT * FROM /tmp").unwrap();

    assert!(needs_confirmation(&recursive, false, true));
    assert!(!needs_confirmation(&recursive, true, true));
    assert!(!needs_confirmation(&recursive, false, false));
    assert!(!needs_confirmation(&shallow, false, true));
    assert!(!needs_confirmation(&select, false, true));
}

#[test]
fn without_sudo_keeps_the_subcommands_arguments() {
    let args = ["--sudo=doas", "--home=/x", "sql", "--sudo", "--home=/y"].map(OsString::from);
    assert_eq!(
        without_sudo(args.into_iter()),
        ["sql", "--sudo", "--home=/y"].map(OsString::from)
    );
}
