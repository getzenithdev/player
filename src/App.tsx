import { useState, type ReactNode, type KeyboardEvent } from "react";
import { Icon } from "./Icon";
import { phases, type Phase, type View } from "./phases";
import "./App.css";

const navigation: { view: View; label: string }[] = [
  { view: "library", label: "Biblioteca" },
  { view: "playlists", label: "Listas" },
  { view: "queue", label: "Cola" },
  { view: "lyrics", label: "Letras" },
  { view: "artwork", label: "Portadas" },
  { view: "sources", label: "Fuentes" },
  { view: "equalizer", label: "Ecualizador" },
  { view: "settings", label: "Configuración" },
  { view: "phases", label: "Fases" },
];

const pagePhases: Record<View, number[]> = {
  library: [4, 6],
  playlists: [5],
  queue: [3, 5],
  lyrics: [7],
  artwork: [8],
  sources: [9],
  equalizer: [12],
  settings: [2, 11, 13, 14, 16],
  phases: [],
};
const frequencies = [
  "31",
  "62",
  "125",
  "250",
  "500",
  "1k",
  "2k",
  "4k",
  "8k",
  "16k",
];
const settingsTabs = [
  "Apariencia",
  "Almacenamiento",
  "Sistema",
  "Actualizaciones",
  "Privacidad",
];

function navigateTabs(event: KeyboardEvent<HTMLButtonElement>) {
  if (!["ArrowLeft", "ArrowRight", "Home", "End"].includes(event.key)) return;
  const buttons = Array.from(
    event.currentTarget.parentElement?.querySelectorAll<HTMLButtonElement>(
      '[role="tab"]',
    ) ?? [],
  );
  const current = buttons.indexOf(event.currentTarget);
  const next =
    event.key === "Home"
      ? 0
      : event.key === "End"
        ? buttons.length - 1
        : (current + (event.key === "ArrowRight" ? 1 : -1) + buttons.length) %
          buttons.length;
  event.preventDefault();
  buttons[next]?.focus();
  buttons[next]?.click();
}

function PendingButton({
  children,
  phase,
}: {
  children: ReactNode;
  phase: number;
}) {
  return (
    <button
      className="button"
      disabled
      title={`Disponible al implementar la fase ${phase}`}
    >
      {children}
    </button>
  );
}

function EmptyState({
  icon,
  title,
  children,
}: {
  icon: View;
  title: string;
  children: ReactNode;
}) {
  return (
    <div className="empty-state">
      <span className="empty-icon">
        <Icon name={icon} size={30} />
      </span>
      <h2>{title}</h2>
      <p>{children}</p>
    </div>
  );
}

function SettingRow({
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
        <p>{description}</p>
      </div>
      <div className="setting-control">{children}</div>
    </div>
  );
}

