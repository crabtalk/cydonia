//! What an app says about itself, in a custom section of its `.wasm` that
//! Cydonia reads without running it. [`crate::app!`] writes it.

use serde::{Deserialize, Serialize};

/// The custom section's name.
pub const SECTION: &str = "cydonia.manifest";

/// The groups of calls an app may declare.
pub mod calls {
    /// [`crate::tables`].
    pub const TABLES: &str = "tables";
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Manifest {
    /// The app's crate name.
    pub id: String,
    /// What Cydonia lists the app as.
    pub name: String,
    /// The app's crate version.
    pub version: String,
    /// The [`crate::CONTRACT`] the app was built against.
    pub contract: u32,
    /// The groups of calls it makes, from [`calls`].
    #[serde(default)]
    pub calls: Vec<String>,
    /// The hosts it fetches from.
    #[serde(default)]
    pub hosts: Vec<String>,
}

impl Manifest {
    /// The manifest in `wasm`'s custom section. `None` for a module that has
    /// none.
    pub fn read(wasm: &[u8]) -> Result<Option<Self>, String> {
        let Some(mut rest) = wasm.strip_prefix(b"\0asm") else {
            return Err("not a wasm module".into());
        };
        rest = rest.get(4..).ok_or("a wasm module cut short")?;
        while let Some((&id, after)) = rest.split_first() {
            let (size, after) = leb(after)?;
            let body = after.get(..size).ok_or("a wasm section cut short")?;
            rest = &after[size..];
            if id != 0 {
                continue;
            }
            let (len, body) = leb(body)?;
            let name = body.get(..len).ok_or("a custom section's name cut short")?;
            if name == SECTION.as_bytes() {
                return serde_json::from_slice(&body[len..])
                    .map(Some)
                    .map_err(|error| format!("an unreadable manifest: {error}"));
            }
        }
        Ok(None)
    }

    /// Whether the app declared the group of calls `group`.
    pub fn calls(&self, group: &str) -> bool {
        self.calls.iter().any(|call| call == group)
    }
}

/// An unsigned LEB128 length and what follows it.
fn leb(bytes: &[u8]) -> Result<(usize, &[u8]), String> {
    let mut value = 0usize;
    for (ix, byte) in bytes.iter().enumerate().take(5) {
        value |= ((byte & 0x7f) as usize) << (7 * ix);
        if byte & 0x80 == 0 {
            return Ok((value, &bytes[ix + 1..]));
        }
    }
    Err("a malformed wasm length".into())
}

/// A manifest as [`crate::app!`] declares it, written out at compile time.
pub struct Declared<'a> {
    pub id: &'a str,
    pub name: &'a str,
    pub version: &'a str,
    pub calls: &'a [&'a str],
    pub hosts: &'a [&'a str],
}

impl Declared<'_> {
    /// How many bytes [`Self::bytes`] writes.
    pub const fn size(&self) -> usize {
        self.write(&mut [])
    }

    /// The manifest as JSON. `N` is [`Self::size`].
    pub const fn bytes<const N: usize>(&self) -> [u8; N] {
        let mut out = [0; N];
        self.write(&mut out);
        out
    }

    /// Write the JSON into `out`, or only count it when `out` is empty.
    const fn write(&self, out: &mut [u8]) -> usize {
        let mut at = 0;
        at = raw(out, at, "{\"id\":");
        at = string(out, at, self.id);
        at = raw(out, at, ",\"name\":");
        at = string(out, at, self.name);
        at = raw(out, at, ",\"version\":");
        at = string(out, at, self.version);
        at = raw(out, at, ",\"contract\":");
        at = number(out, at, crate::CONTRACT);
        at = raw(out, at, ",\"calls\":");
        at = strings(out, at, self.calls);
        at = raw(out, at, ",\"hosts\":");
        at = strings(out, at, self.hosts);
        raw(out, at, "}")
    }
}

const fn byte(out: &mut [u8], at: usize, byte: u8) -> usize {
    if !out.is_empty() {
        out[at] = byte;
    }
    at + 1
}

const fn raw(out: &mut [u8], mut at: usize, text: &str) -> usize {
    let bytes = text.as_bytes();
    let mut ix = 0;
    while ix < bytes.len() {
        at = byte(out, at, bytes[ix]);
        ix += 1;
    }
    at
}

const fn string(out: &mut [u8], mut at: usize, text: &str) -> usize {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let bytes = text.as_bytes();
    at = byte(out, at, b'"');
    let mut ix = 0;
    while ix < bytes.len() {
        let b = bytes[ix];
        if b == b'"' || b == b'\\' {
            at = byte(out, at, b'\\');
            at = byte(out, at, b);
        } else if b < 0x20 {
            at = raw(out, at, "\\u00");
            at = byte(out, at, HEX[(b >> 4) as usize]);
            at = byte(out, at, HEX[(b & 0xf) as usize]);
        } else {
            at = byte(out, at, b);
        }
        ix += 1;
    }
    byte(out, at, b'"')
}

const fn strings(out: &mut [u8], mut at: usize, texts: &[&str]) -> usize {
    at = byte(out, at, b'[');
    let mut ix = 0;
    while ix < texts.len() {
        if ix > 0 {
            at = byte(out, at, b',');
        }
        at = string(out, at, texts[ix]);
        ix += 1;
    }
    byte(out, at, b']')
}

const fn number(out: &mut [u8], mut at: usize, value: u32) -> usize {
    let mut digits = [0u8; 10];
    let (mut len, mut rest) = (0, value);
    loop {
        digits[len] = b'0' + (rest % 10) as u8;
        len += 1;
        rest /= 10;
        if rest == 0 {
            break;
        }
    }
    while len > 0 {
        len -= 1;
        at = byte(out, at, digits[len]);
    }
    at
}
