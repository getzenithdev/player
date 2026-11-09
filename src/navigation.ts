export const navigation = [
  { view: "home", label: "Inicio", group: "main" },
  { view: "search", label: "Buscar", group: "main" },
  { view: "library", label: "Canciones", group: "library" },
  { view: "albums", label: "Álbumes", group: "library" },
  { view: "artists", label: "Artistas", group: "library" },
  { view: "favorites", label: "Favoritos", group: "library" },
  { view: "playlists", label: "Mis listas", group: "library" },
  { view: "history", label: "Historial", group: "library" },
  { view: "downloads", label: "Descargas", group: "collection" },
  { view: "sources", label: "Fuentes", group: "collection" },
  { view: "equalizer", label: "Ecualizador", group: "settings" },
  { view: "settings", label: "Configuración", group: "settings" },
] as const;
export type AppView =
  | (typeof navigation)[number]["view"]
  | "nowPlaying"
  | "genres"
  | "queue"
  | "lyrics"
  | "artwork"
  | "albumDetail"
  | "artistDetail";
export interface CollectionRoute {
  kind: "album" | "artist";
  key: string;
  title: string;
}
export interface LibraryLocation {
  view: import("./library").LibraryView;
  query: string;
}
export const viewTitle = (view: AppView) =>
  navigation.find((item) => item.view === view)?.label ??
  (
    {
      nowPlaying: "Ahora suena",
      genres: "Géneros",
      queue: "Cola de reproducción",
      lyrics: "Letras",
      artwork: "Portada de la canción",
      albumDetail: "Álbum",
      artistDetail: "Artista",
    } as Record<string, string>
  )[view];