export default function App() {
  const [view, setView] = useState<View>("library");
  const [theme, setTheme] = useState("light");
  const [libraryTab, setLibraryTab] = useState("Canciones");
  const [search, setSearch] = useState("");
  const [settingsTab, setSettingsTab] = useState("Apariencia");
  const [selectedPhase, setSelectedPhase] = useState(2);
  const [reviewed, setReviewed] = useState<string[]>([]);
  const [playlistName, setPlaylistName] = useState("");
  const [playlists, setPlaylists] = useState<{ id: number; name: string }[]>(
    [],
  );
  const [nextPlaylistId, setNextPlaylistId] = useState(1);
  const [bands, setBands] = useState<number[]>(Array(10).fill(0));
  const [lyricsMode, setLyricsMode] = useState("Normal");
  const phase = phases.find((item) => item.id === selectedPhase)!;
  const title = navigation.find((item) => item.view === view)!.label;

  function openPhase(id: number) {
    setSelectedPhase(id);
    setView("phases");
  }

  function openModule(item: Phase) {
    setView(item.view);
    if (item.view === "settings") {
      setSettingsTab(
        item.id === 2
          ? "Almacenamiento"
          : item.id === 11
            ? "Sistema"
            : item.id >= 14
              ? "Actualizaciones"
              : "Apariencia",
      );
    }
  }

  function toggleReview(key: string) {
    setReviewed((current) =>
      current.includes(key)
        ? current.filter((value) => value !== key)
        : [...current, key],
    );
  }

  const phaseShortcut = (id: number) => (
    <button key={id} className="text-button" onClick={() => openPhase(id)}>
      Fase {id}
    </button>
  );

  function renderContent() {
    switch (view) {
      case "library":
        return (
          <>
            <div className="toolbar">
              <label className="search-field">
                <span className="sr-only">Buscar en la biblioteca</span>
                <input
                  type="search"
                  placeholder="Buscar en la biblioteca"
                  value={search}
                  onChange={(event) => setSearch(event.target.value)}
                />
              </label>
              <PendingButton phase={4}>Agregar música</PendingButton>
            </div>
            <div
              className="tabs"
              role="tablist"
              aria-label="Vistas de biblioteca"
            >
              {[
                "Canciones",
                "Álbumes",
                "Artistas",
                "Favoritos",
                "Historial",
              ].map((tab) => (
                <button
                  key={tab}
                  role="tab"
                  id={`library-${tab}`}
                  aria-selected={libraryTab === tab}
                  tabIndex={libraryTab === tab ? 0 : -1}
                  onKeyDown={navigateTabs}
                  aria-controls="library-content"
                  onClick={() => setLibraryTab(tab)}
                >
                  {tab}
                </button>
              ))}
            </div>
            <section
              id="library-content"
              role="tabpanel"
              aria-labelledby={`library-${libraryTab}`}
            >
              <div className="list-heading">
                <span>{libraryTab}</span>
                <span>0 elementos</span>
              </div>
              {libraryTab === "Canciones" && (
                <div className="track-columns" aria-hidden="true">
                  <span>Título</span>
                  <span>Artista</span>
                  <span>Álbum</span>
                  <span>Duración</span>
                </div>
              )}
              <EmptyState
                icon="library"
                title={
                  search
                    ? `Sin resultados para “${search}”`
                    : "Tu biblioteca empieza aquí"
                }
              >
                La importación de música se conectará en la fase 4. Después
                podrás consultar canciones, álbumes, artistas y sus metadatos.
              </EmptyState>
            </section>
            <div className="module-note">
              <span>
                Importación y biblioteca: fase 4. Identidad y metadatos: fase 6.
              </span>
              <button className="text-button" onClick={() => openPhase(4)}>
                Ver comprobaciones
              </button>
            </div>
          </>
        );
      case "playlists":
        return (
          <>
            <p className="page-description">
              Organiza tu música en listas independientes de la cola.
            </p>
            <form
              className="toolbar"
              onSubmit={(event) => {
                event.preventDefault();
                const name = playlistName.trim();
                if (!name) return;
                setPlaylists((current) => [
                  ...current,
                  { id: nextPlaylistId, name },
                ]);
                setNextPlaylistId((current) => current + 1);
                setPlaylistName("");
              }}
            >
              <label className="search-field">
                <span className="sr-only">Nombre de la nueva lista</span>
                <input
                  placeholder="Nombre de la lista"
                  value={playlistName}
                  maxLength={64}
                  onChange={(event) => setPlaylistName(event.target.value)}
                  required
                />
              </label>
              <button
                className="button primary"
                disabled={!playlistName.trim()}
              >
                Crear lista de prueba
              </button>
            </form>
            <p className="inline-note">
              Las listas de prueba se conservan solo mientras esta ventana esté
              abierta. Guardar e importar canciones está pendiente de la fase 5.
            </p>
            {playlists.length ? (
              <ul className="plain-list">
                {playlists.map((item) => (
                  <li key={item.id}>
                    <span className="row-icon">
                      <Icon name="playlists" />
                    </span>
                    <div className="grow">
                      <strong>{item.name}</strong>
                      <span className="secondary">0 canciones</span>
                    </div>
                    <button
                      className="text-button"
                      aria-label={`Eliminar lista ${item.name}`}
                      onClick={() =>
                        setPlaylists((current) =>
                          current.filter((entry) => entry.id !== item.id),
                        )
                      }
                    >
                      Eliminar
                    </button>
                  </li>
                ))}
              </ul>
            ) : (
              <EmptyState icon="playlists" title="Todavía no hay listas">
                Crea una lista de prueba para revisar esta pantalla. La
                persistencia y los formatos M3U y M3U8 se implementarán más
                adelante.
              </EmptyState>
            )}
          </>
        );
      case "queue":
        return (
          <>
            <div className="toolbar">
              <p className="page-description">0 canciones en la cola</p>
              <div className="button-group">
                <PendingButton phase={5}>Guardar cola</PendingButton>
                <PendingButton phase={5}>Limpiar</PendingButton>
              </div>
            </div>
            <EmptyState icon="queue" title="No hay canciones en espera">
              Aquí podrás reordenar la reproducción, agregar la siguiente
              canción y restaurar tu última sesión.
            </EmptyState>
            <div className="module-note">
              El motor de reproducción corresponde a la fase 3. La cola y su
              restauración corresponden a la fase 5.
            </div>
          </>
        );
      case "lyrics":
        return (
          <>
            <div className="tabs" role="tablist" aria-label="Modo de letras">
              {["Normal", "Karaoke", "Minimalista", "Pantalla completa"].map(
                (mode) => (
                  <button
                    key={mode}
                    role="tab"
                    id={`lyrics-${mode.replace(/\s+/g, "-")}`}
                    aria-controls="lyrics-content"
                    aria-selected={lyricsMode === mode}
                    tabIndex={lyricsMode === mode ? 0 : -1}
                    onKeyDown={navigateTabs}
                    onClick={() => setLyricsMode(mode)}
                  >
                    {mode}
                  </button>
                ),
              )}
            </div>
            <section
              id="lyrics-content"
              role="tabpanel"
              aria-labelledby={`lyrics-${lyricsMode.replace(/\s+/g, "-")}`}
            >
              <div className="lyrics-preview">
                <Icon name="lyrics" size={30} />
                <h2>
                  {lyricsMode === "Normal"
                    ? "Las letras aparecerán aquí"
                    : `Vista de letras: ${lyricsMode.toLowerCase()}`}
                </h2>
                <p>Selecciona una canción cuando el motor esté conectado.</p>
                <span className="status">Fase 7 pendiente</span>
              </div>
            </section>
            <div className="toolbar bottom-tools">
              <label className="control-label">
                Desfase{" "}
                <input
                  aria-label="Desfase de letras en milisegundos"
                  type="number"
                  value="0"
                  disabled
                />{" "}
                ms
              </label>
              <div className="button-group">
                <PendingButton phase={7}>Buscar letras</PendingButton>
                <PendingButton phase={7}>Abrir LRC</PendingButton>
              </div>
            </div>
            <p className="inline-note">
              El selector permite revisar la navegación. La sincronización,
              animación y pantalla completa real aún no están implementadas.
            </p>
          </>
        );
      case "artwork":
        return (
          <>
            <p className="page-description">
              Portadas estáticas y animadas, con una alternativa local.
            </p>
            <div className="artwork-layout">
              <div className="artwork-placeholder">
                <Icon name="artwork" size={54} />
                <span>Sin portada</span>
              </div>
              <div>
                <h2>Orden de búsqueda previsto</h2>
                <ol className="provider-order">
                  {[
                    "Apple Music · animada",
                    "Apple Music · estática",
                    "Archive.org",
                    "YouTube",
                    "Portada local",
                    "Imagen predeterminada",
                  ].map((item) => (
                    <li key={item}>{item}</li>
                  ))}
                </ol>
                <PendingButton phase={8}>Buscar portada</PendingButton>
              </div>
            </div>
            <div className="module-note">
              Sin conexión se usarán recursos guardados y portadas locales. La
              obtención y reproducción de recursos autorizados se implementará
              en la fase 8.
            </div>
          </>
        );
      case "sources":
        return (
          <>
            <p className="page-description">
              Conecta fuentes sin depender de ellas para usar tu biblioteca
              local.
            </p>
            <ul className="plain-list">
              {[
                "Archivos locales",
                "YouTube",
                "Google Drive",
                "Dropbox",
                "OneDrive",
                "WebDAV",
              ].map((source) => (
                <li key={source}>
                  <span className="row-icon">
                    <Icon
                      name={
                        source === "Archivos locales" ? "library" : "sources"
                      }
                    />
                  </span>
                  <div className="grow">
                    <strong>{source}</strong>
                    <span className="secondary">
                      {source === "Archivos locales"
                        ? "Importación pendiente"
                        : "Sin conectar"}
                    </span>
                  </div>
                  <PendingButton phase={source === "Archivos locales" ? 4 : 9}>
                    {source === "Archivos locales"
                      ? "Agregar carpeta"
                      : "Conectar"}
                  </PendingButton>
                </li>
              ))}
            </ul>
            <p className="inline-note">
              La autenticación y las conexiones reales se implementarán en la
              fase 9.
            </p>
          </>
        );
      case "equalizer":
        return (
          <>
            <div className="toolbar">
              <p className="page-description">
                Diez bandas para ajustar el sonido.
              </p>
              <button
                className="button"
                onClick={() => setBands(Array(10).fill(0))}
              >
                Restablecer a plano
              </button>
            </div>
            <div className="equalizer">
              <div className="band-grid">
                {frequencies.map((frequency, index) => (
                  <label className="band" key={frequency}>
                    <output>
                      {bands[index] > 0 ? "+" : ""}
                      {bands[index]} dB
                    </output>
                    <input
                      aria-label={`Ganancia de ${frequency} Hz`}
                      type="range"
                      min="-12"
                      max="12"
                      step="1"
                      value={bands[index]}
                      onChange={(event) =>
                        setBands((current) =>
                          current.map((value, band) =>
                            band === index ? Number(event.target.value) : value,
                          ),
                        )
                      }
                    />
                    <span>{frequency}</span>
                  </label>
                ))}
              </div>
              <div className="eq-footer">
                <span>Frecuencia en Hz</span>
                <span>−12 a +12 dB</span>
              </div>
            </div>
            <div className="module-note">
              Vista previa de los controles. Estos valores no procesan audio ni
              se guardan todavía; se conectarán al motor en la fase 12.
            </div>
            <SettingRow
              title="Procesamiento de audio"
              description="Presets, preamplificación, ReplayGain y crossfade."
            >
              <span className="status">Pendiente</span>
            </SettingRow>
          </>
        );
      case "settings":
        return (
          <>
            <div
              className="tabs"
              role="tablist"
              aria-label="Secciones de configuración"
            >
              {settingsTabs.map((tab) => (
                <button
                  key={tab}
                  role="tab"
                  id={`settings-${tab}`}
                  aria-controls="settings-content"
                  aria-selected={settingsTab === tab}
                  tabIndex={settingsTab === tab ? 0 : -1}
                  onKeyDown={navigateTabs}
                  onClick={() => setSettingsTab(tab)}
                >
                  {tab}
                </button>
              ))}
            </div>
            <section
              id="settings-content"
              role="tabpanel"
              aria-labelledby={`settings-${settingsTab}`}
              className="settings-content"
            >
              {settingsTab === "Apariencia" && (
                <>
                  <SettingRow
                    title="Tema"
                    description="Vista previa en esta sesión. Se restablece al volver a abrir."
                  >
                    <select
                      aria-label="Tema de la interfaz"
                      value={theme}
                      onChange={(event) => setTheme(event.target.value)}
                    >
                      <option value="light">Claro</option>
                      <option value="dark">Oscuro</option>
                      <option value="system">Sistema</option>
                    </select>
                  </SettingRow>
                  <SettingRow
                    title="Idioma"
                    description="La interfaz de trabajo está disponible en español."
                  >
                    <select aria-label="Idioma" disabled>
                      <option>Español</option>
                    </select>
                  </SettingRow>
                </>
              )}
              {settingsTab === "Almacenamiento" && (
                <>
                  <SettingRow
                    title="Datos locales"
                    description="Configuración, biblioteca, listas y preferencias en archivos JSON."
                  >
                    <span className="status">Fase 2 pendiente</span>
                  </SettingRow>
                  <SettingRow
                    title="Respaldos y recuperación"
                    description="Protección frente a archivos dañados y escrituras interrumpidas."
                  >
                    <PendingButton phase={2}>Crear respaldo</PendingButton>
                  </SettingRow>
                  <SettingRow
                    title="Caché"
                    description="Límites, limpieza y ubicación de los recursos guardados."
                  >
                    <PendingButton phase={13}>Administrar</PendingButton>
                  </SettingRow>
                  <SettingRow
                    title="Importar y exportar"
                    description="Transferir datos sin incluir credenciales."
                  >
                    <PendingButton phase={2}>Exportar</PendingButton>
                  </SettingRow>
                </>
              )}
              {settingsTab === "Sistema" && (
                <>
                  <SettingRow
                    title="Controles multimedia"
                    description="Teclas del sistema, controles de reproducción y suspensión."
                  >
                    <span className="status">Fase 11 pendiente</span>
                  </SettingRow>
                  <SettingRow
                    title="Ventana y notificaciones"
                    description="Bandeja, Dock, notificaciones y comportamiento al cerrar."
                  >
                    <PendingButton phase={11}>Configurar</PendingButton>
                  </SettingRow>
                  <SettingRow
                    title="Accesibilidad"
                    description="Foco visible, navegación con teclado y adaptación de la interfaz."
                  >
                    <button
                      className="text-button"
                      onClick={() => openPhase(11)}
                    >
                      Ver comprobaciones
                    </button>
                  </SettingRow>
                </>
              )}
              {settingsTab === "Actualizaciones" && (
                <>
                  <SettingRow
                    title="Zenith Player"
                    description="Versión del proyecto: 0.1.0. Aún no se consultan versiones publicadas."
                  >
                    <PendingButton phase={14}>
                      Buscar actualizaciones
                    </PendingButton>
                  </SettingRow>
                  <SettingRow
                    title="Canal de distribución"
                    description="Los canales Stable y Beta se conectarán al actualizador."
                  >
                    <select aria-label="Canal de actualizaciones" disabled>
                      <option>Stable</option>
                      <option>Beta</option>
                    </select>
                  </SettingRow>
                  <SettingRow
                    title="Publicación v1.0"
                    description="Instaladores, firmas, documentación y validación final."
                  >
                    <button
                      className="text-button"
                      onClick={() => openPhase(16)}
                    >
                      Ver preparación
                    </button>
                  </SettingRow>
                </>
              )}
              {settingsTab === "Privacidad" && (
                <>
                  <SettingRow
                    title="Telemetría"
                    description="Esta vista previa no envía datos de uso."
                  >
                    <span className="status">Sin telemetría</span>
                  </SettingRow>
                  <SettingRow
                    title="Historial"
                    description="La gestión del historial se conectará a la biblioteca local."
                  >
                    <PendingButton phase={13}>Borrar historial</PendingButton>
                  </SettingRow>
                  <SettingRow
                    title="Credenciales"
                    description="Las conexiones utilizarán almacenamiento protegido del sistema."
                  >
                    <PendingButton phase={9}>
                      Administrar conexiones
                    </PendingButton>
                  </SettingRow>
                </>
              )}
            </section>
            <p className="inline-note">
              La persistencia de preferencias se implementará en las fases 2 y
              13.
            </p>
          </>
        );
      case "phases":
        return (
          <>
            <p className="page-description">
              Un lugar para revisar cada entrega, sin dar por terminadas las
              funciones pendientes.
            </p>
            <div className="phase-workspace">
              <nav className="phase-list" aria-label="Fases del proyecto">
                {phases.map((item) => (
                  <button
                    key={item.id}
                    className={item.id === selectedPhase ? "selected" : ""}
                    aria-current={
                      item.id === selectedPhase ? "step" : undefined
                    }
                    onClick={() => setSelectedPhase(item.id)}
                  >
                    <span className="phase-number">
                      {String(item.id).padStart(2, "0")}
                    </span>
                    <span>{item.name}</span>
                  </button>
                ))}
              </nav>
              <section className="phase-detail" aria-labelledby="phase-title">
                <div className="phase-meta">
                  <span>Fase {phase.id} de 16</span>
                  <span className="status">{phase.status}</span>
                </div>
                <h2 id="phase-title">{phase.name}</h2>
                <p>{phase.objective}</p>
                <h3>Comprobación manual</h3>
                <div className="checklist">
                  {phase.checks.map((check, index) => {
                    const key = `${phase.id}-${index}`;
                    return (
                      <label key={key} className="check-row">
                        <input
                          type="checkbox"
                          checked={reviewed.includes(key)}
                          onChange={() => toggleReview(key)}
                        />
                        <span>{check}</span>
                      </label>
                    );
                  })}
                </div>
                <p className="inline-note">
                  {
                    phase.checks.filter((_, index) =>
                      reviewed.includes(`${phase.id}-${index}`),
                    ).length
                  }{" "}
                  de {phase.checks.length} comprobaciones marcadas en esta
                  sesión. Las marcas no cambian el estado de implementación ni
                  sustituyen las pruebas.
                </p>
                {phase.view !== "phases" && (
                  <button className="button" onClick={() => openModule(phase)}>
                    Ir al módulo
                  </button>
                )}
                <div className="phase-reference">
                  Lista resumida para trabajar. Consulta el alcance completo en{" "}
                  <code>docs/roadmap.md</code>.
                </div>
              </section>
            </div>
          </>
        );
    }
  }

  return (
    <div className="workspace" data-theme={theme}>
      <a className="skip-link" href="#main-content">
        Saltar al contenido
      </a>
      <aside className="sidebar">
        <div className="brand">
          <img src="/favicon.svg" width="29" height="29" alt="" />
          <span>
            Zenith <strong>Player</strong>
          </span>
        </div>
        <nav aria-label="Navegación principal">
          {navigation.map((item) => (
            <button
              key={item.view}
              className={`nav-button ${view === item.view ? "active" : ""} ${item.view === "sources" || item.view === "phases" ? "nav-divider" : ""}`}
              aria-current={view === item.view ? "page" : undefined}
              aria-label={item.label}
              title={item.label}
              onClick={() => setView(item.view)}
            >
              <Icon name={item.view} />
              <span>{item.label}</span>
            </button>
          ))}
        </nav>
        <div className="sidebar-note">
          <span className="connection-dot" />
          Vista de trabajo<span>v0.1.0</span>
        </div>
      </aside>
      <main id="main-content" tabIndex={-1}>
        <header className="page-header">
          <div>
            <p className="page-context">Zenith Player</p>
            <h1>{title}</h1>
          </div>
          <div className="phase-shortcuts">
            {pagePhases[view].map(phaseShortcut)}
            {view !== "phases" && (
              <button
                className="button small"
                onClick={() => setView("phases")}
              >
                Ver fases
              </button>
            )}
          </div>
        </header>
        <div className="prototype-note">
          Interfaz de trabajo. Las funciones pendientes se habilitarán en sus
          fases correspondientes.
        </div>
        <div className="page-content" key={view}>
          {renderContent()}
        </div>
      </main>
      <footer className="player-bar" aria-label="Reproductor">
        <div className="now-playing">
          <div className="mini-artwork">
            <Icon name="music" />
          </div>
          <div>
            <strong>Sin reproducción</strong>
            <span>Agrega música cuando la biblioteca esté disponible</span>
          </div>
        </div>
        <div className="transport">
          <div className="transport-buttons">
            <button
              className="icon-button"
              disabled
              aria-label="Canción anterior"
            >
              <Icon name="previous" size={18} />
            </button>
            <button
              className="play-button"
              disabled
              aria-label="Reproducir: motor pendiente de la fase 3"
            >
              <Icon name="play" size={19} />
            </button>
            <button
              className="icon-button"
              disabled
              aria-label="Canción siguiente"
            >
              <Icon name="next" size={18} />
            </button>
          </div>
          <div className="progress-row">
            <span>0:00</span>
            <input
              type="range"
              aria-label="Posición de reproducción"
              min="0"
              max="100"
              value="0"
              disabled
            />
            <span>0:00</span>
          </div>
        </div>
        <div className="player-tools">
          <Icon name="volume" size={18} />
          <input
            type="range"
            aria-label="Volumen: motor de audio pendiente"
            min="0"
            max="100"
            value="70"
            disabled
          />
          <button
            className={`icon-button ${view === "queue" ? "active" : ""}`}
            aria-label="Abrir cola"
            title="Cola"
            onClick={() => setView("queue")}
          >
            <Icon name="queue" />
          </button>
        </div>
      </footer>
    </div>
  );
}
