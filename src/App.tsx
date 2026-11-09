import {
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
  type CSSProperties,
} from "react";
import { pickAudioFiles, useAudioEngine } from "./audio";
import { accessibilityAttributes, playbackAnnouncement } from "./accessibility";
import { Icon } from "./Icon";
import { navigation, viewTitle, type AppView } from "./navigation";
import { hasNativeStorage, loadAppearance, saveAppearance } from "./storage";
import {
  defaultIntegrationSettings,
  loadIntegration,
  normalizeIntegration,
  saveIntegration,
  subscribeNativeNotices,
  type IntegrationSettings,
  type IntegrationSnapshot,
} from "./native";
import { useGlobalShortcuts } from "./shortcuts";
import { Library } from "./LibraryPanel";
import { Playlists } from "./PlaylistsPanel";
import { QueuePanel } from "./QueuePanel";
import { Artwork } from "./ArtworkPanel";
import { Lyrics } from "./LyricsPanel";
import { ListeningHistory } from "./ListeningHistory";
import { Sources } from "./SourcesPanel";
import { Equalizer } from "./EqualizerPanel";
import { Settings } from "./SettingsPanel";
import { Home, Search, Downloads } from "./MusicScreens";
import { PlayerBar } from "./PlayerBar";
import { ExpandedPlayer } from "./ExpandedPlayer";
import { checkForUpdate } from "./updater";
import { useMusicLibrary } from "./useMusicLibrary";
import { libraryAction, type LibraryView } from "./library";
import "./App.css";
import "./Experience.css";
import "./Refinement.css";
import "./Navigation.css";

const libraryRoutes: Partial<Record<AppView, LibraryView>> = {
  library: "Canciones",
  albums: "Álbumes",
  artists: "Artistas",
  genres: "Géneros",
  favorites: "Favoritos",
};

