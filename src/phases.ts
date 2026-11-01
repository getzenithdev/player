export type View =
  | "library"
  | "playlists"
  | "queue"
  | "history"
  | "lyrics"
  | "artwork"
  | "sources"
  | "equalizer"
  | "settings"
  | "phases";

export interface Phase {
  id: number;
  name: string;
  objective: string;
  view: View;
  status: "Base disponible" | "En pruebas" | "Vista previa" | "Pendiente";
  checks: string[];
}

export const phases: Phase[] = [
  {
    id: 1,
    name: "Entorno y proyecto base",
    objective:
      "Preparar una aplicación nativa compilable y las herramientas de desarrollo.",
    view: "phases",
    status: "Base disponible",
    checks: [
      "Abrir la ventana nativa en cada plataforma objetivo.",
      "Ejecutar tipos, lint, formato y comprobaciones de Rust.",
      "Comprobar las compilaciones en CI.",
    ],
  },
  {
    id: 2,
    name: "Almacenamiento local",
    objective:
      "Conservar datos y preferencias en archivos locales, sin bases de datos.",
    view: "settings",
    status: "En pruebas",
    checks: [
      "Guardar una preferencia y recuperarla tras reiniciar.",
      "Recuperar un JSON dañado desde un respaldo válido.",
      "Importar y exportar datos sin incluir credenciales.",
      "Verificar la ruta de almacenamiento y el modo portátil.",
    ],
  },
  {
    id: 3,
    name: "Motor de audio",
    objective: "Reproducir archivos locales de forma estable y sin conexión.",
    view: "queue",
    status: "Base disponible",
    checks: [
      "Reproducir, pausar, detener, cambiar de pista y buscar una posición.",
      "Ajustar volumen, silencio, repetición y aleatorio.",
      "Verificar MP3, FLAC, WAV, OGG, AIFF, AAC/ALAC y M4A con archivos de referencia.",
      "Reiniciar la salida tras suspensión o cambio de dispositivo.",
    ],
  },
  {
    id: 4,
    name: "Biblioteca musical",
    objective:
      "Importar, organizar y mantener actualizados los archivos musicales.",
    view: "library",
    status: "En pruebas",
    checks: [
      "Importar una carpeta dos veces sin duplicar sus entradas.",
      "Detectar archivos agregados, modificados, movidos y eliminados.",
      "Editar metadatos sin alterar originales sin autorización.",
    ],
  },
  {
    id: 5,
    name: "Listas, cola e historial",
    objective:
      "Organizar listas y una cola independiente que se pueda restaurar.",
    view: "playlists",
    status: "En pruebas",
    checks: [
      "Crear, editar, reordenar e intercambiar listas.",
      "Modificar la cola sin cambiar la lista de origen.",
      "Restaurar la sesión y verificar el historial.",
    ],
  },
  {
    id: 6,
    name: "Metadatos e identidad",
    objective:
      "Identificar pistas de distintas fuentes sin confundir sus versiones.",
    view: "library",
    status: "Pendiente",
    checks: [
      "Conservar la identidad interna al reiniciar o mover archivos.",
      "Distinguir versiones en vivo, remixes y artistas homónimos.",
      "Aceptar metadatos incompletos sin perder canciones.",
    ],
  },
  {
    id: 7,
    name: "Letras",
    objective: "Buscar, guardar y mostrar letras con o sin sincronización.",
    view: "lyrics",
    status: "Pendiente",
    checks: [
      "Leer letras locales, embebidas y de proveedores compatibles.",
      "Mantener la sincronía al pausar, buscar y ajustar el desfase.",
      "Verificar los cuatro modos y la consulta sin conexión.",
    ],
  },
  {
    id: 8,
    name: "Portadas",
    objective: "Encontrar portadas válidas y conservar alternativas locales.",
    view: "artwork",
    status: "Pendiente",
    checks: [
      "Comprobar el orden de proveedores y la alternativa local.",
      "Rechazar imágenes inválidas y conservar los recursos permitidos.",
      "Detener animaciones fuera de vista y respetar movimiento reducido.",
    ],
  },
  {
    id: 9,
    name: "Fuentes externas",
    objective:
      "Conectar servicios independientes mediante sus capacidades autorizadas.",
    view: "sources",
    status: "Pendiente",
    checks: [
      "Conectar, consultar y desconectar cada proveedor.",
      "Probar reproducción y descarga cuando estén permitidas.",
      "Gestionar tokens caducados y pérdida de red sin afectar lo local.",
    ],
  },
  {
    id: 10,
    name: "Interfaz y experiencia",
    objective:
      "Conectar las pantallas, los controles y los estados de cada módulo.",
    view: "library",
    status: "Vista previa",
    checks: [
      "Recorrer los flujos principales con teclado y ratón.",
      "Revisar estados vacíos, de carga y de error.",
      "Comprobar temas, tamaños de ventana y pantalla completa.",
    ],
  },
  {
    id: 11,
    name: "Integración nativa",
    objective: "Integrar controles del sistema y una experiencia accesible.",
    view: "settings",
    status: "Pendiente",
    checks: [
      "Controlar la reproducción mediante las teclas y controles del sistema.",
      "Verificar notificaciones, bandeja o Dock según la plataforma.",
      "Probar lectores de pantalla, foco, escalado y suspensión.",
    ],
  },
  {
    id: 12,
    name: "Ecualizador y audio",
    objective:
      "Aplicar procesamiento sin cortes, saturación ni bloqueos de interfaz.",
    view: "equalizer",
    status: "Pendiente",
    checks: [
      "Comprobar las diez bandas con señales de referencia.",
      "Aplicar presets, preamplificación y balance sin cortes.",
      "Guardar ajustes y medir la carga del procesamiento.",
    ],
  },
  {
    id: 13,
    name: "Configuración",
    objective: "Centralizar, validar y conservar todas las preferencias.",
    view: "settings",
    status: "Pendiente",
    checks: [
      "Guardar y restaurar los ajustes de cada sección.",
      "Rechazar valores inválidos y comprobar su efecto real.",
      "Limpiar la caché sin borrar biblioteca ni archivos originales.",
    ],
  },
  {
    id: 14,
    name: "Actualizaciones",
    objective: "Distribuir y verificar versiones por plataforma y canal.",
    view: "settings",
    status: "Pendiente",
    checks: [
      "Detectar y aplicar una versión de prueba conservando los datos.",
      "Rechazar firmas inválidas y paquetes incompatibles.",
      "Probar canales, descargas interrumpidas y recuperación.",
    ],
  },
  {
    id: 15,
    name: "Pruebas y calidad",
    objective:
      "Validar el conjunto y registrar resultados reales de compatibilidad.",
    view: "phases",
    status: "Pendiente",
    checks: [
      "Ejecutar los flujos críticos y las pruebas de fallos de almacenamiento.",
      "Medir rendimiento y reproducción prolongada.",
      "Revisar seguridad y cerrar los defectos que impidan el lanzamiento.",
    ],
  },
  {
    id: 16,
    name: "Publicación v1.0",
    objective:
      "Preparar una distribución instalable, verificada y documentada.",
    view: "settings",
    status: "Pendiente",
    checks: [
      "Instalar los paquetes finales en entornos limpios.",
      "Verificar firmas, actualizaciones y enlaces de descarga.",
      "Publicar manuales, licencias y notas de la versión al autorizar el lanzamiento.",
    ],
  },
];
