use cydonia_cupola::shell::Script;

#[test]
fn plain_words_stay_bare_and_the_rest_are_single_quoted() {
    let mut script = Script::new();
    script
        .raw("cd &&")
        .word("/srv/app-1.2_x")
        .word("a b")
        .word("it's")
        .word("")
        .word("$HOME");
    assert_eq!(
        script.to_string_lossy(),
        r#"cd && /srv/app-1.2_x 'a b' 'it'\''s' '' '$HOME'"#
    );
}

#[cfg(unix)]
#[test]
fn a_name_that_is_not_utf8_keeps_its_bytes() {
    use std::{ffi::OsStr, os::unix::ffi::OsStrExt as _};

    let mut script = Script::new();
    script.word(OsStr::from_bytes(b"caf\xe9"));
    assert_eq!(script.as_bytes(), b"'caf\xe9'");
}

#[test]
fn an_appended_script_follows_after_a_space() {
    let mut inner = Script::new();
    inner.raw("uname -sm");
    let mut outer = Script::new();
    outer.raw("cd &&").append(&inner);
    assert_eq!(outer.to_string_lossy(), "cd && uname -sm");
}
