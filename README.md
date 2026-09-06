# BoxFlux

Optimizador y conversor de imágenes para escritorio. Suelta una carpeta (o
unos archivos), el motor analiza cada imagen y decide por sí mismo qué
codec, qué parámetros y qué nivel de calidad producen el archivo más pequeño
que sigue viéndose igual al original.

No aplica recetas fijas: para cada imagen genera candidatos reales (PNG con
oxipng/zopfli, JPEG con mozjpeg, WebP, AVIF con ravif), los mide contra el
original (MSE/PSNR/SSIM + butteraugli como métrica perceptual) y puntúa
calidad, compresión, velocidad y compatibilidad antes de elegir un ganador.
Si ninguno vale la pena, conserva el original y te dice por qué.

## Stack

| Capa      | Tecnología                                  |
| --------- | ------------------------------------------- |
| Backend   | Rust + Tauri 2                              |
| Frontend  | React 18 + TypeScript + Vite + Tailwind CSS |
| Codecs    | oxipng, mozjpeg, webpx, ravif, image        |
| Métricas  | MSE / PSNR / SSIM + butteraugli             |
| Tests     | Vitest (unitarios mínimos)                  |

## Cómo ejecutarlo

Requisitos: Node.js 20+, pnpm, Rust 1.75+ (con las dependencias de sistema
de Tauri: webkit2gtk en Linux, WebView2 en Windows, WKWebView en macOS).

```bash
pnpm install          # dependencias del frontend
pnpm dev              # solo frontend (Vite)
pnpm tauri:dev        # app completa (ventana Tauri)
pnpm tauri:build      # build de producción
pnpm test             # tests unitarios
```

En `src-tauri/`: `cargo check`, `cargo clippy` y `cargo fmt --check` para
trabajar el backend.

Nota: este repositorio es una instantánea de código fuente. Los archivos de
scaffolding que genera `tauri init` (`tauri.conf.json`, `build.rs`,
`capabilities/`, `icons/`, `index.html`, `vite.config.ts`, `tsconfig.json`)
no están incluidos; si quieres levantar la app de escritorio, regenera ese
andamiaje con `pnpm tauri init` sobre este código.

## Estructura

```text
boxflux/
├── src/            # Frontend React (páginas, estado, componentes)
├── src-tauri/      # Backend Rust
│   └── src/
│       ├── commands/   # Commands de Tauri expuestos al frontend
│       ├── engine/     # Motor: análisis, estrategia, candidatos,
│       │   │           # medición de calidad y scoring
│       │   └── formats/   # Handlers por codec (PNG, JPEG, WebP, AVIF…)
│       └── queue/      # Cola de trabajos async (tokio)
└── LICENSE
```

El motor (`engine/`) no depende de Tauri: es una crate pura que se puede
probar y evolucionar por separado de la capa de UI.

## Estado del proyecto

**Discontinuado.** Este proyecto ya no recibe mantenimiento ni soporte de
ningún tipo. No hay canal de issues, ni roadmap, ni garantía de que se
respondan preguntas. Se publica tal cual, como referencia: úsalo, cópialo o
abandónalo bajo tu propio criterio.

## Licencia

MIT — ver [LICENSE](LICENSE). El software se provee "tal cual", sin
garantía de ningún tipo y con total exención de responsabilidad para los
autores. La feature opcional de PNG lossy (`png-lossy`) depende de
`imagequant`, que es GPL-3.0: habilítala solo si aceptas esa licencia.
