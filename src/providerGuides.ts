import type { ProviderId } from "./sources";

export interface ProviderGuide {
  title: string;
  website?: string;
  steps: string[];
  note?: string;
}

/** Instructions match the desktop OAuth callback and scopes in sources.rs. */
export const providerGuides: Record<ProviderId, ProviderGuide> = {
  local: {
    title: "Usar archivos locales",
    steps: [
      "Abre Biblioteca y añade una carpeta o archivos de música.",
      "Espera al análisis de metadatos. No necesitas cuenta, API ni conexión a Internet.",
    ],
  },
  "youtube-auto": {
    title: "YouTube sin API key · opción principal",
    website: "https://github.com/yt-dlp/yt-dlp",
    steps: [
      "Activa Permitir consultas externas. La aplicación incluye los componentes de búsqueda y extracción; no necesitas crear una clave ni iniciar sesión.",
      "Elige YouTube automático en el buscador y escribe un título o artista.",
      "Pulsa Reproducir. Primero se prepara el audio M4A en la caché y después comienza la reproducción; la primera carga puede tardar.",
      "Si falla, prueba otro resultado o actualiza la aplicación. Si tienes una clave configurada, YouTube API permite buscar metadatos como alternativa, pero no ofrece audio.",
    ],
    note: "No se importan cookies del navegador. Algunos videos, directos o contenidos restringidos no estarán disponibles. Usa contenido con los permisos necesarios y respeta las condiciones de la fuente.",
  },
  youtube: {
    title: "Obtener una clave de YouTube Data API",
    website: "https://console.cloud.google.com/",
    steps: [
      "Abre Google Cloud Console, inicia sesión y crea o selecciona un proyecto.",
      "En APIs y servicios → Biblioteca, busca YouTube Data API v3 y pulsa Habilitar.",
      "En APIs y servicios → Credenciales, selecciona Crear credenciales → Clave de API.",
      "Edita la clave y restringe las APIs permitidas a YouTube Data API v3. Las restricciones de sitio web no corresponden a esta aplicación de escritorio.",
      "Copia la clave en Clave de la API, activa las consultas externas y pulsa Conectar. Selecciona YouTube API para buscar.",
    ],
    note: "Solo búsqueda y metadatos, sujeto a la cuota de tu proyecto. La clave se guarda en el almacén de credenciales del sistema. No la publiques en GitHub.",
  },
  "google-drive": {
    title: "Crear las credenciales de Google Drive",
    website: "https://console.cloud.google.com/",
    steps: [
      "Crea o selecciona un proyecto de Google Cloud. En APIs y servicios → Biblioteca, habilita Google Drive API.",
      "En Google Auth Platform configura Branding y Audience. Si la aplicación está en pruebas, añade tu cuenta como usuario de prueba.",
      "En Data Access, añade el permiso https://www.googleapis.com/auth/drive.readonly. Permite leer tu Drive; no modifica archivos.",
      "En Clients → Create client, selecciona Desktop app (aplicación de escritorio). Copia el Client ID; si Google entrega un Client secret, cópialo también en el campo opcional de Zenith.",
      "Pega el ID de cliente en Google Drive y pulsa Conectar. Autoriza en el navegador y vuelve a Zenith para pulsar Ya lo autoricé, continuar.",
    ],
    note: "El retorno usa un puerto local temporal; no crees un cliente web. Google puede exigir verificación para distribuir una aplicación que use este permiso y limitar la duración de los tokens en modo de pruebas.",
  },
  dropbox: {
    title: "Crear una aplicación de Dropbox",
    website: "https://www.dropbox.com/developers/apps",
    steps: [
      "Abre Dropbox App Console → Create app → Scoped access. Elige Full Dropbox para buscar en tu cuenta, o App folder para limitar el acceso a la carpeta de la aplicación.",
      "En Permissions activa account_info.read, files.metadata.read y files.content.read. Guarda con Submit.",
      "En Settings → OAuth 2 → Redirect URIs registra exactamente http://127.0.0.1:53682/callback y pulsa Add.",
      "Copia App key en ID de cliente de Dropbox. Deja vacío el secreto: Zenith utiliza PKCE.",
      "Pulsa Conectar, autoriza en el navegador y vuelve para pulsar Ya lo autoricé, continuar.",
    ],
    note: "No necesitas generar ni pegar un access token. El puerto 53682 debe estar libre. App folder solo permite ver los archivos dentro de esa carpeta.",
  },
  onedrive: {
    title: "Registrar una aplicación de OneDrive",
    website: "https://entra.microsoft.com/",
    steps: [
      "En Microsoft Entra → App registrations, crea un registro. Para OneDrive personal selecciona una opción que incluya cuentas personales de Microsoft.",
      "En Authentication añade la plataforma Mobile and desktop applications y la dirección de retorno http://localhost/callback. Zenith utiliza un puerto local temporal.",
      "En API permissions → Microsoft Graph → Delegated permissions añade Files.Read y offline_access. Aplica el consentimiento del administrador si tu organización lo exige.",
      "Copia Application (client) ID desde Overview al campo ID de cliente de OneDrive. No crees ni pegues un secreto para este cliente público.",
      "Pulsa Conectar, acepta los permisos en el navegador y vuelve para pulsar Ya lo autoricé, continuar.",
    ],
    note: "Desconectar borra las credenciales locales. Para retirar el consentimiento remoto, elimina el permiso desde tu cuenta de Microsoft.",
  },
  webdav: {
    title: "Conectar un servidor WebDAV",
    website:
      "https://docs.nextcloud.com/server/latest/user_manual/en/files/access_webdav.html",
    steps: [
      "En tu servicio de almacenamiento busca la dirección WebDAV. Por ejemplo, Nextcloud la muestra en Archivos → Ajustes de archivos → WebDAV.",
      "Copia la dirección HTTPS completa, incluida la ruta de tu usuario o carpeta raíz.",
      "Si tu servicio ofrece contraseñas de aplicación, crea una en Seguridad. Usa esa contraseña en lugar de la principal.",
      "Introduce Dirección, Usuario y Contraseña en Zenith y pulsa Conectar. Después usa Recorrer desde la raíz.",
    ],
    note: "WebDAV no requiere una API key. Las instrucciones exactas para crear credenciales dependen de tu servidor.",
  },
};
