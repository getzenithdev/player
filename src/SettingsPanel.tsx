import { useEffect, useState, type ReactNode } from "react";
import type { AudioController } from "./audio";
import {
  defaultIntegrationSettings,
  testNotification,
  type IntegrationSettings,
  type IntegrationSnapshot,
} from "./native";
import {
  createBackup,
  clearCache,
  exportData,
  formatBytes,
  hasNativeStorage,
  importData,
  loadStorageStatus,
  type StorageStatus,
} from "./storage";

function Setting({
  title,
  description,
  children,
}: {
  title: string;
  description: string;
  children: ReactNode;
}) {
  return (
    <div className="setting-row">
      <div>
        <h3>{title}</h3>
        {description && <p>{description}</p>}
      </div>
      <div className="setting-control">{children}</div>
    </div>
  );
}
function Toggle({
  label,
  checked,
  disabled,
  onChange,
}: {
  label: string;
  checked: boolean;
  disabled: boolean;
  onChange: (checked: boolean) => void;
}) {
  return (
    <label className="toggle-control">
      <input
        type="checkbox"
        aria-label={label}
        checked={checked}
        disabled={disabled}
        onChange={(event) => onChange(event.target.checked)}
      />
      <span>{checked ? "Activado" : "Desactivado"}</span>
    </label>
  );
}
export function Settings({
  theme,
  onTheme,
  audio,
  onSources,
  onEqualizer,
  integration = defaultIntegrationSettings,
  integrationSnapshot = null,
  onIntegration,
}: {
  theme: string;
  onTheme: (theme: string) => void;
  audio: AudioController;
  onSources: () => void;
  onEqualizer: () => void;
  integration?: IntegrationSettings;
  integrationSnapshot?: IntegrationSnapshot | null;
  onIntegration: (settings: IntegrationSettings) => void;
}) {
  const [tab, setTab] = useState("Apariencia");
  const [status, setStatus] = useState<StorageStatus | null>(null);
  const [busy, setBusy] = useState(false);
  const [notice, setNotice] = useState("");
  const [error, setError] = useState("");
  const native = hasNativeStorage();
  useEffect(() => {
    let disposed = false;
    void loadStorageStatus()
      .then((value) => {
        if (!disposed) setStatus(value);
      })
      .catch((reason) => {
        if (!disposed) setError(String(reason));
      });
    return () => {
      disposed = true;
    };
  }, []);
  async function run(action: () => Promise<string>) {
    setBusy(true);
    setError("");
    setNotice("");
    try {
      setNotice(await action());
      setStatus(await loadStorageStatus());
    } catch (reason) {
      setError(String(reason));
    } finally {
      setBusy(false);
    }
  }
  const tabs = [
    "Apariencia",
    "Sistema",
    "Accesibilidad",
    "Biblioteca y datos",
    "Audio",
    "Privacidad",
  ];
  return (
    <section className="settings-screen">
      <div
        className="tabs"
        role="tablist"
        aria-label="Secciones de configuración"
      >
        {tabs.map((item, index) => (
          <button
            key={item}
            id={`setting-tab-${index}`}
            role="tab"
            aria-selected={tab === item}
            aria-controls="settings-panel"
            tabIndex={tab === item ? 0 : -1}
            onClick={() => setTab(item)}
            onKeyDown={(event) => {
              if (
                !["ArrowLeft", "ArrowRight", "Home", "End"].includes(event.key)
              )
                return;
              event.preventDefault();
              const next =
                event.key === "Home"
                  ? 0
                  : event.key === "End"
                    ? tabs.length - 1
                    : (index +
                        (event.key === "ArrowRight" ? 1 : -1) +
                        tabs.length) %
                      tabs.length;
              setTab(tabs[next]);
              document.getElementById(`setting-tab-${next}`)?.focus();
            }}
          >
            {item}
          </button>
        ))}
      </div>
      <div
        id="settings-panel"
        role="tabpanel"
        aria-labelledby={`setting-tab-${tabs.indexOf(tab)}`}
      >
        {tab === "Apariencia" && (
          <>
            <Setting
              title="Tema"
              description="Automático sigue el tema de tu equipo."
            >
              <select
                aria-label="Tema de la interfaz"
                value={theme}
                onChange={(e) => onTheme(e.target.value)}
              >
                <option value="dark">Oscuro</option>
                <option value="light">Claro</option>
                <option value="system">Automático</option>
              </select>
            </Setting>
            <div className="theme-previews" aria-hidden="true">
              <div className="theme-preview dark-preview">
                <i />
                <span />
                <b>Oscuro</b>
              </div>
              <div className="theme-preview light-preview">
                <i />
                <span />
                <b>Claro</b>
              </div>
            </div>
            <Setting title="Idioma" description="">
              <span className="status">Español</span>
            </Setting>
          </>
        )}
        {tab === "Sistema" && (
          <>
            <Setting
              title="Controles multimedia"
              description="Reproduce, pausa y cambia de pista desde los controles del sistema (SMTC, MPRIS o Now Playing)."
            >
              <Toggle
                label="Controles multimedia del sistema"
                checked={integration.mediaControls}
                disabled={!native}
                onChange={(value) =>
                  onIntegration({ ...integration, mediaControls: value })
                }
              />
            </Setting>
            <Setting
              title="Teclas multimedia"
              description="Reproducir, pausar, anterior, siguiente y detener desde el teclado, incluso con la ventana oculta."
            >
              <Toggle
                label="Teclas multimedia globales"
                checked={integration.globalShortcuts}
                disabled={!native}
                onChange={(value) =>
                  onIntegration({ ...integration, globalShortcuts: value })
                }
              />
            </Setting>
            <Setting
              title="Notificaciones"
              description="Avisa del cambio de pista cuando la ventana no tiene el foco."
            >
              <div className="button-group">
                <Toggle
                  label="Notificaciones de pista"
                  checked={integration.notifications}
                  disabled={!native}
                  onChange={(value) =>
                    onIntegration({ ...integration, notifications: value })
                  }
                />
                <button
                  className="button"
                  disabled={!native || busy || !integration.notifications}
                  onClick={() =>
                    void run(async () => {
                      const sent = await testNotification();
                      return sent
                        ? "Notificación de prueba enviada."
                        : "Las notificaciones requieren la aplicación de escritorio.";
                    })
                  }
                >
                  Probar
                </button>
              </div>
            </Setting>
            <Setting
              title="Bandeja del sistema"
              description="Mantiene un icono con acciones de reproducción y acceso a la ventana."
            >
              <Toggle
                label="Icono en la bandeja del sistema"
                checked={integration.tray}
                disabled={!native}
                onChange={(value) =>
                  onIntegration({ ...integration, tray: value })
                }
              />
            </Setting>
            <Setting
              title="Cerrar a la bandeja"
              description="Cerrar la ventana mantiene la música sonando hasta salir desde la bandeja."
            >
              <Toggle
                label="Cerrar la ventana a la bandeja"
                checked={integration.closeToTray}
                disabled={!native || !integration.tray}
                onChange={(value) =>
                  onIntegration({ ...integration, closeToTray: value })
                }
              />
            </Setting>
            <Setting
              title="Inicio automático"
              description="Abre Zenith al iniciar sesión, sin interrumpir el escritorio."
            >
              <Toggle
                label="Iniciar al encender el equipo"
                checked={integration.autostart}
                disabled={!native || !integrationSnapshot?.autostartSupported}
                onChange={(value) =>
                  onIntegration({ ...integration, autostart: value })
                }
              />
            </Setting>
            <Setting
              title="Estado de la integración"
              description={`Plataforma: ${integrationSnapshot?.platform ?? "vista previa del navegador"}.`}
            >
              <span className="status">
                {integrationSnapshot
                  ? [
                      integrationSnapshot.mediaControlsActive && "Controles",
                      integrationSnapshot.trayActive && "Bandeja",
                      integrationSnapshot.shortcutsActive && "Atajos",
                    ]
                      .filter(Boolean)
                      .join(" · ") || "Ninguna integración activa"
                  : native
                    ? "Cargando…"
                    : "Aplicación de escritorio"}
              </span>
            </Setting>
            {integrationSnapshot?.notices.map((item, index) => (
              <p className="inline-note" key={`${item}-${index}`}>
                {item}
              </p>
            ))}
          </>
        )}
        {tab === "Accesibilidad" && (
          <>
            <Setting
              title="Movimiento reducido"
              description="Desactiva animaciones y transiciones aunque el sistema no lo indique."
            >
              <Toggle
                label="Reducir el movimiento de la interfaz"
                checked={integration.reducedMotion}
                disabled={!native}
                onChange={(value) =>
                  onIntegration({ ...integration, reducedMotion: value })
                }
              />
            </Setting>
            <Setting
              title="Alto contraste"
              description="Refuerza bordes, texto y foco para pantallas con poca visibilidad."
            >
              <Toggle
                label="Activar el alto contraste"
                checked={integration.highContrast}
                disabled={!native}
                onChange={(value) =>
                  onIntegration({ ...integration, highContrast: value })
                }
              />
            </Setting>
            <Setting
              title="Lectores de pantalla"
              description="Los cambios de pista y los avisos se anuncian sin mover el foco."
            >
              <span className="status">Región de estado activa</span>
            </Setting>
            <Setting
              title="Escalado y zoom"
              description="La interfaz respeta la escala del sistema y el zoom del motor web."
            >
              <span className="status">Automático</span>
            </Setting>
            <div className="shortcut-help">
              <h3>Atajos de teclado</h3>
              <ul className="shortcut-list">
                <li>
                  <kbd>Espacio</kbd> Reproducir o pausar
                </li>
                <li>
                  <kbd>←</kbd> <kbd>→</kbd> Retroceder o avanzar 5 segundos
                </li>
                <li>
                  <kbd>Ctrl</kbd> + <kbd>←</kbd> / <kbd>→</kbd> Pista anterior o
                  siguiente
                </li>
                <li>
                  <kbd>M</kbd> Silenciar o restaurar el sonido
                </li>
                <li>
                  <kbd>Ctrl</kbd> + <kbd>K</kbd> Buscar en la biblioteca
                </li>
              </ul>
              <p className="inline-note">
                Los campos de texto y los controles con foco conservan sus
                propias teclas.
              </p>
            </div>
          </>
        )}
        {tab === "Biblioteca y datos" && (
          <>
            <Setting
              title="Ubicación de los datos"
              description={
                status
                  ? status.root
                  : "Tu biblioteca, listas, letras y preferencias se guardan localmente."
              }
            >
              <span className="status">
                {status
                  ? status.writable
                    ? "Disponible"
                    : "Solo lectura"
                  : native
                    ? "Cargando…"
                    : "Aplicación de escritorio"}
              </span>
            </Setting>
            <Setting
              title="Modo portátil"
              description="Se activa con el archivo zenith-portable junto al ejecutable."
            >
              <span className="status">
                {status?.portable ? "Activado" : "Desactivado"}
              </span>
            </Setting>
            <Setting
              title="Respaldo de tus datos"
              description="Biblioteca y preferencias."
            >
              <button
                className="button"
                disabled={!native || busy}
                onClick={() =>
                  void run(async () => {
                    const r = await createBackup();
                    return `Respaldo creado: ${r.created.length} documentos en ${r.directory}.`;
                  })
                }
              >
                Crear respaldo
              </button>
            </Setting>
            <Setting
              title="Importar y exportar"
              description="Las credenciales no se exportan."
            >
              <div className="button-group">
                <button
                  className="button"
                  disabled={!native || busy}
                  onClick={() =>
                    void run(async () => {
                      const r = await exportData();
                      return `Datos exportados a ${r.path}.`;
                    })
                  }
                >
                  Exportar
                </button>
                <button
                  className="button"
                  disabled={!native || busy}
                  onClick={() =>
                    void run(async () => {
                      const r = await importData();
                      return `Datos importados de ${r.path}.`;
                    })
                  }
                >
                  Importar
                </button>
              </div>
            </Setting>
            <Setting
              title="Espacio de la caché"
              description={`${status ? formatBytes(status.cache_bytes) : "Audio e imágenes"} en caché. Limpiarla elimina las descargas, sin afectar tus archivos originales.`}
            >
              <button
                className="button"
                disabled={!native || busy}
                onClick={() =>
                  void run(async () => {
                    const r = await clearCache();
                    return `Liberados ${formatBytes(r.freed_bytes)} en ${r.removed_files} archivos.`;
                  })
                }
              >
                Limpiar caché
              </button>
            </Setting>
            <details className="advanced-details">
              <summary>Estado y recuperación</summary>
              <p>
                {status?.documents.filter((d) => d.valid).length ?? 0}{" "}
                documentos válidos. {status?.index.tracks ?? 0} canciones
                indexadas.
              </p>
              {status?.recovery.map((item) => (
                <p key={`${item.file}-${item.kind}`}>{item.message}</p>
              ))}
            </details>
          </>
        )}
        {tab === "Audio" && (
          <>
            <Setting
              title="Salida de sonido"
              description={
                audio.capabilities
                  ? `${audio.capabilities.decoder} · ${audio.capabilities.output}`
                  : "Salida de audio del sistema."
              }
            >
              <button
                className="button"
                disabled={!audio.available}
                onClick={() => void audio.restartOutput()}
              >
                Reiniciar salida
              </button>
            </Setting>
            <Setting
              title="Formatos de reproducción"
              description={
                audio.capabilities?.formats.join(", ") ??
                "Los formatos disponibles se muestran en la aplicación de escritorio."
              }
            >
              <span className="status">Audio local</span>
            </Setting>
            <Setting
              title="Ecualizador y procesamiento"
              description="Diez bandas, presets, preamplificación, balance, ReplayGain, normalización y fundido entre pistas."
            >
              <button className="button" onClick={onEqualizer}>
                Abrir ecualizador
              </button>
            </Setting>
            <p className="inline-note">
              El procesamiento se aplica en tiempo real y se guarda en la
              configuración local. Desactívalo para escuchar la señal original.
            </p>
          </>
        )}
        {tab === "Privacidad" && (
          <>
            <Setting
              title="Fuentes conectadas"
              description="Conecta o desconecta tus servicios."
            >
              <button className="button" onClick={onSources}>
                Administrar fuentes
              </button>
            </Setting>
            <Setting
              title="Credenciales protegidas"
              description="Las contraseñas y los tokens se guardan en el almacén de credenciales del sistema."
            >
              <span className="status">Almacenamiento del sistema</span>
            </Setting>
            <Setting title="Telemetría" description="">
              <span className="status">Sin telemetría</span>
            </Setting>
          </>
        )}
      </div>
      {busy && <p role="status">Guardando cambios…</p>}
      {notice && (
        <p className="inline-note" role="status">
          {notice}
        </p>
      )}
      {error && (
        <p className="error-note" role="alert">
          {error}
        </p>
      )}
      <p className="settings-version">
        Zenith Player <span>0.1.0</span>
      </p>
    </section>
  );
}
