//! `#.ffini` — binary INI-like prefs for fuckFM.
//!
//! Layout (little-endian). Sections + typed keys, forward-compatible:
//! unknown sections/keys are ignored; unknown value types are skipped via
//! the length-prefixed payload.
//!
//! ```text
//! magic          : 6 bytes  b"FFINI\0"
//! version        : u16      = 1
//! section_count  : u16
//! sections[]:
//!   name_len     : u16
//!   name         : utf-8
//!   entry_count  : u16
//!   entries[]:
//!     key_len    : u16
//!     key        : utf-8
//!     type       : u8       (see Value)
//!     payload_len: u32
//!     payload    : type-specific bytes
//! ```
//!
//! Value types: Bool=1, I64=2, U64=3, F64=4, Str=5, Bytes=6.

use anyhow::{bail, Context, Result};
use std::collections::BTreeMap;
use std::fs;
use std::io::{Read, Write};
use std::path::Path;

pub const MAGIC: &[u8; 6] = b"FFINI\0";
pub const VERSION: u16 = 1;

const TY_BOOL: u8 = 1;
const TY_I64: u8 = 2;
const TY_U64: u8 = 3;
const TY_F64: u8 = 4;
const TY_STR: u8 = 5;
const TY_BYTES: u8 = 6;

#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Bool(bool),
    I64(i64),
    U64(u64),
    F64(f64),
    Str(String),
    Bytes(Vec<u8>),
}

impl Value {
    fn type_tag(&self) -> u8 {
        match self {
            Value::Bool(_) => TY_BOOL,
            Value::I64(_) => TY_I64,
            Value::U64(_) => TY_U64,
            Value::F64(_) => TY_F64,
            Value::Str(_) => TY_STR,
            Value::Bytes(_) => TY_BYTES,
        }
    }

    fn encode_payload(&self) -> Vec<u8> {
        match self {
            Value::Bool(v) => vec![u8::from(*v)],
            Value::I64(v) => v.to_le_bytes().to_vec(),
            Value::U64(v) => v.to_le_bytes().to_vec(),
            Value::F64(v) => v.to_le_bytes().to_vec(),
            Value::Str(v) => v.as_bytes().to_vec(),
            Value::Bytes(v) => v.clone(),
        }
    }

    fn decode(ty: u8, payload: &[u8]) -> Result<Self> {
        Ok(match ty {
            TY_BOOL => {
                if payload.len() != 1 {
                    bail!("bool payload must be 1 byte");
                }
                Value::Bool(payload[0] != 0)
            }
            TY_I64 => {
                if payload.len() != 8 {
                    bail!("i64 payload must be 8 bytes");
                }
                let mut buf = [0u8; 8];
                buf.copy_from_slice(payload);
                Value::I64(i64::from_le_bytes(buf))
            }
            TY_U64 => {
                if payload.len() != 8 {
                    bail!("u64 payload must be 8 bytes");
                }
                let mut buf = [0u8; 8];
                buf.copy_from_slice(payload);
                Value::U64(u64::from_le_bytes(buf))
            }
            TY_F64 => {
                if payload.len() != 8 {
                    bail!("f64 payload must be 8 bytes");
                }
                let mut buf = [0u8; 8];
                buf.copy_from_slice(payload);
                Value::F64(f64::from_le_bytes(buf))
            }
            TY_STR => Value::Str(String::from_utf8(payload.to_vec()).context("utf-8 string")?),
            TY_BYTES => Value::Bytes(payload.to_vec()),
            other => bail!("unknown ffini type tag {other}"),
        })
    }
}

/// Ordered section → key → value map (INI shape, binary storage).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Document {
    pub sections: BTreeMap<String, BTreeMap<String, Value>>,
}

impl Document {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set(&mut self, section: &str, key: &str, value: Value) {
        self.sections
            .entry(section.to_string())
            .or_default()
            .insert(key.to_string(), value);
    }

    pub fn get(&self, section: &str, key: &str) -> Option<&Value> {
        self.sections.get(section)?.get(key)
    }

    pub fn get_bool(&self, section: &str, key: &str) -> Option<bool> {
        match self.get(section, key)? {
            Value::Bool(v) => Some(*v),
            _ => None,
        }
    }

    pub fn get_u64(&self, section: &str, key: &str) -> Option<u64> {
        match self.get(section, key)? {
            Value::U64(v) => Some(*v),
            Value::I64(v) if *v >= 0 => Some(*v as u64),
            _ => None,
        }
    }

    pub fn get_str(&self, section: &str, key: &str) -> Option<&str> {
        match self.get(section, key)? {
            Value::Str(v) => Some(v.as_str()),
            _ => None,
        }
    }

