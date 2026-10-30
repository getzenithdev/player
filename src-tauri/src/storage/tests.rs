//! Unit tests for the storage layer.
//!
//! They reproduce the failures listed in the roadmap acceptance criteria:
//! damaged JSON, interrupted writes, a location that cannot be written, and
//! documents written by a newer format version. Every case must keep the last
//! valid copy intact.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use serde_json::{Value, json};

use super::*;

static NEXT_ROOT: AtomicUsize = AtomicUsize::new(0);

/// Temporary root that removes itself when the test finishes.
struct TestRoot {
    path: PathBuf,
}

impl TestRoot {
    fn new(label: &str) -> Self {
        let unique = NEXT_ROOT.fetch_add(1, Ordering::SeqCst);
        let path = std::env::temp_dir()
            .join("zenith-player-storage-tests")
            .join(format!("{label}-{}-{unique}", std::process::id()));
        let _ = fs::remove_dir_all(&path);
        Self { path }
    }

    fn path(&self) -> &Path {
        &self.path
    }

    fn manager(&self) -> StorageManager {
        StorageManager::open(self.path.clone(), false).expect("el almacenamiento debe abrir")
    }
}

impl Drop for TestRoot {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

fn log_names(root: &Path) -> Vec<String> {
    fs::read_dir(root.join(LOG_DIR))
        .map(|entries| {
            entries
                .flatten()
                .map(|entry| entry.file_name().to_string_lossy().to_string())
                .collect()
        })
        .unwrap_or_default()
}

#[test]
fn creates_the_logical_layout() {
    let root = TestRoot::new("layout");
    let _manager = root.manager();
    for relative in SUBDIRS {
        assert!(root.path().join(relative).is_dir(), "falta {relative}");
    }
}

#[test]
fn default_root_follows_the_platform() {
    let root = default_root();
    assert!(root.ends_with(APP_DIR_NAME));
    #[cfg(windows)]
    {
        let appdata = std::env::var("APPDATA").expect("APPDATA debe estar definido");
        assert!(
            root.starts_with(appdata),
            "la raíz por defecto vive en APPDATA"
        );
    }
}

#[test]
fn portable_mode_is_an_explicit_choice() {
    let executable_directory = Path::new("/opt/zenith");
    assert_eq!(
        root_for(executable_directory, true),
        executable_directory.join(APP_DIR_NAME)
    );
    assert_eq!(root_for(executable_directory, false), default_root());
}

#[test]
fn documents_survive_reopening_the_manager() {
    let root = TestRoot::new("roundtrip");
    {
        let manager = root.manager();
        manager
            .save_document("config/settings.json", &json!({ "language": "es" }))
            .unwrap();
        manager.save_appearance("dark").unwrap();
    }
    let reopened = root.manager();
    let settings: StoredDocument<Value> = reopened
        .load_document("config/settings.json")
        .unwrap()
        .expect("el documento debe volver a cargarse");
    assert_eq!(settings.data["language"], "es");
    assert_eq!(reopened.load_appearance().unwrap().theme, "dark");
}

#[test]
fn missing_documents_report_defaults() {
    let root = TestRoot::new("missing");
    let manager = root.manager();
    assert!(
        manager
            .load_document::<Value>("config/settings.json")
            .unwrap()
            .is_none()
    );
    assert_eq!(manager.load_appearance().unwrap().theme, "light");
    let status = manager.status();
    assert_eq!(
        status.documents.len(),
        CONFIG_FILES.len() + LIBRARY_FILES.len()
    );
    assert!(
        status
            .documents
            .iter()
            .all(|document| !document.exists && !document.valid)
    );
    assert!(!status.directories_ready || status.writable);
}

#[test]
fn saving_replaces_the_previous_content() {
    let root = TestRoot::new("replace");
    let manager = root.manager();
    manager.save_appearance("light").unwrap();
    manager.save_appearance("dark").unwrap();
    assert_eq!(manager.load_appearance().unwrap().theme, "dark");
    assert!(
        !root.path().join("config/.appearance.json.tmp").exists(),
        "la escritura atómica no deja temporales"
    );
}

#[test]
fn failed_write_keeps_the_previous_copy() {
    let root = TestRoot::new("failed-write");
    let manager = root.manager();
    let path = manager.document_path("config/settings.json");
    fs::create_dir_all(&path).unwrap();
    let error = manager
        .save_document("config/settings.json", &json!({ "a": 1 }))
        .unwrap_err();
    assert!(matches!(error, StorageError::Io { .. }), "{error}");
    assert!(path.is_dir(), "el destino no debe modificarse");
    assert!(!root.path().join("config/.settings.json.tmp").exists());
}

#[test]
fn write_atomic_cleans_up_when_replacement_fails() {
    let root = TestRoot::new("atomic");
    let target = root.path().join("file.json");
    fs::create_dir_all(&target).unwrap();
    let error = write_atomic(&target, b"{}").unwrap_err();
    assert!(matches!(error, StorageError::Io { .. }));
    assert!(target.is_dir());
    assert!(!root.path().join(".file.json.tmp").exists());
}

#[test]
fn interrupted_write_is_ignored() {
    let root = TestRoot::new("stale-tmp");
    let manager = root.manager();
    manager.save_appearance("dark").unwrap();
    let temporary = root.path().join("config/.appearance.json.tmp");
    fs::write(&temporary, b"{ interrumpido").unwrap();
    assert_eq!(manager.load_appearance().unwrap().theme, "dark");
    manager.save_appearance("light").unwrap();
    assert!(!temporary.exists());
    assert_eq!(manager.load_appearance().unwrap().theme, "light");
}

#[test]
fn damaged_document_recovers_from_backup() {
    let root = TestRoot::new("recover");
    let manager = root.manager();
    manager.save_appearance("light").unwrap();
    manager.save_appearance("dark").unwrap();
    let path = manager.document_path(APPEARANCE_DOCUMENT);
    fs::write(&path, "{ dañado").unwrap();

    let recovered = manager
        .load_document::<AppearanceData>(APPEARANCE_DOCUMENT)
        .unwrap()
        .expect("debe recuperarse desde el respaldo");
    assert_eq!(recovered.data.theme, "light");

    let raw = fs::read_to_string(&path).unwrap();
    assert!(
        serde_json::from_str::<Value>(&raw).is_ok(),
        "el archivo principal vuelve a ser JSON válido"
    );
    assert!(
        log_names(root.path())
            .iter()
            .any(|name| name.contains("corrupt")),
        "la copia dañada se conserva en logs"
    );
    assert!(
        root.path().join(LOG_DIR).join("storage.log").exists(),
        "el evento queda registrado en el log"
    );
    assert!(
        manager
            .status()
            .recovery
            .iter()
            .any(|notice| notice.kind == "recovered")
    );
}

#[test]
fn damaged_document_without_backup_is_preserved() {
    let root = TestRoot::new("no-backup");
    let manager = root.manager();
    manager.save_appearance("dark").unwrap();
    let path = manager.document_path(APPEARANCE_DOCUMENT);
    fs::write(&path, b"no es json").unwrap();

    let error = manager
        .load_document::<AppearanceData>(APPEARANCE_DOCUMENT)
        .unwrap_err();
    assert!(matches!(error, StorageError::InvalidJson { .. }));
    assert!(!path.exists(), "el archivo dañado se movió a logs");
    assert!(
        log_names(root.path())
            .iter()
            .any(|name| name.contains("corrupt"))
    );
    assert!(
        manager
            .status()
            .recovery
            .iter()
            .any(|notice| notice.kind == "preserved")
    );
}

#[test]
fn newer_format_version_is_never_overwritten() {
    let root = TestRoot::new("newer");
    let manager = root.manager();
    let path = manager.document_path(APPEARANCE_DOCUMENT);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    let future = json!({ "version": 99, "updated_at": 1, "data": { "theme": "dark" } });
    fs::write(&path, serde_json::to_vec_pretty(&future).unwrap()).unwrap();
    let before = fs::read(&path).unwrap();

    let load_error = manager
        .load_document::<AppearanceData>(APPEARANCE_DOCUMENT)
        .unwrap_err();
    assert!(matches!(
        load_error,
        StorageError::NewerVersion { found: 99, .. }
    ));
    let save_error = manager.save_appearance("light").unwrap_err();
    assert!(matches!(save_error, StorageError::NewerVersion { .. }));
    assert_eq!(fs::read(&path).unwrap(), before, "el archivo no cambia");
    assert!(manager.status().recovery.is_empty());
}

#[test]
fn backups_are_pruned_to_the_limit() {
    let root = TestRoot::new("prune");
    let manager = root.manager();
    for index in 0..8 {
        manager
            .save_appearance(if index % 2 == 0 { "light" } else { "dark" })
            .unwrap();
    }
    let status = manager.status();
    let appearance = status
        .documents
        .iter()
        .find(|document| document.path == APPEARANCE_DOCUMENT)
        .unwrap();
    assert!(appearance.exists && appearance.valid);
    assert_eq!(
        appearance.backups, MAX_BACKUPS_PER_DOCUMENT,
        "solo se conservan los respaldos más recientes"
    );
}

#[test]
fn manual_backup_covers_every_stored_document() {
    let root = TestRoot::new("backup-all");
    let manager = root.manager();
    manager.save_appearance("dark").unwrap();
    manager
        .save_document(TRACKS_DOCUMENT, &json!({ "tracks": [] }))
        .unwrap();

    let report = manager.create_backup().unwrap();
    assert_eq!(report.created.len(), 2);
    assert_eq!(
        report.skipped,
        CONFIG_FILES.len() + LIBRARY_FILES.len() - report.created.len()
    );
    assert!(Path::new(&report.directory).is_dir());
    assert_eq!(
        manager
            .status()
            .documents
            .iter()
            .filter(|d| d.backups > 0)
            .count(),
        2
    );
}

#[test]
fn export_excludes_provider_credentials() {
    let root = TestRoot::new("export");
    let manager = root.manager();
    manager.save_appearance("dark").unwrap();
    manager
        .save_document(PROTECTED_DOCUMENT, &json!({ "token": "secreto" }))
        .unwrap();

    let report = manager.export_bundle().unwrap();
    assert_eq!(report.excluded, vec![PROTECTED_DOCUMENT.to_string()]);
    assert_eq!(report.config, 1);
    assert_eq!(report.library, 0);
    assert!(report.skipped.is_empty());
    assert!(Path::new(&report.path).is_file());

    let raw = fs::read_to_string(&report.path).unwrap();
    assert!(!raw.contains("secreto"), "las credenciales no se exportan");
    let bundle: Value = serde_json::from_str(&raw).unwrap();
    assert_eq!(bundle["includes_secrets"], false);
    assert_eq!(bundle["format"], EXPORT_FORMAT);
    assert!(bundle["config"].get("config/appearance.json").is_some());
    assert!(bundle["config"].get(PROTECTED_DOCUMENT).is_none());
}

#[test]
fn import_restores_documents_and_rebuilds_indexes() {
    let source = TestRoot::new("import-source");
    let exporter = source.manager();
    exporter.save_appearance("dark").unwrap();
    exporter
        .save_document(
            TRACKS_DOCUMENT,
            &json!({ "tracks": [{ "id": "t1", "title": "Canción" }] }),
        )
        .unwrap();
    exporter
        .save_document(PROTECTED_DOCUMENT, &json!({ "token": "origen" }))
        .unwrap();
    let exported = exporter.export_bundle().unwrap();

    let target = TestRoot::new("import-target");
    let importer = target.manager();
    importer
        .save_document(PROTECTED_DOCUMENT, &json!({ "token": "propio" }))
        .unwrap();

    let report = importer
        .import_bundle(Some(Path::new(&exported.path)))
        .unwrap();
    assert_eq!(report.config, 1);
    assert_eq!(report.library, 1);
    assert_eq!(importer.load_appearance().unwrap().theme, "dark");

    let providers = fs::read_to_string(target.path().join(PROTECTED_DOCUMENT)).unwrap();
    assert!(
        providers.contains("propio"),
        "las credenciales locales se conservan"
    );
    assert!(!providers.contains("origen"));

    let status = importer.status();
    assert_eq!(
        status.index.tracks, 1,
        "los índices se reconstruyen al importar"
    );
    assert_eq!(importer.search("canción").len(), 1);
}

#[test]
fn import_rejects_unknown_bundles() {
    let root = TestRoot::new("import-invalid");
    let manager = root.manager();
    manager.save_appearance("dark").unwrap();
    let directory = root.path().join(BACKUP_DIR);
    let bundle = |format: &str, version: u32| {
        json!({
            "format": format,
            "format_version": version,
            "schema_version": SCHEMA_VERSION,
            "exported_at": 0,
            "includes_secrets": false,
            "excluded": [],
            "config": {},
            "library": {},
            "lyrics": []
        })
    };

    let foreign = directory.join("export@1.json");
    fs::write(
        &foreign,
        serde_json::to_vec_pretty(&bundle("otro-formato", 1)).unwrap(),
    )
    .unwrap();
    assert!(matches!(
        manager.import_bundle(Some(&foreign)).unwrap_err(),
        StorageError::Import { .. }
    ));

    let future = directory.join("export@2.json");
    fs::write(
        &future,
        serde_json::to_vec_pretty(&bundle(EXPORT_FORMAT, EXPORT_FORMAT_VERSION + 1)).unwrap(),
    )
    .unwrap();
    assert!(matches!(
        manager.import_bundle(Some(&future)).unwrap_err(),
        StorageError::Import { .. }
    ));

    let broken = directory.join("export@3.json");
    fs::write(&broken, b"{ roto").unwrap();
    assert!(matches!(
        manager.import_bundle(Some(&broken)).unwrap_err(),
        StorageError::Import { .. }
    ));

    assert_eq!(
        manager.load_appearance().unwrap().theme,
        "dark",
        "una importación rechazada no modifica nada"
    );
}

#[test]
fn import_skips_unknown_paths() {
    let root = TestRoot::new("import-paths");
    let manager = root.manager();
    let bundle = json!({
        "format": EXPORT_FORMAT,
        "format_version": EXPORT_FORMAT_VERSION,
        "schema_version": SCHEMA_VERSION,
        "exported_at": 0,
        "includes_secrets": false,
        "excluded": [],
        "config": {
            "config/providers.json": { "openai": { "api_key": "robada" } },
            "config/../../../fuera.json": { "version": 1, "updated_at": 0, "data": { "theme": "dark" } }
        },
        "library": {},
        "lyrics": [
            { "file": "../fuera.lrc", "content": "no debe salir" },
            { "file": "validez.lrc", "content": "[00:01.00] línea" }
        ]
    });
    let path = root.path().join(BACKUP_DIR).join("export@9.json");
    fs::write(&path, serde_json::to_vec_pretty(&bundle).unwrap()).unwrap();

    let report = manager.import_bundle(Some(&path)).unwrap();
    assert!(
        report
            .skipped
            .iter()
            .any(|entry| entry.contains("credenciales excluidas"))
    );
    assert!(
        report
            .skipped
            .iter()
            .any(|entry| entry.contains("ruta desconocida"))
    );
    assert!(
        report
            .skipped
            .iter()
            .any(|entry| entry.contains("no válida"))
    );
    assert_eq!(report.lyrics, 1);
    assert!(root.path().join(LYRICS_DIR).join("validez.lrc").is_file());
    assert!(!root.path().join("fuera.json").exists());
    assert!(
        !root.path().join(PROTECTED_DOCUMENT).exists(),
        "un bundle externo no puede inyectar credenciales"
    );
}

#[test]
fn clear_cache_keeps_library_and_documents() {
    let root = TestRoot::new("cache");
    let manager = root.manager();
    manager
        .save_document(
            TRACKS_DOCUMENT,
            &json!({ "tracks": [{ "id": "t1", "title": "X" }] }),
        )
        .unwrap();
    manager
        .save_document(PROTECTED_DOCUMENT, &json!({ "token": "secreto" }))
        .unwrap();
    fs::write(root.path().join("cache/images/portada.png"), b"imagen").unwrap();
    fs::write(root.path().join("cache/temporary/parcial.tmp"), b"tmp").unwrap();
    fs::write(root.path().join("cache/audio/pista.mp3"), b"audio").unwrap();

    let report = manager.clear_cache().unwrap();
    assert!(report.errors.is_empty(), "{:?}", report.errors);
    assert_eq!(report.removed_files, 3);
    assert!(
        report.freed_bytes >= 13,
        "imagen(5) + tmp(3) + audio(5) = 13 bytes"
    );
    assert!(
        root.path().join("cache/images").is_dir(),
        "los directorios de caché siguen disponibles"
    );

    let tracks = manager
        .load_document::<TracksDocument>(TRACKS_DOCUMENT)
        .unwrap()
        .expect("la biblioteca no se toca");
    assert_eq!(tracks.data.tracks.len(), 1);
    let providers = fs::read_to_string(root.path().join(PROTECTED_DOCUMENT)).unwrap();
    assert!(providers.contains("secreto"));
    assert_eq!(manager.status().cache_bytes, 0);
}

#[test]
fn indexes_cover_every_field() {
    let root = TestRoot::new("indexes");
    let manager = root.manager();
    manager
        .save_document(
            TRACKS_DOCUMENT,
            &json!({ "tracks": [
                {
                    "id": "t1",
                    "title": "Night Drive",
                    "artist": "Aurora Lane",
                    "album": "Neon Roads",
                    "genre": "Synthwave",
                    "year": 1985,
                    "duration": 245,
                    "local_path": "/music/night.flac"
                },
                {
                    "id": "t2",
                    "title": "Solar Winds",
                    "artist": "Aurora Lane",
                    "album": "Orbits",
                    "year": 1990,
                    "duration": 180
                }
            ] }),
        )
        .unwrap();

    let reopened = root.manager();
    let status = reopened.status();
    assert_eq!(status.index.tracks, 2);
    assert_eq!(
        status.index.fields,
        INDEX_FIELDS
            .iter()
            .map(|field| field.to_string())
            .collect::<Vec<String>>()
    );
    assert_eq!(reopened.search("night").len(), 1, "título");
    assert_eq!(
        reopened.search("AURORA").len(),
        2,
        "artista sin distinguir mayúsculas"
    );
    assert_eq!(reopened.search("neon").len(), 1, "álbum");
    assert_eq!(reopened.search("synth").len(), 1, "género");
    assert_eq!(reopened.search("1985").len(), 1, "año");
    assert_eq!(reopened.search("245").len(), 1, "duración");
    assert_eq!(reopened.search("night.flac").len(), 1, "ruta");
    assert_eq!(
        reopened.search("").len(),
        2,
        "consulta vacía: toda la biblioteca"
    );
    assert!(reopened.search("inexistente").is_empty());
}

#[test]
fn incomplete_tracks_are_indexed() {
    let root = TestRoot::new("incomplete");
    let manager = root.manager();
    manager
        .save_document(
            TRACKS_DOCUMENT,
            &json!({ "tracks": [{ "id": "t1" }, { "title": "Sin identificador" }] }),
        )
        .unwrap();

    let reopened = root.manager();
    assert_eq!(reopened.status().index.tracks, 2);
    assert_eq!(reopened.search("sin identificador").len(), 1);
}

#[test]
fn unknown_documents_are_rejected() {
    let root = TestRoot::new("unknown");
    let manager = root.manager();
    let error = manager
        .save_document("config/../../fuera.json", &json!({}))
        .unwrap_err();
    assert!(matches!(error, StorageError::UnknownDocument(_)));
    let error = manager
        .load_document::<Value>("library/../../fuera.json")
        .unwrap_err();
    assert!(matches!(error, StorageError::UnknownDocument(_)));
    assert!(!root.path().join("fuera.json").exists());
}

#[test]
fn theme_validation_rejects_unknown_values() {
    let root = TestRoot::new("theme");
    let manager = root.manager();
    let error = manager.save_appearance("neon").unwrap_err();
    assert!(matches!(error, StorageError::InvalidValue { .. }));
    assert!(
        !manager.document_path(APPEARANCE_DOCUMENT).exists(),
        "una preferencia inválida no se guarda"
    );
    manager.save_appearance("system").unwrap();
    assert_eq!(manager.load_appearance().unwrap().theme, "system");
}

#[test]
fn degraded_storage_keeps_the_application_running() {
    let root = TestRoot::new("degraded");
    fs::write(root.path(), b"ocupado").unwrap();
    let error = StorageManager::open(root.path().to_path_buf(), false).unwrap_err();
    let manager = StorageManager::unavailable(root.path().to_path_buf(), false, error);

    let status = manager.status();
    assert!(!status.writable);
    assert!(status.error.is_some());
    assert!(!status.directories_ready);
    assert!(status.documents.iter().all(|document| !document.exists));
    assert!(manager.search("x").is_empty());
    assert!(manager.save_appearance("dark").is_err());
    assert!(manager.create_backup().is_err());
    assert!(manager.export_bundle().is_err());
    assert!(manager.clear_cache().is_err());
    if let Ok(appearance) = manager.load_appearance() {
        assert_eq!(appearance.theme, "light");
    }
}

#[test]
fn concurrent_writes_stay_consistent() {
    let root = TestRoot::new("concurrent");
    let manager = Arc::new(root.manager());
    let documents = [
        "library/tracks.json",
        "library/albums.json",
        "library/artists.json",
        "library/playlists.json",
        "library/favorites.json",
        "library/history.json",
    ];
    let handles: Vec<_> = (0..6)
        .map(|worker| {
            let manager = Arc::clone(&manager);
            std::thread::spawn(move || {
                for round in 0..5 {
                    for rel in documents {
                        manager
                            .save_document(rel, &json!({ "worker": worker, "round": round }))
                            .unwrap();
                    }
                }
            })
        })
        .collect();
    for handle in handles {
        handle.join().expect("ningún hilo debe fallar");
    }

    for rel in documents {
        let document = manager
            .load_document::<Value>(rel)
            .unwrap()
            .expect("el documento debe existir");
        assert!(document.data.get("worker").is_some());
    }
    let leftovers = fs::read_dir(root.path().join("library"))
        .unwrap()
        .flatten()
        .filter(|entry| entry.file_name().to_string_lossy().contains(".tmp"))
        .count();
    assert_eq!(
        leftovers, 0,
        "no quedan temporales tras escribir en paralelo"
    );
}
