//! Names that are not UTF-8 in the payloads of Cupola's own services.

#![cfg(unix)]

use std::{ffi::OsString, os::unix::ffi::OsStringExt as _};

use cydonia_cupola::{
    exec::Command,
    fs::{Entry, Expect, Hash, Kind, Request},
};
use serde::{Serialize, de::DeserializeOwned};

fn odd(prefix: &str) -> OsString {
    let mut bytes = prefix.as_bytes().to_vec();
    bytes.push(0xff);
    OsString::from_vec(bytes)
}

fn round_trip<T: Serialize + DeserializeOwned>(value: &T) -> T {
    postcard::from_bytes(&postcard::to_allocvec(value).unwrap()).unwrap()
}

#[test]
fn a_command_arrives_as_it_left() {
    let command = Command {
        program: odd("program-"),
        args: vec![odd("arg-"), "plain".into()],
        cwd: Some(odd("/cwd-").into()),
        env: vec![(odd("KEY_"), odd("value-"))],
    };
    assert_eq!(round_trip(&command), command);
}

#[test]
fn fs_paths_and_names_arrive_as_they_left() {
    let request = Request::Write {
        path: odd("/file-").into(),
        bytes: vec![1, 2],
        expect: Expect::Holds(Hash::of(&[1])),
    };
    assert_eq!(round_trip(&request), request);
    let entry = Entry {
        name: odd("name-"),
        kind: Kind::File,
    };
    assert_eq!(round_trip(&entry), entry);
}
