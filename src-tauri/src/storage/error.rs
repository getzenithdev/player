use std::fmt;
use std::path::PathBuf;

/// Result type used across the storage layer.
pub type StorageResult<T> = Result<T, StorageError>;

/// Errors produced by the local storage layer.
///
/// Messages are written in Spanish because they are surfaced directly in the
/// interface, where the roadmap asks the application to explain recoverable
/// losses to the user.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StorageError {
    /// A filesystem operation failed.
    Io { path: PathBuf, message: String },
    /// The document exists but cannot be parsed as a supported JSON document.
    InvalidJson { path: PathBuf, message: String },
    /// The document was written by a newer format version; it must not be
    /// overwritten by this build.
    NewerVersion { path: PathBuf, found: u32 },
    /// The requested path is not one of the known roadmap documents.
    UnknownDocument(String),
    /// The storage root could not be prepared for writing.
    NotWritable { path: PathBuf },
    /// An export or import payload is not acceptable.
    Import { message: String },
    /// A value failed validation before being persisted.
    InvalidValue { field: String, message: String },
}

impl fmt::Display for StorageError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io { path, message } => {
                write!(
                    formatter,
                    "No se pudo acceder a {}: {message}",
                    path.display()
                )
            }
            Self::InvalidJson { path, message } => write!(
                formatter,
                "El documento {} no es un JSON válido: {message}",
                path.display()
            ),
            Self::NewerVersion { path, found } => write!(
                formatter,
                "{} pertenece a la versión {found} del formato, más reciente que la soportada ({}). No se modifica.",
                path.display(),
                crate::storage::schema::SCHEMA_VERSION
            ),
            Self::UnknownDocument(path) => write!(
                formatter,
                "{path} no es un documento conocido de Zenith Player"
            ),
            Self::NotWritable { path } => write!(
                formatter,
                "No hay permiso de escritura en {}",
                path.display()
            ),
            Self::Import { message } => write!(formatter, "No se pudo importar: {message}"),
            Self::InvalidValue { field, message } => {
                write!(formatter, "Valor no válido en {field}: {message}")
            }
        }
    }
}

impl std::error::Error for StorageError {}