export default function App() {
  const [view, setView] = useState<AppView>("home");
  const [playerExpanded, setPlayerExpanded] = useState(false);
  const [sidebarCollapsed, setSidebarCollapsed] = useState(() => {
    try {
      return (
        localStorage.getItem("zenith.sidebar.collapsed") === "true" ||
        window.matchMedia("(max-width: 900px)").matches
      );
    } catch {
      return false;
    }
  });
  function toggleSidebar() {
    setSidebarCollapsed((collapsed) => {
      const next = !collapsed;
      try {
        localStorage.setItem("zenith.sidebar.collapsed", String(next));
      } catch {
        // The control remains usable when browser storage is unavailable.
      }
      return next;
    });
  }
  const [theme, setTheme] = useState("violet");
  const [query, setQuery] = useState("");
  const [libraryQuery, setLibraryQuery] = useState("");
  const [picking, setPicking] = useState(false);
  const [themeError, setThemeError] = useState("");
  const [integrationError, setIntegrationError] = useState("");
  const [integration, setIntegration] = useState<IntegrationSettings>(
    defaultIntegrationSettings,
  );
  const [integrationSnapshot, setIntegrationSnapshot] =
    useState<IntegrationSnapshot | null>(null);
  const [nativeNotice, setNativeNotice] = useState("");
  const [accent, setAccent] = useState<string | undefined>();
  const main = useRef<HTMLElement>(null);
  const search = useRef<HTMLInputElement>(null);
  const audio = useAudioEngine();
  const { library, loading, error, reload } = useMusicLibrary();
  const current = useMemo(
    () =>
      library.tracks.find(
        (t) =>
          t.id === audio.state.track?.trackId ||
          t.local_path === audio.state.track?.path,
      ),
    [library.tracks, audio.state.track?.trackId, audio.state.track?.path],
  );
  const favoriteCount = useMemo(
    () => library.tracks.filter((track) => track.favorite).length,
    [library.tracks],
  );
  function navigate(next: AppView) {
    if (window.matchMedia("(max-width: 900px)").matches) {
      setSidebarCollapsed(true);
    }
    if (next === "nowPlaying") {
      setPlayerExpanded(true);
      return;
    }
    setPlayerExpanded(false);
    setView(next);
    setLibraryQuery("");
    main.current?.scrollTo({ top: 0 });
    main.current?.focus({ preventScroll: true });
  }
  useEffect(() => {
    document.title = `${viewTitle(view)} · Zenith Player`;
  }, [view]);
  useEffect(() => {
    let disposed = false;
    void loadAppearance()
      .then((appearance) => {
        if (!disposed && appearance) setTheme(appearance.theme);
      })
      .catch((reason) => {
        if (!disposed) setThemeError(String(reason));
      });
    return () => {
      disposed = true;
    };
  }, []);
  useEffect(() => {
    const shortcut = (event: KeyboardEvent) => {
      if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === "k") {
        event.preventDefault();
        search.current?.focus();
      }
    };
    window.addEventListener("keydown", shortcut);
    return () => window.removeEventListener("keydown", shortcut);
  }, []);
  useEffect(() => {
    let disposed = false;
    void loadIntegration()
      .then((snapshot) => {
        if (disposed || !snapshot) return;
        setIntegration(snapshot.settings);
        setIntegrationSnapshot(snapshot);
      })
      .catch((reason) => {
        if (!disposed) setIntegrationError(String(reason));
      });
    return () => {
      disposed = true;
    };
  }, []);
  useEffect(() => {
    let disposed = false;
    let unlisten: (() => void) | undefined;
    void subscribeNativeNotices(setNativeNotice).then((stop) => {
      if (disposed) stop();
      else unlisten = stop;
    });
    return () => {
      disposed = true;
      unlisten?.();
    };
  }, []);
  useEffect(() => {
    if (!nativeNotice) return;
    const timer = window.setTimeout(() => setNativeNotice(""), 6000);
    return () => window.clearTimeout(timer);
  }, [nativeNotice]);
  useEffect(() => {
    let disposed = false;
    void checkForUpdate()
      .then(async (available) => {
        if (!disposed && available) {
          const version = available.update.version;
          await available.update.close();
          setNativeNotice(`Hay una actualización disponible: ${version}.`);
        } else {
          await available?.update.close();
        }
      })
      .catch(() => {
        // The settings screen reports errors for explicit checks.
      });
    return () => {
      disposed = true;
    };
  }, []);
  const trackTitle = audio.state.track?.title ?? null;
  const trackArtist = audio.state.track?.artist ?? null;
  const trackAlbum = audio.state.track?.album ?? null;
  const playbackMessage = playbackAnnouncement(
    trackTitle === null
      ? null
      : { title: trackTitle, artist: trackArtist, album: trackAlbum },
    audio.state.status,
  );
  const announcement = nativeNotice
    ? `${playbackMessage}. ${nativeNotice}`
    : playbackMessage;
  const {
    toggle: toggleAudio,
    next: nextAudio,
    previous: previousAudio,
    seek: seekAudio,
    setMuted: setAudioMuted,
  } = audio;
  const togglePlayback = useCallback(() => void toggleAudio(), [toggleAudio]);
  const playNext = useCallback(() => void nextAudio(), [nextAudio]);
  const playPrevious = useCallback(() => void previousAudio(), [previousAudio]);
  const seekTo = useCallback(
    (positionMs: number) => void seekAudio(positionMs),
    [seekAudio],
  );
  const mute = useCallback(
    (muted: boolean) => void setAudioMuted(muted),
    [setAudioMuted],
  );
  useGlobalShortcuts(
    {
      toggle: togglePlayback,
      next: playNext,
      previous: playPrevious,
      seek: seekTo,
      setMuted: mute,
      muted: audio.state.muted,
      positionMs: audio.state.positionMs,
      durationMs: audio.state.durationMs,
    },
    audio.available,
  );
  async function changeTheme(next: string) {
    setTheme(next);
    setThemeError("");
    if (hasNativeStorage())
      try {
        await saveAppearance(next);
      } catch (reason) {
        setThemeError(String(reason));
      }
  }
  async function changeIntegration(next: IntegrationSettings) {
    const previous = integration;
    setIntegration(normalizeIntegration(next));
    setIntegrationError("");
    try {
      const snapshot = await saveIntegration(next);
      if (!snapshot) return;
      setIntegration(snapshot.settings);
      setIntegrationSnapshot(snapshot);
      if (snapshot.notices.length)
        setNativeNotice(snapshot.notices[snapshot.notices.length - 1]);
    } catch (reason) {
      setIntegration(previous);
      setIntegrationError(String(reason));
    }
  }
  async function openFiles() {
    if (!audio.available) return;
    setPicking(true);
    try {
      const paths = await pickAudioFiles();
      if (paths.length) await audio.openFiles(paths);
    } catch (reason) {
      audio.reportError(reason);
    } finally {
      setPicking(false);
    }
  }
  async function favorite() {
    const playing = audio.state.track;
    if (!playing) return;
    try {
      if (current) {
        await libraryAction({
          kind: "favorite",
          id: current.id,
          favorite: !current.favorite,
        });
      } else {
        await libraryAction({ kind: "favoriteLocal", path: playing.path });
      }
      await reload();
    } catch (reason) {
      audio.reportError(reason);
    }
  }
  function content() {
    const shared = { audio, tracks: library.tracks, navigate };
    if (libraryRoutes[view])
      return (
        <Library
          audio={audio}
          initialView={libraryRoutes[view]}
          initialQuery={libraryQuery}
        />
      );
    switch (view) {
      case "home":
        return <Home {...shared} onImport={() => navigate("library")} />;
      case "search":
        return (
          <Search
            {...shared}
            query={query}
            onQuery={setQuery}
            onAlbum={(text, next) => {
              setLibraryQuery(text);
              setView(next);
            }}
          />
        );
      case "playlists":
        return <Playlists audio={audio} />;
      case "queue":
        return <QueuePanel audio={audio} />;
      case "history":
        return <ListeningHistory audio={audio} />;
      case "lyrics":
        return <Lyrics audio={audio} />;
      case "artwork":
        return <Artwork audio={audio} />;
      case "sources":
        return (
          <Sources
            audio={audio}
            initialQuery={query}
            onOpenLibrary={() => navigate("library")}
          />
        );
      case "downloads":
        return <Downloads audio={audio} navigate={navigate} />;
      case "equalizer":
        return <Equalizer audio={audio} />;
      case "settings":
        return (
          <Settings
            audio={audio}
            theme={theme}
            onTheme={(next) => void changeTheme(next)}
            onSources={() => navigate("sources")}
            onEqualizer={() => navigate("equalizer")}
            integration={integration}
            integrationSnapshot={integrationSnapshot}
            onIntegration={(next) => void changeIntegration(next)}
          />
        );
    }
  }
  return (
    <div
      className={`workspace ${sidebarCollapsed ? "sidebar-collapsed" : "sidebar-expanded"}`}
      data-theme={theme}
      {...accessibilityAttributes(integration)}
      style={{ "--artwork-color": accent } as CSSProperties}
      onLoad={(event) => {
        const image = event.target;
        if (
          !(image instanceof HTMLImageElement) ||
          image.classList.contains("fallback-mark") ||
          !image.closest(".now-art, .hero-art")
        )
          return;
        try {
          const canvas = document.createElement("canvas");
          canvas.width = 1;
          canvas.height = 1;
          const ctx = canvas.getContext("2d");
          ctx?.drawImage(image, 0, 0, 1, 1);
          const pixel = ctx?.getImageData(0, 0, 1, 1).data;
          if (pixel) setAccent(`rgb(${pixel[0]} ${pixel[1]} ${pixel[2]})`);
        } catch {
          /* Missing artwork colors keep the brand palette. */
        }
      }}
    >
      <a className="skip-link" href="#main-content">
        Saltar al contenido
      </a>
      <p
        className="sr-only"
        role="status"
        aria-live="polite"
        aria-atomic="true"
      >
        {announcement}
      </p>
      {!sidebarCollapsed && (
        <button
          className="sidebar-backdrop"
          aria-label="Cerrar navegación"
          onClick={toggleSidebar}
        />
      )}
      <aside
        className="sidebar"
        id="sidebar"
        onKeyDown={(event) => {
          if (
            event.key === "Escape" &&
            window.matchMedia("(max-width: 900px)").matches
          ) {
            setSidebarCollapsed(true);
            main.current
              ?.querySelector<HTMLButtonElement>(".sidebar-mobile-toggle")
              ?.focus();
          }
        }}
      >
        <button
          className="brand"
          onClick={() => navigate("home")}
          aria-label="Zenith, inicio"
        >
          <img src="/favicon.svg" width="36" height="36" alt="" />
          <span>Zenith</span>
        </button>
        <button
          className="sidebar-toggle"
          onClick={toggleSidebar}
          aria-label={
            sidebarCollapsed
              ? "Expandir barra lateral"
              : "Retraer barra lateral"
          }
          title={
            sidebarCollapsed
              ? "Expandir barra lateral"
              : "Retraer barra lateral"
          }
          aria-expanded={!sidebarCollapsed}
          aria-controls="sidebar-navigation"
        >
          <Icon name={sidebarCollapsed ? "sidebarExpand" : "sidebarCollapse"} />
          <span>Retraer menú</span>
        </button>
        <nav id="sidebar-navigation" aria-label="Navegación principal">
          {["main", "library", "collection", "settings"].map((group) => (
            <div className={`nav-group nav-group-${group}`} key={group}>
              {group === "library" && <p className="nav-heading">Biblioteca</p>}
              {navigation
                .filter((item) => item.group === group)
                .map((item) => (
                  <button
                    className={`nav-button ${view === item.view ? "active" : ""}`}
                    key={item.view}
                    title={item.label}
                    aria-label={item.label}
                    aria-current={view === item.view ? "page" : undefined}
                    onClick={() => navigate(item.view)}
                  >
                    <Icon name={item.view} size={19} />
                    <span>{item.label}</span>
                    <span className="sidebar-tooltip" aria-hidden="true">
                      {item.label}
                    </span>
                    {item.view === "favorites" && favoriteCount > 0 && (
                      <small>{favoriteCount}</small>
                    )}
                  </button>
                ))}
            </div>
          ))}
        </nav>
      </aside>
      <main ref={main} id="main-content" tabIndex={-1}>
        <header className="page-header">
          <button
            className="sidebar-mobile-toggle icon-button"
            onClick={toggleSidebar}
            aria-label="Abrir navegación"
            aria-expanded={!sidebarCollapsed}
            aria-controls="sidebar"
          >
            <Icon name="sidebarExpand" />
          </button>
          <div className="page-heading">
            <h1>{viewTitle(view)}</h1>
          </div>
          <form
            className="global-search"
            role="search"
            onSubmit={(event) => {
              event.preventDefault();
              navigate("search");
            }}
          >
            <Icon name="search" size={17} />
            <input
              ref={search}
              type="search"
              aria-label="Buscar en Zenith"
              placeholder="Buscar en tu música"
              value={query}
              onChange={(event) => setQuery(event.target.value)}
            />
            <kbd>Ctrl K</kbd>
          </form>
          <button
            className="header-add icon-button"
            disabled={!audio.available || picking}
            onClick={() => void openFiles()}
            title="Abrir archivos de audio"
            aria-label="Abrir archivos de audio"
          >
            <Icon name="plus" />
          </button>
        </header>
        <div className="page-content" key={`${view}-${libraryQuery}`}>
          {loading && (
            <p className="inline-note" role="status">
              Abriendo tu biblioteca…
            </p>
          )}
          {(error || themeError || integrationError) && (
            <p className="error-note" role="alert">
              {error || themeError || integrationError}
            </p>
          )}
          {(audio.error || audio.state.error) && (
            <div className="playback-alert" role="alert">
              <span>{audio.error || audio.state.error}</span>
              <button
                className="text-button"
                onClick={() => void audio.restartOutput()}
              >
                Reintentar salida
              </button>
            </div>
          )}
          {content()}
        </div>
      </main>
      <PlayerBar
        audio={audio}
        current={current}
        navigate={navigate}
        onOpenFiles={() => void openFiles()}
        onFavorite={() => void favorite()}
      />
      {playerExpanded && (
        <ExpandedPlayer
          audio={audio}
          lightweightMotion={
            integrationSnapshot?.lightweightPlayerMotion ?? false
          }
          current={current}
          navigate={navigate}
          onOpenFiles={() => void openFiles()}
          onFavorite={() => void favorite()}
          onMinimize={() => setPlayerExpanded(false)}
        />
      )}
    </div>
  );
}
