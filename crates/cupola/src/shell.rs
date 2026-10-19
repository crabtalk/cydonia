//! Command lines a POSIX shell reads back as the words they were built from.

use std::ffi::{OsStr, OsString};

use crate::bytes::{Names as _, Platform};

/// The bytes besides ASCII alphanumerics a word holds and stays unquoted.
const PLAIN: &[u8] = b"/._-+,:@%=";

/// A command line for a POSIX shell, built word by word, words separated by
/// one space.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Script(Vec<u8>);

impl Script {
    pub fn new() -> Self {
        Self::default()
    }

    /// Appends `word` as one word: as it is when it is not empty and every
    /// byte is ASCII alphanumeric or one of `/._-+,:@%=`, single-quoted
    /// otherwise.
    pub fn word(&mut self, word: impl AsRef<OsStr>) -> &mut Self {
        let word = Platform::bytes(word.as_ref());
        self.space();
        let plain = !word.is_empty()
            && word
                .iter()
                .all(|byte| byte.is_ascii_alphanumeric() || PLAIN.contains(byte));
        if plain {
            self.0.extend_from_slice(&word);
        } else {
            self.0.push(b'\'');
            for &byte in word.iter() {
                if byte == b'\'' {
                    self.0.extend_from_slice(b"'\\''");
                } else {
                    self.0.push(byte);
                }
            }
            self.0.push(b'\'');
        }
        self
    }

    pub fn words<I>(&mut self, words: I) -> &mut Self
    where
        I: IntoIterator,
        I::Item: AsRef<OsStr>,
    {
        for word in words {
            self.word(word);
        }
        self
    }

    /// Appends `syntax` unquoted, for the shell to parse.
    pub fn raw(&mut self, syntax: &str) -> &mut Self {
        self.space();
        self.0.extend_from_slice(syntax.as_bytes());
        self
    }

    /// Appends `script` as it is.
    pub fn append(&mut self, script: &Script) -> &mut Self {
        if !script.is_empty() {
            self.space();
            self.0.extend_from_slice(&script.0);
        }
        self
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }

    pub fn to_os_string(&self) -> OsString {
        Platform::os_string(self.0.clone())
    }

    pub fn to_string_lossy(&self) -> String {
        String::from_utf8_lossy(&self.0).into_owned()
    }

    fn space(&mut self) {
        if !self.0.is_empty() {
            self.0.push(b' ');
        }
    }
}
