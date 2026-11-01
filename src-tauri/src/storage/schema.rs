//! Versioned document format shared by every stored JSON file.
//!
//! Each file is an envelope with a format version, a modification timestamp
//! and the payload. The version guard is what allows a build to refuse data it
//! does not understand instead of silently overwriting it.

use std::path::Path;

use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::Value;

use super::error::{StorageError, StorageResult};

/// Format version written by this build.
pub const SCHEMA_VERSION: u32 = 2;

/// Stored envelope: `{"version": n, "updated_at": n, "data": ...}`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredDocument<T> {
    pub version: u32,
    #[serde(default)]
    pub updated_at: u64,
    pub data: T,
}

/// Envelope with its payload still as raw JSON, used while validating files.
#[derive(Debug, Clone)]
pub struct DecodedDocument {
    /// Version declared by the file, before any migration.
    pub version: u32,
    pub updated_at: u64,
    pub data: Value,
}

/// Payload of `config/appearance.json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppearanceData {
    #[serde(default = "default_theme")]
    pub theme: String,
}

impl Default for AppearanceData {
    fn default() -> Self {
        Self {
            theme: default_theme(),
        }
    }
}

fn default_theme() -> String {
    "light".to_string()
}

/// Themes accepted by the interface.
pub const THEMES: [&str; 3] = ["light", "dark", "system"];

pub fn is_valid_theme(theme: &str) -> bool {
    THEMES.contains(&theme)
}

/// Encodes a payload as a pretty-printed envelope with the current version.
pub fn document_bytes<T: Serialize>(data: &T, updated_at: u64) -> StorageResult<Vec<u8>> {
    let envelope = StoredDocument {
        version: SCHEMA_VERSION,
        updated_at,
        data,
    };
    serde_json::to_vec_pretty(&envelope).map_err(|error| StorageError::InvalidValue {
        field: "documento".to_string(),
        message: error.to_string(),
    })
}

/// Parses and validates an envelope from its textual representation.
pub fn decode_document(path: &Path, raw: &str) -> StorageResult<DecodedDocument> {
    let envelope: StoredDocument<Value> =
        serde_json::from_str(raw).map_err(|error| StorageError::InvalidJson {
            path: path.to_path_buf(),
            message: error.to_string(),
        })?;
    decode_envelope(path, envelope)
}

/// Validates an envelope whose payload is still raw JSON.
fn decode_envelope(path: &Path, envelope: StoredDocument<Value>) -> StorageResult<DecodedDocument> {
    if envelope.version > SCHEMA_VERSION {
        return Err(StorageError::NewerVersion {
            path: path.to_path_buf(),
            found: envelope.version,
        });
    }
    let data = migrate(envelope.version, envelope.data, path)?;
    Ok(DecodedDocument {
        version: envelope.version,
        updated_at: envelope.updated_at,
        data,
    })
}

/// Validates an envelope held as raw JSON, without a first deserialization
/// pass into a concrete document type.
pub fn decode_envelope_value(path: &Path, value: Value) -> StorageResult<DecodedDocument> {
    let envelope: StoredDocument<Value> =
        serde_json::from_value(value).map_err(|error| StorageError::InvalidJson {
            path: path.to_path_buf(),
            message: error.to_string(),
        })?;
    decode_envelope(path, envelope)
}

/// Brings a payload from `from` up to [`SCHEMA_VERSION`].
///
/// Version 2 protects universal metadata from older binaries that would
/// discard unknown track fields. Version 1 payloads are preserved verbatim;
/// metadata preparation upgrades legacy track identities in one atomic write.
fn migrate(from: u32, data: Value, path: &Path) -> StorageResult<Value> {
    if from == 1 || from == SCHEMA_VERSION {
        return Ok(data);
    }
    Err(StorageError::InvalidValue {
        field: "version".to_string(),
        message: format!(
            "{} usa la versión {from} y falta la migración a la versión {SCHEMA_VERSION}",
            path.display()
        ),
    })
}

/// Deserializes the payload of a decoded document into a concrete type.
pub fn payload<T: DeserializeOwned>(decoded: &DecodedDocument, path: &Path) -> StorageResult<T> {
    serde_json::from_value(decoded.data.clone()).map_err(|error| StorageError::InvalidJson {
        path: path.to_path_buf(),
        message: error.to_string(),
    })
}
