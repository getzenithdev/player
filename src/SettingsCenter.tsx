import { themes } from "./preferences";
import {
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
  type ReactNode,
} from "react";
import type { AudioController } from "./audio";
import {
  accents,
  applyPreferences,
  artworkProviders,
  defaultSettings,
  loadSettings,
  lyricProviders,
  normalizeSettings,
  resetSettings,
  saveSettings,
  type AppSettings,
  type ArtworkQuality,
  type LyricFont,
  type LyricProvider,
  type RepeatMode,
} from "./preferences";
import {
  clearCache,
  createBackup,
  exportData,
  formatBytes,
  hasNativeStorage,
  importData,
  loadStorageStatus,
  type StorageStatus,
} from "./storage";
import {
  checkForUpdate,
  getUpdateChannel,
  installUpdate,
  saveUpdateChannel,
  type AvailableUpdate,
  type UpdateChannel,
} from "./updater";

const sections = [
  "General",
  "Apariencia",
  "Reproducción",
  "Letras",
  "Portadas",
  "Almacenamiento",
  "Privacidad",
] as const;

type Section = (typeof sections)[number];

function Row({
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

function Check({
  label,
  checked,
  onChange,
  disabled,
}: {
  label: string;
  checked: boolean;
  onChange: (checked: boolean) => void;
  disabled?: boolean;
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

/**
 * Settings center (roadmap phase 13).
 *
 * Controls apply immediately to the running interface and persist through the
 * debounced save; the footer also offers an explicit save. System integration
 * and audio-processing controls stay in their existing tabs because those
 * modules already own their documents and live state.
 */
export function SettingsCenter({
  audio,
  theme,
  onTheme,
  onSources,
}: {
  audio: AudioController;
  theme: string;
  onTheme: (theme: string) => void;
  onSources: () => void;
}) {
  const [storedDraft, setDraft] = useState<AppSettings>(defaultSettings);
  const draft = useMemo(
    () => ({
      ...storedDraft,
      appearance: { ...storedDraft.appearance, theme },
    }),
    [storedDraft, theme],
  );
  const [section, setSection] = useState<Section>("General");
  const [ready, setReady] = useState(false);
  const [busy, setBusy] = useState(false);
  const [notice, setNotice] = useState("");
  const [error, setError] = useState("");
  const [status, setStatus] = useState<StorageStatus | null>(null);
  const [updateChannel, setUpdateChannel] =
    useState<UpdateChannel>(getUpdateChannel);
  const [availableUpdate, setAvailableUpdate] =
    useState<AvailableUpdate | null>(null);
  const [updateProgress, setUpdateProgress] = useState<number | null>(null);
  const initialTheme = useRef(theme);
  const themeRef = useRef(theme);
  const onThemeRef = useRef(onTheme);
  const revision = useRef(0);
  const firstDraft = useRef(true);
  const pendingDraft = useRef<AppSettings | null>(null);
  const native = hasNativeStorage();

  useEffect(() => {
    themeRef.current = theme;
  }, [theme]);

  useEffect(() => {
    onThemeRef.current = onTheme;
  }, [onTheme]);

  useEffect(() => {
    let disposed = false;
    void loadSettings()
      .then((snapshot) => {
        if (disposed) return;
        const loaded = snapshot?.settings ?? defaultSettings;
        const next = normalizeSettings({
          ...loaded,
          appearance: {
            ...loaded.appearance,
            theme: currentTheme(initialTheme.current, loaded.appearance.theme),
          },
        });
        setDraft(next);
        applyPreferences(next);
        setReady(true);
      })
      .catch((reason) => {
        if (disposed) return;
        setError(String(reason));
      });
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

  useEffect(() => {
    if (!ready) return;
    let disposed = false;
    void checkForUpdate(updateChannel)
      .then((available) => {
        if (!disposed) setAvailableUpdate(available);
      })
      .catch(() => {
        if (!disposed)
          setNotice("No se pudo comprobar si hay actualizaciones.");
      });
    return () => {
      disposed = true;
    };
  }, [ready, updateChannel]);

  useEffect(
    () => () => {
      void availableUpdate?.update.close();
    },
    [availableUpdate],
  );

  const persist = useCallback(async (value: AppSettings) => {
    const current = ++revision.current;
    setBusy(true);
    setError("");
    try {
      applyPreferences(value);
      const snapshot = await saveSettings(value);
      if (current !== revision.current) return;
      if (value.appearance.theme !== themeRef.current) {
        themeRef.current = value.appearance.theme;
        onThemeRef.current(value.appearance.theme);
      }
      setNotice(
        snapshot
          ? "Preferencias guardadas."
          : "La vista previa se aplica; la aplicación de escritorio conserva los cambios.",
      );
    } catch (reason) {
      if (current === revision.current) setError(String(reason));
    } finally {
      if (current === revision.current) setBusy(false);
    }
  }, []);

  // Debounced autosave keeps range sliders responsive without writing on
  // every pointer movement. The first render only hydrates the form.
  useEffect(() => {
    if (!ready) return;
    pendingDraft.current = draft;
    if (firstDraft.current) {
      firstDraft.current = false;
      return;
    }
    const timer = window.setTimeout(() => void persist(draft), 400);
    return () => window.clearTimeout(timer);
  }, [draft, ready, persist]);

  useEffect(
    () => () => {
      if (pendingDraft.current) {
        void saveSettings(pendingDraft.current).catch(() => undefined);
      }
    },
    [],
  );

  function update(mutator: (current: AppSettings) => AppSettings) {
    setDraft((current) => {
      const next = normalizeSettings(mutator(current));
      applyPreferences(next);
      return next;
    });
    setNotice("");
  }

  function changeTheme(next: string) {
    update((current) => ({
      ...current,
      appearance: { ...current.appearance, theme: next },
    }));
    onTheme(next);
  }

  function changeVolume(value: number) {
    update((current) => ({
      ...current,
      playback: { ...current.playback, initialVolume: value },
    }));
    void audio.setVolume(value / 100);
  }

  function changeShuffle(value: boolean) {
    update((current) => ({
      ...current,
      playback: { ...current.playback, shuffle: value },
    }));
    void audio.setShuffle(value);
  }

  function changeRepeat(value: RepeatMode) {
    update((current) => ({
      ...current,
      playback: { ...current.playback, repeat: value },
    }));
    void audio.setRepeat(value);
  }

  function toggleProvider(provider: LyricProvider, checked: boolean) {
    update((current) => {
      const providers = current.lyrics.providers.filter(
        (value) => value !== provider,
      );
      if (checked) providers.push(provider);
      return {
        ...current,
        lyrics: {
          ...current.lyrics,
          providers: providers.length ? providers : [provider],
        },
      };
    });
  }

  async function resetAll() {
    if (
      !allowed("Se restaurarán todos los valores predeterminados. ¿Continuar?")
    )
      return;
    setBusy(true);
    setError("");
    try {
      const snapshot = await resetSettings();
      const next = snapshot?.settings ?? defaultSettings;
      setDraft(next);
      applyPreferences(next);
      onTheme(next.appearance.theme);
      setNotice("Preferencias restablecidas.");
    } catch (reason) {
      setError(String(reason));
    } finally {
      setBusy(false);
    }
  }

  function allowed(message: string) {
    return !draft.general.confirmDestructive || window.confirm(message);
  }

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

  return (
    <div className="preference-section">
      <div
        className="tabs"
        role="tablist"
        aria-label="Secciones de preferencias"
      >
        {sections.map((item, index) => (
          <button
            key={item}
            id={`preference-tab-${index}`}
            role="tab"
            type="button"
            aria-selected={section === item}
            aria-controls="preference-panel"
            tabIndex={section === item ? 0 : -1}
            onClick={() => setSection(item)}
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
                    ? sections.length - 1
                    : (index +
                        (event.key === "ArrowRight" ? 1 : -1) +
                        sections.length) %
                      sections.length;
              setSection(sections[next]);
              document.getElementById(`preference-tab-${next}`)?.focus();
            }}
          >
            {item}
          </button>
        ))}
      </div>
      <div
        id="preference-panel"
        role="tabpanel"
        aria-labelledby={`preference-tab-${sections.indexOf(section)}`}
      >
        {section === "General" && (
          <>
            <Row
              title="Idioma"
              description="La interfaz está en español; el ajuste queda listo para futuras traducciones."
            >
              <select
                aria-label="Idioma de la interfaz"
                value={draft.general.language}
                onChange={(event) =>
                  update((current) => ({
                    ...current,
                    general: {
                      ...current.general,
                      language: event.target.value as "es" | "system",
                    },
                  }))
                }
              >
                <option value="es">Español</option>
                <option value="system">Automático del sistema</option>
              </select>
            </Row>
            <Row
              title="Confirmaciones"
              description="Pide confirmación antes de limpiar la caché, importar datos o restablecer las preferencias."
            >
              <Check
                label="Confirmar acciones destructivas"
                checked={draft.general.confirmDestructive}
                onChange={(value) =>
                  update((current) => ({
                    ...current,
                    general: {
                      ...current.general,
                      confirmDestructive: value,
                    },
                  }))
                }
              />
            </Row>
            <Row
              title="Inicio y cierre"
              description="El inicio automático y el cierre a la bandeja se controlan en Sistema, junto con la integración nativa."
            >
              <span className="status">Ver sección Sistema</span>
            </Row>
            <Row
              title="Actualizaciones"
              description="Las versiones se descargan desde GitHub Releases y se verifican antes de instalarse."
            >
              <div className="button-group">
                <select
                  aria-label="Canal de actualizaciones"
                  value={updateChannel}
                  disabled={busy}
                  onChange={(event) => {
                    const channel = event.target.value as UpdateChannel;
                    setUpdateChannel(channel);
                    saveUpdateChannel(channel);
                    setAvailableUpdate(null);
                  }}
                >
                  <option value="stable">Estable</option>
                  <option value="beta">Beta</option>
                </select>
                <button
                  className="button"
                  type="button"
                  disabled={busy}
                  onClick={() =>
                    void run(async () => {
                      const available = await checkForUpdate(updateChannel);
                      setAvailableUpdate(available);
                      return available
                        ? `Versión ${available.update.version} disponible.`
                        : "Ya tienes la versión más reciente.";
                    })
                  }
                >
                  Buscar
                </button>
                {availableUpdate && (
                  <>
                    <button
                      className="button"
                      type="button"
                      disabled={busy}
                      onClick={() => {
                        void (async () => {
                          setBusy(true);
                          setError("");
                          setUpdateProgress(0);
                          try {
                            await installUpdate(
                              availableUpdate,
                              setUpdateProgress,
                            );
                          } catch (reason) {
                            setError(String(reason));
                          } finally {
                            setBusy(false);
                            setUpdateProgress(null);
                          }
                        })();
                      }}
                    >
                      {updateProgress === null
                        ? "Instalar"
                        : `Descargando ${updateProgress} %`}
                    </button>
                    {availableUpdate.update.body && (
                      <span className="status">
                        Notas: {availableUpdate.update.body}
                      </span>
                    )}
                  </>
                )}
              </div>
            </Row>
          </>
        )}
        {section === "Apariencia" && (
          <>
            <Row
              title="Tema"
              description="Violeta es el tema predeterminado. Automático sigue el tema de tu equipo."
            >
              <select
                aria-label="Tema de la interfaz"
                value={draft.appearance.theme}
                onChange={(event) => changeTheme(event.target.value)}
              >
                {themes.map((theme) => (
                  <option key={theme.value} value={theme.value}>
                    {theme.label}
                  </option>
                ))}
              </select>
            </Row>
            <Row
              title="Color de acento"
              description="Afecta controles, enlaces y estados seleccionados."
            >
              <select
                aria-label="Color de acento"
                value={draft.appearance.accent}
                onChange={(event) =>
                  update((current) => ({
                    ...current,
                    appearance: {
                      ...current.appearance,
                      accent: event.target
                        .value as AppSettings["appearance"]["accent"],
                    },
                  }))
                }
              >
                {accents.map((accent) => (
                  <option value={accent.value} key={accent.value}>
                    {accent.label}
                  </option>
                ))}
              </select>
            </Row>
            <Row
              title="Escala"
              description="Amplía o reduce toda la interfaz sin cambiar el tamaño de la ventana."
            >
              <div className="preference-inline">
                <input
                  type="range"
                  aria-label="Escala de la interfaz"
                  min={75}
                  max={200}
                  step={5}
                  value={draft.appearance.scale}
                  onChange={(event) =>
                    update((current) => ({
                      ...current,
                      appearance: {
                        ...current.appearance,
                        scale: Number(event.target.value),
                      },
                    }))
                  }
                />
                <output>{draft.appearance.scale} %</output>
              </div>
            </Row>
            <Row
              title="Animaciones"
              description="La opción reducida desactiva transiciones y movimiento decorativo."
            >
              <select
                aria-label="Animaciones de la interfaz"
                value={draft.appearance.animations}
                onChange={(event) =>
                  update((current) => ({
                    ...current,
                    appearance: {
                      ...current.appearance,
                      animations: event.target
                        .value as AppSettings["appearance"]["animations"],
                    },
                  }))
                }
              >
                <option value="full">Completas</option>
                <option value="reduced">Reducidas</option>
              </select>
            </Row>
            <Row
              title="Fondo"
              description="El fondo dinámico toma color de la portada; el sólido usa solo la superficie del tema."
            >
              <select
                aria-label="Fondo de la interfaz"
                value={draft.appearance.background}
                onChange={(event) =>
                  update((current) => ({
                    ...current,
                    appearance: {
                      ...current.appearance,
                      background: event.target
                        .value as AppSettings["appearance"]["background"],
                    },
                  }))
                }
              >
                <option value="dynamic">Dinámico</option>
                <option value="solid">Sólido</option>
              </select>
            </Row>
            <Row
              title="Reproductor"
              description="La disposición compacta reduce la altura del reproductor inferior."
            >
              <select
                aria-label="Disposición del reproductor"
                value={draft.appearance.playerLayout}
                onChange={(event) =>
                  update((current) => ({
                    ...current,
                    appearance: {
                      ...current.appearance,
                      playerLayout: event.target
                        .value as AppSettings["appearance"]["playerLayout"],
                    },
                  }))
                }
              >
                <option value="full">Completa</option>
                <option value="compact">Compacta</option>
              </select>
            </Row>
            <Row
              title="Estilo de letras"
              description="Ajusta el espacio y el contraste de la lectura sin cambiar el tamaño configurado en Letras."
            >
              <select
                aria-label="Estilo de letras"
                value={draft.appearance.lyricsStyle}
                onChange={(event) =>
                  update((current) => ({
                    ...current,
                    appearance: {
                      ...current.appearance,
                      lyricsStyle: event.target
                        .value as AppSettings["appearance"]["lyricsStyle"],
                    },
                  }))
                }
              >
                <option value="balanced">Equilibrado</option>
                <option value="focus">Enfoque</option>
                <option value="lyrics">Letra protagonista</option>
              </select>
            </Row>
          </>
        )}
        {section === "Reproducción" && (
          <>
            <Row
              title="Volumen inicial"
              description="Se aplica al abrir la aplicación sin una sesión restaurada."
            >
              <div className="preference-inline">
                <input
                  type="range"
                  aria-label="Volumen inicial"
                  min={0}
                  max={100}
                  step={1}
                  value={draft.playback.initialVolume}
                  onChange={(event) => changeVolume(Number(event.target.value))}
                />
                <output>{draft.playback.initialVolume} %</output>
              </div>
            </Row>
            <Row
              title="Aleatorio"
              description="Valor inicial de la cola; puedes cambiarlo desde el reproductor."
            >
              <Check
                label="Reproducir en orden aleatorio"
                checked={draft.playback.shuffle}
                onChange={changeShuffle}
              />
            </Row>
            <Row
              title="Repetición"
              description="Valor inicial aplicado a la cola de reproducción."
            >
              <select
                aria-label="Modo de repetición inicial"
                value={draft.playback.repeat}
                onChange={(event) =>
                  changeRepeat(event.target.value as RepeatMode)
                }
              >
                <option value="off">Desactivada</option>
                <option value="all">Toda la cola</option>
                <option value="one">Una canción</option>
              </select>
            </Row>
            <Row
              title="Salida de sonido"
              description={
                audio.capabilities
                  ? `${audio.capabilities.decoder} · ${audio.capabilities.output}`
                  : "Salida de audio del sistema."
              }
            >
              <button
                className="button"
                type="button"
                disabled={!audio.available}
                onClick={() => void audio.restartOutput()}
              >
                Reiniciar salida
              </button>
            </Row>
            <p className="preference-note">
              El ecualizador, el crossfade y el procesamiento avanzado viven en
              la sección Audio/Ecualizador y en el documento del motor de audio.
            </p>
          </>
        )}
        {section === "Letras" && (
          <>
            <Row
              title="Proveedores"
              description="Los archivos locales y los metadatos funcionan sin conexión. LRCLIB requiere la autorización externa existente."
            >
              <div className="preference-checks">
                {lyricProviders.map((provider) => (
                  <label key={provider.value}>
                    <input
                      type="checkbox"
                      checked={draft.lyrics.providers.includes(provider.value)}
                      onChange={(event) =>
                        toggleProvider(provider.value, event.target.checked)
                      }
                    />
                    {provider.label}
                  </label>
                ))}
              </div>
            </Row>
            <Row
              title="Tamaño"
              description="Tamaño base de la letra en las vistas normal y de enfoque."
            >
              <div className="preference-inline">
                <input
                  type="range"
                  aria-label="Tamaño de la letra"
                  min={16}
                  max={80}
                  step={1}
                  value={draft.lyrics.size}
                  onChange={(event) =>
                    update((current) => ({
                      ...current,
                      lyrics: {
                        ...current.lyrics,
                        size: Number(event.target.value),
                      },
                    }))
                  }
                />
                <output>{draft.lyrics.size} px</output>
              </div>
            </Row>
            <Row
              title="Tipografía"
              description="La tipografía del sistema respeta las fuentes instaladas."
            >
              <select
                aria-label="Tipografía de las letras"
                value={draft.lyrics.font}
                onChange={(event) =>
                  update((current) => ({
                    ...current,
                    lyrics: {
                      ...current.lyrics,
                      font: event.target.value as LyricFont,
                    },
                  }))
                }
              >
                <option value="system">Sistema</option>
                <option value="serif">Serif</option>
                <option value="sans-serif">Sans serif</option>
              </select>
            </Row>
            <Row
              title="Colores"
              description="Color de la letra inactiva y de la línea sincronizada."
            >
              <div className="preference-colors">
                <label>
                  Inactiva
                  <input
                    type="color"
                    aria-label="Color de la letra inactiva"
                    value={draft.lyrics.color}
                    onChange={(event) =>
                      update((current) => ({
                        ...current,
                        lyrics: {
                          ...current.lyrics,
                          color: event.target.value,
                        },
                      }))
                    }
                  />
                </label>
                <label>
                  Activa
                  <input
                    type="color"
                    aria-label="Color de la letra activa"
                    value={draft.lyrics.activeColor}
                    onChange={(event) =>
                      update((current) => ({
                        ...current,
                        lyrics: {
                          ...current.lyrics,
                          activeColor: event.target.value,
                        },
                      }))
                    }
                  />
                </label>
              </div>
            </Row>
            <Row
              title="Desfase"
              description="Valores positivos adelantan la letra. Se guarda para todas las canciones."
            >
              <div className="preference-inline">
                <input
                  type="number"
                  aria-label="Desfase de letras en milisegundos"
                  min={-600000}
                  max={600000}
                  step={50}
                  value={draft.lyrics.offsetMs}
                  onChange={(event) =>
                    update((current) => ({
                      ...current,
                      lyrics: {
                        ...current.lyrics,
                        offsetMs: Number(event.target.value),
                      },
                    }))
                  }
                />
                <span className="preference-value">ms</span>
              </div>
            </Row>
          </>
        )}
        {section === "Portadas" && (
          <>
            <Row
              title="Prioridad de proveedores"
              description="El motor prefiere las portadas animadas y ordena los proveedores con esta lista."
            >
              <ol className="preference-order">
                {draft.artwork.providerPriority.map((provider, index) => (
                  <li key={provider}>
                    {artworkProviders.find((item) => item.value === provider)
                      ?.label ?? provider}
                    <button
                      type="button"
                      className="button"
                      aria-label={`Subir ${provider}`}
                      disabled={index === 0}
                      onClick={() =>
                        update((current) => {
                          const priority = [
                            ...current.artwork.providerPriority,
                          ];
                          [priority[index - 1], priority[index]] = [
                            priority[index],
                            priority[index - 1],
                          ];
                          return {
                            ...current,
                            artwork: {
                              ...current.artwork,
                              providerPriority: priority,
                            },
                          };
                        })
                      }
                    >
                      ↑
                    </button>
                    <button
                      type="button"
                      className="button"
                      aria-label={`Bajar ${provider}`}
                      disabled={
                        index === draft.artwork.providerPriority.length - 1
                      }
                      onClick={() =>
                        update((current) => {
                          const priority = [
                            ...current.artwork.providerPriority,
                          ];
                          [priority[index], priority[index + 1]] = [
                            priority[index + 1],
                            priority[index],
                          ];
                          return {
                            ...current,
                            artwork: {
                              ...current.artwork,
                              providerPriority: priority,
                            },
                          };
                        })
                      }
                    >
                      ↓
                    </button>
                  </li>
                ))}
              </ol>
            </Row>
            <Row
              title="Portadas animadas"
              description="Reproduce GIF y videos cortos solo mientras la portada está visible."
            >
              <Check
                label="Animar portadas"
                checked={draft.artwork.animate}
                onChange={(value) =>
                  update((current) => ({
                    ...current,
                    artwork: { ...current.artwork, animate: value },
                  }))
                }
              />
            </Row>
            <Row
              title="Repetición y batería"
              description="Las portadas animadas se pausan al ocultarse y con el ahorro de batería."
            >
              <div className="preference-checks">
                <label>
                  <input
                    type="checkbox"
                    checked={draft.artwork.loopAnimation}
                    onChange={(event) =>
                      update((current) => ({
                        ...current,
                        artwork: {
                          ...current.artwork,
                          loopAnimation: event.target.checked,
                        },
                      }))
                    }
                  />
                  Repetir la animación
                </label>
                <label>
                  <input
                    type="checkbox"
                    checked={draft.artwork.batterySaver}
                    onChange={(event) =>
                      update((current) => ({
                        ...current,
                        artwork: {
                          ...current.artwork,
                          batterySaver: event.target.checked,
                        },
                      }))
                    }
                  />
                  Ahorro de batería
                </label>
              </div>
            </Row>
            <Row
              title="Calidad"
              description="La vista previa usa menos memoria; la miniatura es más pequeña en listas."
            >
              <select
                aria-label="Calidad de las portadas"
                value={draft.artwork.quality}
                onChange={(event) =>
                  update((current) => ({
                    ...current,
                    artwork: {
                      ...current.artwork,
                      quality: event.target.value as ArtworkQuality,
                    },
                  }))
                }
              >
                <option value="preview">Vista previa</option>
                <option value="thumbnail">Miniatura</option>
              </select>
            </Row>
            <Row
              title="Descargas de portadas"
              description="Descargar imágenes de proveedores externos requiere la autorización de Fuentes."
            >
              <Check
                label="Permitir descargas de portadas"
                checked={draft.artwork.allowDownloads}
                onChange={(value) =>
                  update((current) => ({
                    ...current,
                    artwork: { ...current.artwork, allowDownloads: value },
                  }))
                }
              />
            </Row>
            <p className="preference-note">
              La prioridad se guarda en la configuración central; el orden de
              resolución actual se aplica en la próxima consulta de portadas.
            </p>
          </>
        )}
        {section === "Almacenamiento" && (
          <>
            <Row
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
            </Row>
            <Row
              title="Modo portátil"
              description="Se activa con el archivo zenith-portable junto al ejecutable."
            >
              <span className="status">
                {status?.portable ? "Activado" : "Desactivado"}
              </span>
            </Row>
            <Row
              title="Espacio de la caché"
              description={`${status ? formatBytes(status.cache_bytes) : "Audio e imágenes"} en caché. Limpiarla elimina descargas temporales, no la biblioteca ni tus archivos originales.`}
            >
              <button
                className="button"
                type="button"
                disabled={!native || busy}
                onClick={() => {
                  if (
                    !allowed(
                      "Se vaciarán las descargas y las imágenes en caché. La biblioteca y los archivos originales no se modifican. ¿Continuar?",
                    )
                  )
                    return;
                  void run(async () => {
                    const report = await clearCache();
                    return `Liberados ${formatBytes(report.freed_bytes)} en ${report.removed_files} archivos.`;
                  });
                }}
              >
                Limpiar caché
              </button>
            </Row>
            <Row
              title="Respaldo"
              description="Copia los documentos válidos sin incluir credenciales."
            >
              <button
                className="button"
                type="button"
                disabled={!native || busy}
                onClick={() =>
                  void run(async () => {
                    const report = await createBackup();
                    return `Respaldo creado: ${report.created.length} documentos en ${report.directory}.`;
                  })
                }
              >
                Crear respaldo
              </button>
            </Row>
            <Row
              title="Importar y exportar"
              description="El paquete incluye configuración, biblioteca y letras; nunca credenciales."
            >
              <div className="button-group">
                <button
                  className="button"
                  type="button"
                  disabled={!native || busy}
                  onClick={() =>
                    void run(async () => {
                      const report = await exportData();
                      return `Datos exportados a ${report.path}.`;
                    })
                  }
                >
                  Exportar
                </button>
                <button
                  className="button"
                  type="button"
                  disabled={!native || busy}
                  onClick={() => {
                    if (
                      !allowed(
                        "Los documentos actuales se reemplazarán por el paquete importado. ¿Continuar?",
                      )
                    )
                      return;
                    void run(async () => {
                      const report = await importData();
                      return `Datos importados de ${report.path}.`;
                    });
                  }}
                >
                  Importar
                </button>
              </div>
            </Row>
            <details className="advanced-details">
              <summary>Estado y recuperación</summary>
              <p>
                {status?.documents.filter((document) => document.valid)
                  .length ?? 0}{" "}
                documentos válidos. {status?.index.tracks ?? 0} canciones
                indexadas.
              </p>
              {status?.recovery.map((item) => (
                <p key={`${item.file}-${item.kind}`}>{item.message}</p>
              ))}
            </details>
          </>
        )}
        {section === "Privacidad" && (
          <>
            <Row
              title="Telemetría"
              description="Zenith no envía estadísticas de uso. El código no incluye un recolector de telemetría."
            >
              <span className="status">Sin telemetría</span>
            </Row>
            <Row
              title="Credenciales"
              description="Las contraseñas, claves y tokens viven en el almacén de credenciales del sistema, nunca en la configuración central ni en las exportaciones."
            >
              <span className="status">Almacenamiento del sistema</span>
            </Row>
            <Row
              title="Fuentes conectadas"
              description="Conecta, revoca o revisa la autorización externa de cada proveedor."
            >
              <button className="button" type="button" onClick={onSources}>
                Administrar fuentes
              </button>
            </Row>
            <Row
              title="Historial y caché"
              description="Limpiar la caché no borra el historial, las listas ni los archivos originales. El historial se administra desde su pantalla."
            >
              <button
                className="button"
                type="button"
                disabled={!native || busy}
                onClick={() => {
                  if (
                    !allowed(
                      "Se vaciarán las descargas y las imágenes en caché. ¿Continuar?",
                    )
                  )
                    return;
                  void run(async () => {
                    const report = await clearCache();
                    return `Liberados ${formatBytes(report.freed_bytes)} en ${report.removed_files} archivos.`;
                  });
                }}
              >
                Limpiar caché
              </button>
            </Row>
            <p className="preference-note">
              La revocación de una conexión no elimina archivos locales ni la
              biblioteca.
            </p>
          </>
        )}
      </div>
      <div className="preference-footer">
        <span className="preference-status" role="status">
          {busy ? "Guardando…" : notice}
        </span>
        <button
          className="button"
          type="button"
          disabled={!ready || busy}
          onClick={() => void persist(draft)}
        >
          Guardar ahora
        </button>
        <button
          className="button"
          type="button"
          disabled={!ready || busy}
          onClick={() => void resetAll()}
        >
          Restablecer
        </button>
      </div>
      {error && (
        <p className="error-note" role="alert">
          {error}
        </p>
      )}
    </div>
  );
}

function currentTheme(current: string, stored: string) {
  return current || stored;
}
