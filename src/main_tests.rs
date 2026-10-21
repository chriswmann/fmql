use super::without_sudo;
use std::ffi::OsString;

#[test]
fn without_sudo_keeps_the_subcommands_arguments() {
    let args = ["--sudo=doas", "--home=/x", "sql", "--sudo", "--home=/y"].map(OsString::from);
    assert_eq!(
        without_sudo(args.into_iter()),
        ["sql", "--sudo", "--home=/y"].map(OsString::from)
    );
}
