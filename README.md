# Glyph

> Every character matters.

Editor de código nativo construido en Rust: renderizado GPU, syntax highlighting con tree-sitter, cliente LSP asíncrono y un sistema de plugins híbrido Lua/WASM.

![License](https://img.shields.io/badge/license-Apache%202.0-blue.svg)
![Rust](https://img.shields.io/badge/rust-1.75%2B-orange.svg)
![Status](https://img.shields.io/badge/status-milestone%203%20avanzado-green.svg)

---

## ¿Qué es Glyph?

Glyph es un editor de código de escritorio multiplataforma construido desde cero en Rust, con renderizado GPU, soporte nativo de LSP y un sistema de plugins que no te obliga a aprender un lenguaje específico.

## Filosofía

- **El core hace lo mínimo** — sin dependencias de UI, sin I/O; todo lo demás es una capa encima
- **Los plugins son ciudadanos de primera clase** — usan la misma API que el editor internamente
- **Ecosistema abierto** — escribe plugins en Lua, Rust (WASM), Go, C, Zig o cualquier lenguaje que compile a WASM
- **La IA es un plugin más** — soporta múltiples providers sin estar hardcodeada en el core

---

## Stack técnico

| Componente | Librería | Versión | Razón |
|---|---|---|---|
| Renderizado GPU | `wgpu` | 0.19 | API gráfica cross-platform (Vulkan / Metal / DX12 / WebGPU) |
| Ventana y eventos | `winit` | 0.29 | Event loop nativo, soporte Wayland/X11/macOS/Windows |
| Renderizado de texto | `glyphon` | 0.5 | Puente `cosmic-text` ↔ `wgpu`; ligaduras, RTL, Unicode |
| Motor de fuentes | `cosmic-text` | 0.10 | Shaping profesional; dependencia transitiva de glyphon |
| Buffer de texto | `ropey` | 1.6 | Estructura Rope — inserción/borrado O(log n) en archivos grandes |
| Syntax highlighting | `tree-sitter` + `tree-sitter-highlight` | 0.22 | El mismo motor que Neovim y Zed; queries reutilizables |
| Gramática Rust | `tree-sitter-rust` | 0.21 | Compatible con tree-sitter 0.22 |
| Gramática JavaScript | `tree-sitter-javascript` | 0.21 | Compatible con tree-sitter 0.22 |
| Gramática Python | `tree-sitter-python` | 0.21 | Compatible con tree-sitter 0.22 |
| Tipos LSP | `lsp-types` | 0.95 | Mensajes JSON-RPC estándar para el cliente LSP |
| Runtime async | `tokio` | 1 | El cliente LSP corre en un hilo independiente |
| Serialización | `serde` + `serde_json` | 1 | Mensajes LSP, metadata de plugins WASM y configuración |
| Logging | `tracing` + `tracing-subscriber` | 0.1 / 0.3 | Niveles de log configurables con `RUST_LOG` |
| Errores | `anyhow` | 1 | Propagación ergonómica de errores en código de aplicación |
| Runtime de plugins Lua | `mlua` | 0.9 | Lua embebido (vendored); sandbox granular por permisos |
| Runtime de plugins WASM | `wasmtime` | 20 | Sandbox seguro para plugins compilados a `wasm32-unknown-unknown` |

---

## Arquitectura

### Grafo de dependencias entre crates

```
glyph-app  (binario)
  ├── glyph-core       — núcleo puro (sin UI, sin async, sin I/O)
  ├── glyph-renderer   — ventana + GPU + event loop
  └── glyph-lsp        — cliente LSP asíncrono (tokio)
```

Los crates no tienen dependencias circulares. `glyph-core` no conoce ni al renderer ni al LSP.

### glyph-core

El núcleo del editor. Contiene:

- **`Buffer`** — texto almacenado en una `Rope` de `ropey`. Todas las operaciones de edición son O(log n). Guarda la ruta del archivo, codificación y si hay cambios sin guardar.
- **`Cursor` / `Posicion`** — posición actual del cursor (línea + columna). Soporta múltiples cursores en la estructura pero la app usa uno por ahora.
- **`Document`** — orquesta `Buffer` + `cursores` + `Historia`. Es el único punto de entrada para operaciones de edición (`insertar_en_cursor`, `borrar_antes_cursor`, `mover_cursor_*`, `deshacer`, `rehacer`).
- **`Historia`** — undo/redo mediante una pila de `Operacion` (inserciones y borrados con posición y texto).
- **`Resaltador`** — wrappea `tree-sitter-highlight`. Recibe texto en bruto y devuelve `Vec<SpanSintactico>` con rangos de bytes y tipo semántico (`PalabraClave`, `Funcion`, `Comentario`, etc.). La gramática activa se elige por extensión de archivo.

### glyph-renderer

Toda la lógica de presentación. No importa ningún tipo de `glyph-core`.

- **`ContenidoRender`** — DTO (*Data Transfer Object*) que `glyph-app` construye a partir del `Document` y pasa al renderer. Contiene líneas de texto, posición del cursor, `Vec<SpanTexto>` (spans con `ColorRender` ya resuelto), `Vec<DiagnosticoRender>`, `matches_busqueda: Vec<(usize, usize)>`, `match_activo: Option<usize>` y `barra_estado: String`.
- **`ContextoGpu`** — inicializa `wgpu` (instancia, adaptador, dispositivo, superficie). La ventana vive en un `Arc<Window>` para que `Surface<'static>` sea válida.
- **`RendererTexto`** — convierte `ContenidoRender` en llamadas a `glyphon`. Gestiona tres buffers: gutter de números de línea (alineado al scroll del editor), editor principal y barra de estado. La línea activa del cursor se resalta en el gutter. Usa un algoritmo de **barrido de fronteras** para asignar colores por prioridad: `COLOR_TEXTO` < sintaxis < match inactivo < match activo < diagnóstico < cursor.
- **`Renderer`** — event loop de `winit` + render pass de `wgpu`. Gestiona tres modos internos (`Normal`, `Busqueda`, `Reemplazo`) para enrutar teclas correctamente. En modo reemplazo, `Tab` alterna entre el campo de búsqueda y el campo de sustitución; `Enter` reemplaza el match activo; `Ctrl+H` reemplaza todos. Los clicks de ratón en modo Normal convierten coordenadas de píxel a `MoverCursorA { linea, columna }`. `Ctrl+K` emite `PedirHover` para solicitar información LSP. Emite `EventoEditor` al manejador de la app.
- **`EventoEditor`** — enum que describe la intención del usuario. Edición: `InsertarTexto`, `BorrarAtras/Adelante`, `MoverCursor(Direccion)`, `Deshacer`, `Rehacer`, `Guardar`. Búsqueda: `IniciarBusqueda`, `ActualizarBusqueda`, `SiguienteMatch`, `MatchAnterior`, `TerminarBusqueda`. Reemplazo: `IniciarReemplazo`, `ActualizarReemplazo`, `ReemplazarMatch`, `ReemplazarTodo`. Ratón: `MoverCursorA { linea, columna }`. LSP: `PedirHover`.

### glyph-lsp

Cliente JSON-RPC asíncrono para servidores LSP.

- **`transporte`** — lee y escribe el framing `Content-Length:\r\n\r\n<json>` sobre stdin/stdout del proceso servidor.
- **`ClienteLsp`** — se conecta a cualquier servidor LSP (por defecto `rust-analyzer`) lanzándolo como proceso hijo. Internamente usa dos tareas Tokio:
  - *Tarea escritora*: consume un canal y escribe mensajes al stdin del servidor.
  - *Tarea lectora*: lee del stdout, despacha respuestas a peticiones pendientes (via `oneshot`) y emite notificaciones al canal `rx_notificacion`.
- **`Notificacion`** — actualmente: `Diagnosticos(PublishDiagnosticsParams)`. Más notificaciones en Milestone 3.

### glyph-plugin-api

Contratos públicos del SDK. Define el trait `Plugin` con hooks opcionales (`inicializar`, `al_cambiar`, `al_guardar`, `al_abrir`) y `AccionPlugin` (lo que un plugin puede devolver al editor: `EstablecerTema`, `LogMensaje`). Sin dependencias de UI.

El trait incluye `fn permisos(&self) -> Permisos` (defecto: solo `ui`). La struct `Permisos` tiene cinco capacidades: `ui`, `leer_archivos`, `escribir_archivos`, `ejecutar_procesos`, `red`. El plugin las declara en su tabla Lua como `M.permisos = { ui = true }`.

### glyph-plugin-host

Orquesta el ciclo de vida de todos los plugins. `HostPlugins`:
- `cargar_lua(nombre, script)` — carga un script Lua, lee `M.permisos`, aplica sandbox y registra el plugin
- `cargar_wasm(ruta)` — carga un archivo `.wasm`, lee metadatos vía `glyph_metadata()` y registra el plugin
- `cargar_wasm_bytes(nombre, bytes)` — variante en memoria para tests y embed
- `inicializar()` — llama `Plugin::inicializar()` en todos los plugins y aplica sus acciones verificando permisos
- `color(clave)` — devuelve el color RGB activo para una clave semántica (`"keyword"`, `"string"`, etc.)

**Sandbox Lua** — `PluginLua` lee los permisos declarados antes del sandbox, luego elimina de `lua.globals()` las APIs no declaradas: `io`/`dofile`/`loadfile` sin `leer_archivos`, `require`/`load` sin `leer_archivos`, y `os.execute/exit/remove/rename` sin `ejecutar_procesos`.

**ABI WASM** — `PluginWasm` usa `wasmtime` con el core API (no component model). El plugin WASM debe exportar:
- `glyph_alloc(size: i32) -> i32` — el host escribe strings aquí
- `glyph_metadata() -> i32` — puntero a JSON `[u32-LE len][UTF-8]` con nombre y permisos
- `glyph_inicializar() -> i32`, `glyph_al_abrir/cambiar/guardar(...)  -> i32` — listas de `AccionJson`

El host provee al módulo `env::glyph_log(ptr, len)` para logging desde el plugin.

La acción `EstablecerTema` es rechazada en tiempo de ejecución si el plugin no declaró `ui = true`, tanto en plugins Lua como WASM.

### plugin-theme

Plugin oficial de temas escrito en Lua (`init.lua`). El script se embebe en el binario con `include_str!`. Define el tema **One Dark** como una tabla `{ keyword = "#C678DD", string = "#98C379", ... }` devuelta por `M.tema()`. El host lee la tabla al inicializar y la convierte en el mapa de colores activo.

### plugin-wasm-monokai

Plugin oficial de temas compilado a WASM. Implementa el ABI completo (`glyph_alloc`, `glyph_metadata`, `glyph_inicializar`, los tres hooks de eventos) y devuelve la paleta **Monokai** clásica al inicializar. Sirve de referencia para autores de plugins en cualquier lenguaje que compile a `wasm32-unknown-unknown`.

Para compilar:
```bash
cd plugins/plugin-wasm-monokai
cargo build --target wasm32-unknown-unknown --release
```

### glyph-app

Orquesta el sistema. Es el único punto donde todos los crates se tocan.

```
┌─ main() ──────────────────────────────────────────────────────────┐
│  HostPlugins::nuevo()                                             │
│  host.cargar_lua(plugin_theme::TEMA_SCRIPT)                       │
│  host.inicializar()  →  tema One Dark activo                      │
│                                                                   │
│  Document (glyph-core)                                            │
│  Resaltador (glyph-core)          → documento_a_contenido()       │
│  Arc<Mutex<Vec<Diagnostic>>>  ─────────┤                          │
│                                        ↓ ContenidoRender          │
│                              { spans, diagnosticos, cursor }      │
│  glyph_renderer::ejecutar()  ←─────────┘                         │
│       │ EventoEditor                                               │
│       ▼                                                            │
│  match evento → Document::método()                                │
│       │ texto cambiado                                             │
│       ├─ tx_lsp.send(uri, texto, versión)  ──→  hilo tokio        │
│       │                                    ClienteLsp::didChange  │
│       │                                    Diagnosticos → Mutex   │
│       └─ host.al_cambiar(ruta, version)                           │
└───────────────────────────────────────────────────────────────────┘
```

El hilo LSP corre un `tokio::runtime::Runtime` independiente. Los diagnósticos fluyen del hilo LSP al event loop vía `Arc<Mutex<Vec<Diagnostic>>>` — el event loop los convierte a byte offsets en cada frame usando `posicion_lsp_a_byte` (UTF-16 → byte, según el estándar LSP).

---

## Estructura del proyecto

```
glyph/
├── Cargo.toml                  # Workspace — versiones y deps compartidas
├── Cargo.lock                  # Commiteado (proyecto binario)
│
├── crates/
│   ├── glyph-core/             # Núcleo puro: Buffer, Cursor, Document, Resaltador
│   ├── glyph-renderer/         # Ventana, GPU, event loop (winit + wgpu + glyphon)
│   ├── glyph-lsp/              # Cliente LSP asíncrono (tokio + lsp-types)
│   ├── glyph-plugin-api/       # Trait Plugin, AccionPlugin, ContextoPlugin
│   ├── glyph-plugin-host/      # Runtime Lua (mlua) + WASM (wasmtime)
│   └── glyph-app/              # Binario: orquesta todos los crates
│
└── plugins/                    # Plugins oficiales (usan la misma API que terceros)
    ├── plugin-theme/            # Tema One Dark — Lua embebido (init.lua)
    ├── plugin-wasm-monokai/     # Tema Monokai — plugin WASM de referencia
    ├── plugin-git/              # Integración Git — pendiente
    └── plugin-formatter/        # Formateo de código — pendiente
```

---

## Cómo compilar y ejecutar

**Requisitos:**
- Rust 1.75+ (`rustup update stable`)
- Drivers gráficos con soporte Vulkan, Metal o DirectX 12
- (Opcional) `rust-analyzer` en `$PATH` para el cliente LSP

```bash
# Compilar todo el workspace
cargo build

# Abrir un archivo (con syntax highlighting y LSP si rust-analyzer está instalado)
cargo run -p glyph -- src/main.rs

# Abrir un buffer vacío
cargo run -p glyph

# Ver logs de diagnósticos LSP
RUST_LOG=info cargo run -p glyph -- archivo.rs
```

**Atajos de teclado:**

| Tecla | Acción |
|---|---|
| `Ctrl+S` | Guardar |
| `Ctrl+Z` | Deshacer |
| `Ctrl+Y` | Rehacer |
| `Ctrl+F` | Activar búsqueda |
| `Ctrl+H` | Activar búsqueda y reemplazo |
| `Ctrl+K` | Mostrar hover LSP (tipo, doc) en la posición del cursor |
| `Tab` | Insertar 4 espacios |
| `Flechas` | Mover cursor |
| `Inicio` / `Fin` | Inicio / fin de línea |
| `Re Pág` / `Av Pág` | Saltar 20 líneas arriba/abajo |
| `Ctrl+Inicio` / `Ctrl+Fin` | Inicio / fin del documento |
| `Supr` | Borrar carácter adelante |
| `Backspace` | Borrar carácter atrás |
| Click izquierdo | Mover cursor a la posición del puntero |

**Búsqueda (Ctrl+F):**

| Tecla | Acción |
|---|---|
| Caracteres | Actualizar consulta |
| `Enter` | Siguiente resultado |
| `Shift+Enter` | Resultado anterior |
| `Backspace` | Borrar último carácter de la consulta |
| `Escape` | Salir de búsqueda |

**Búsqueda y reemplazo (Ctrl+H):**

| Tecla | Acción |
|---|---|
| Caracteres | Actualizar campo activo |
| `Tab` | Alternar entre campo de búsqueda y campo de reemplazo |
| `Enter` | Reemplazar match activo y avanzar al siguiente |
| `Ctrl+H` | Reemplazar todas las ocurrencias |
| `Shift+Enter` | Match anterior |
| `Backspace` | Borrar último carácter del campo activo |
| `Escape` | Salir del modo reemplazo |

---

## Roadmap

### Milestone 1 — Editor funcional ✅
- [x] Buffer con Rope (`ropey`) en `glyph-core`
- [x] Renderizado GPU con `wgpu` + texto con `glyphon`
- [x] Event loop nativo con `winit`
- [x] Cursor con overlay visual (color de acento)
- [x] Abrir, editar y guardar archivos desde la CLI
- [x] Undo/redo con historial de operaciones

### Milestone 2 — Inteligencia ✅
- [x] Syntax highlighting con `tree-sitter` (Rust, JavaScript, Python)
- [x] Tema One Dark aplicado sobre spans semánticos
- [x] Cliente LSP asíncrono (`rust-analyzer`)
- [x] `didOpen` / `didChange` enviados en cada edición
- [x] Diagnósticos recibidos y registrados en log
- [x] Búsqueda en buffer (Ctrl+F) con navegación entre resultados

### Milestone 3 — Plugin System ✅
- [x] Trait `Plugin` y `AccionPlugin` en `glyph-plugin-api`
- [x] `HostPlugins` con soporte Lua (`mlua`) en `glyph-plugin-host`
- [x] Tema One Dark como plugin Lua (`plugin-theme/init.lua`)
- [x] Diagnósticos LSP inline en el renderer (color overlay por severidad)
- [x] Conversión UTF-16 → byte para posiciones LSP
- [x] Búsqueda en buffer (Ctrl+F) con navegación entre resultados
- [x] Búsqueda y reemplazo (Ctrl+H): reemplazar match activo o todas las ocurrencias
- [x] Click de ratón para posicionar el cursor en cualquier línea y columna
- [x] Barra de estado: nombre de archivo, posición del cursor, errores LSP y estado de búsqueda/reemplazo
- [x] Scroll automático: el editor sigue al cursor con 3 líneas de contexto
- [x] Navegación extendida: Page Up/Down, Ctrl+Home/End
- [x] Números de línea (gutter) con resaltado de línea activa
- [x] Syntax highlighting para JavaScript (`.js`, `.mjs`) y Python (`.py`)
- [x] Popup de hover LSP (Ctrl+K) — texto flotante sobre el cursor con tipo y documentación
- [x] Sistema de permisos declarativo (`Permisos`) — sandbox Lua + enforcement de acciones

### Milestone 4 — Plugin System avanzado + IA (en progreso)
- [x] Plugin host WASM con `wasmtime` (sandboxed, `wasm32-unknown-unknown`)
- [x] WIT API v1 — spec de contratos en `glyph-plugin-api/wit/plugin.wit`
- [x] Plugin de referencia WASM: tema Monokai (`plugins/plugin-wasm-monokai`)
- [ ] Provider pattern para modelos de IA
- [ ] Soporte Ollama (inferencia local/privada)
- [ ] Routing de IA por tipo de archivo

### Milestone 5 — Pulido
- [ ] Temas personalizables
- [ ] Documentación del SDK de plugins
- [ ] Marketplace de plugins

---

## Contribuir

Glyph está en desarrollo activo. Por favor lee [CONTRIBUTING.md](CONTRIBUTING.md) antes de abrir un PR.

El proyecto usa **Apache 2.0** — puedes usar el código, hacer forks, incluso versiones comerciales. Los cambios no necesitan ser abiertos, pero deben documentarse.

## Licencia

Apache License 2.0 — ver [LICENSE](LICENSE) para detalles.

---

*Glyph — porque cada carácter importa.*