    pub fn encode(&self) -> Result<Vec<u8>> {
        let mut out = Vec::new();
        out.write_all(MAGIC)?;
        out.write_all(&VERSION.to_le_bytes())?;
        let section_count = u16::try_from(self.sections.len()).context("too many sections")?;
        out.write_all(&section_count.to_le_bytes())?;

        for (name, entries) in &self.sections {
            write_str16(&mut out, name)?;
            let entry_count = u16::try_from(entries.len()).context("too many entries")?;
            out.write_all(&entry_count.to_le_bytes())?;
            for (key, value) in entries {
                write_str16(&mut out, key)?;
                out.write_all(&[value.type_tag()])?;
                let payload = value.encode_payload();
                let plen = u32::try_from(payload.len()).context("payload too large")?;
                out.write_all(&plen.to_le_bytes())?;
                out.write_all(&payload)?;
            }
        }
        Ok(out)
    }

    pub fn decode(mut bytes: &[u8]) -> Result<Self> {
        let mut magic = [0u8; 6];
        bytes
            .read_exact(&mut magic)
            .context("truncated ffini header")?;
        if &magic != MAGIC {
            bail!("not an ffini file (bad magic)");
        }
        let version = read_u16(&mut bytes)?;
        if version == 0 || version > VERSION {
            bail!("unsupported ffini version {version}");
        }
        let section_count = read_u16(&mut bytes)? as usize;
        let mut doc = Document::new();
        for _ in 0..section_count {
            let name = read_str16(&mut bytes)?;
            let entry_count = read_u16(&mut bytes)? as usize;
            let mut map = BTreeMap::new();
            for _ in 0..entry_count {
                let key = read_str16(&mut bytes)?;
                let mut ty = [0u8; 1];
                bytes.read_exact(&mut ty).context("truncated type")?;
                let plen = read_u32(&mut bytes)? as usize;
                let mut payload = vec![0u8; plen];
                bytes
                    .read_exact(&mut payload)
                    .context("truncated payload")?;
                match Value::decode(ty[0], &payload) {
                    Ok(value) => {
                        map.insert(key, value);
                    }
                    Err(_) => {
                        // Skip unknown / corrupt typed values; keep file loadable.
                    }
                }
            }
            doc.sections.insert(name, map);
        }
        Ok(doc)
    }

    pub fn write_file(&self, path: &Path) -> Result<()> {
        let data = self.encode()?;
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).with_context(|| format!("mkdir {}", parent.display()))?;
        }
        let tmp = path.with_extension("ffini.tmp");
        fs::write(&tmp, &data).with_context(|| format!("write {}", tmp.display()))?;
        fs::rename(&tmp, path).with_context(|| format!("rename to {}", path.display()))?;
        Ok(())
    }

    pub fn read_file(path: &Path) -> Result<Self> {
        let data = fs::read(path).with_context(|| format!("read {}", path.display()))?;
        Self::decode(&data)
    }
}

fn write_str16(out: &mut Vec<u8>, s: &str) -> Result<()> {
    let bytes = s.as_bytes();
    let len = u16::try_from(bytes.len()).context("string too long for u16")?;
    out.write_all(&len.to_le_bytes())?;
    out.write_all(bytes)?;
    Ok(())
}

fn read_u16(r: &mut &[u8]) -> Result<u16> {
    let mut buf = [0u8; 2];
    r.read_exact(&mut buf).context("truncated u16")?;
    Ok(u16::from_le_bytes(buf))
}

fn read_u32(r: &mut &[u8]) -> Result<u32> {
    let mut buf = [0u8; 4];
    r.read_exact(&mut buf).context("truncated u32")?;
    Ok(u32::from_le_bytes(buf))
}

fn read_str16(r: &mut &[u8]) -> Result<String> {
    let len = read_u16(r)? as usize;
    let mut buf = vec![0u8; len];
    r.read_exact(&mut buf).context("truncated string")?;
    String::from_utf8(buf).context("utf-8 name/key")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_typed_ini() {
        let mut doc = Document::new();
        doc.set("appearance", "theme", Value::Str("win98".into()));
        doc.set("appearance", "icon_scale", Value::U64(3));
        doc.set("appearance", "classic_window_chrome", Value::Bool(false));
        doc.set("defaults", "show_hidden", Value::Bool(true));
        doc.set("meta", "pi", Value::F64(3.5));
        doc.set("meta", "blob", Value::Bytes(vec![1, 2, 3]));

        let bytes = doc.encode().unwrap();
        let back = Document::decode(&bytes).unwrap();
        assert_eq!(back.get_str("appearance", "theme"), Some("win98"));
        assert_eq!(back.get_u64("appearance", "icon_scale"), Some(3));
        assert_eq!(
            back.get_bool("appearance", "classic_window_chrome"),
            Some(false)
        );
        assert_eq!(back.get_bool("defaults", "show_hidden"), Some(true));
        assert_eq!(back.get("meta", "pi"), Some(&Value::F64(3.5)));
        assert_eq!(back.get("meta", "blob"), Some(&Value::Bytes(vec![1, 2, 3])));
    }
}
