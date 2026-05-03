# GLYPH — Blueprint de Desarrollo v2.0

> **Documento vivo.** Última actualización: 2026-05-02.
> Pega este archivo al inicio de cualquier sesión nueva con Claude para continuar
> el desarrollo sin perder contexto. Actualiza "Estado actual" y los milestones conforme avance.

---

## 1. Identidad y Filosofía

| Campo | Valor |
|---|---|
| Nombre | Glyph |
| Tagline | *Every character matters* |
| Tipo | Editor de código de escritorio multiplataforma |
| Licencia | Apache 2.0 |
| Lenguaje | Rust (edition 2021, rust-version 1.75) |

### Principios No Negociables

**1. El core hace lo mínimo — todo lo demás es un plugin.**
`glyph-core` no tiene dependencias de UI, no sabe de LSP, no sabe de plugins. Es una biblioteca de texto pura. Si algo puede vivir fuera del core, vive fuera.

**2. Plugin-first con igualdad de ciudadanía.**
Los plugins oficiales (temas, sidebar, git, formatter) usan exactamente la misma API pública que los plugins de terceros. No hay APIs privadas para "primeras partes".

**3. No limitamos el ecosistema.**
Lua para plugins ligeros (temas, keybindings, macros). WASM para plugins potentes (formatters, linters, IA, git). Un usuario puede escribir un plugin en Zig, Go o C.

**4. La IA es un proveedor, no una identidad.**
No hay cliente de IA hardcodeado. El contrato `ai-provider` define la interfaz. Los proveedores son plugins WASM intercambiables.

**5. Privacidad por diseño.**
El proveedor de IA por defecto es Ollama (local, sin red). Los proveedores cloud son opt-in explícito con API key. El usuario puede bloquear archivos sensibles del routing de IA.

**6. Multiplataforma por disciplina.**
Linux, macOS, Windows. El core no tiene código de plataforma. El renderer usa wgpu (abstrae Vulkan/Metal/D3D12/OpenGL).

**7. Rendimiento no es un feature, es un requisito.**
Buffer Rope (O(log n)), resaltado tree-sitter incremental, renderer GPU con wgpu. Los plugins lentos no bloquean el event loop.

---

## 2. Arquitectura de Capas

```
┌─────────────────────────────────────────────────────────────────────┐
│                         USUARIO FINAL                               │
└──────────────────────────────────┬──────────────────────────────────┘
                                   │  inputs (teclado, ratón, ventana)
┌──────────────────────────────────▼──────────────────────────────────┐
│                      glyph-renderer                                 │
│  winit (event loop) + wgpu (GPU) + glyphon/cosmic-text (texto)      │
│                                                                     │
│  Entrada:  EventoEditor (enum plano, sin lógica de negocio)         │
│  Salida:   ContenidoRender (DTO plano, sin tipos de glyph-core)     │
│                                                                     │
│  Secciones visuales (M5+):                                          │
│    QuadRenderer | LayoutManager | SeccionUI                         │
│  Secciones actuales (hardcodeadas, a refactorizar en M5):           │
│    TabsBar | Gutter | EditorPrincipal | BarraEstado | HoverPopup    │
└──────────────────────────────────┬──────────────────────────────────┘
                                   │  EventoEditor / ContenidoRender
┌──────────────────────────────────▼──────────────────────────────────┐
│                        glyph-app                                    │
│  Orquestador central — el único crate que conoce a todos los demás  │
│                                                                     │
│  GestorTabs        — colección de TabDoc (Document + metadatos)     │
│  documento_a_contenido() — construye ContenidoRender desde Document │
│  hilo_lsp()        — runtime Tokio independiente para LSP           │
│  ConfigApp         — config.toml de usuario (~/.config/glyph/)      │
└──────┬──────────────────┬────────────────────┬───────────────────────┘
       │                  │                    │
┌──────▼──────┐  ┌────────▼───────┐  ┌────────▼────────────────────┐
│ glyph-core  │  │   glyph-lsp    │  │ glyph-plugin-host           │
│             │  │                │  │                             │
│ Buffer(Rope)│  │ ClienteLsp     │  │ HostPlugins                 │
│ Cursor      │  │ (rust-analyzer)│  │  ├─ PluginLua (mlua 0.9)   │
│ Document    │  │ Diagnósticos   │  │  └─ PluginWasm (wasmtime 20)│
│ Historia    │  │ Hover LSP      │  │                             │
│ Resaltador  │  │                │  │ glyph-plugin-api (contratos)│
│ (tree-sitter│  │                │  │  Permisos, AccionPlugin      │
│  Rust,JS,Py)│  │                │  │  WIT API v1                 │
└─────────────┘  └────────────────┘  └─────────────────────────────┘
                                              │
                              ┌───────────────┼───────────────┐
                     ┌────────▼────┐ ┌────────▼──────┐ ┌──────▼──────┐
                     │plugin-theme │ │plugin-wasm-   │ │plugin-git   │
                     │(Lua,Wallbash│ │monokai (WASM) │ │(stub)       │
                     └─────────────┘ └───────────────┘ └─────────────┘
```

### Flujo de datos en un keystroke

```
1. winit detecta KeyboardInput
2. renderer/renderer.rs::resolver_evento() → EventoEditor::InsertarTexto("a")
3. glyph-app closure recibe EventoEditor
4. gestor.tab_mut().documento.insertar_en_cursor("a")  → glyph-core
5. host.al_cambiar(ruta, version)  → glyph-plugin-host → Lua/WASM
6. if lsp_uri: tx_lsp.send((uri, texto, version))  → hilo LSP async
7. construir_contenido() → documento_a_contenido() → ContenidoRender
8. renderer recibe ContenidoRender → actualizar_contenido() → frame GPU
```

### Principio DTO (ContenidoRender)

`glyph-renderer` no tiene dependencia de `glyph-core`. `glyph-app` construye el DTO a partir del `Document` del core y lo pasa al renderer. Esto permite cambiar el core sin tocar el renderer, y testear el renderer de forma completamente aislada.

---

## 3. Contratos de API entre Capas

### glyph-core — API pública estable

```rust
pub struct Document {
    pub buffer: Buffer,
    // Operaciones clave
    fn nuevo() -> Self;
    fn desde_archivo(contenido: &str, ruta: PathBuf) -> Self;
    fn insertar_en_cursor(&mut self, texto: &str) -> Result<()>;
    fn borrar_antes_cursor(&mut self) -> Result<()>;
    fn borrar_despues_cursor(&mut self) -> Result<()>;
    fn deshacer(&mut self) -> Result<()>;
    fn rehacer(&mut self) -> Result<()>;
    fn buscar(&self, consulta: &str) -> Vec<(usize, usize)>;
    fn mover_cursor_a_byte(&mut self, byte: usize);
    fn seleccion_bytes(&self) -> Option<(usize, usize)>;
    fn cursor_principal(&self) -> &Cursor;
}
```

### glyph-renderer ↔ glyph-app (contrato DTO)

```rust
// Entrada del renderer hacia la app
pub enum EventoEditor {
    InsertarTexto(String), BorrarAtras, BorrarAdelante,
    MoverCursor(DireccionCursor), MoverCursorA { linea: u32, columna: u32 },
    Deshacer, Rehacer, Guardar,
    IniciarBusqueda, ActualizarBusqueda(String), SiguienteMatch, MatchAnterior, TerminarBusqueda,
    IniciarReemplazo, ActualizarReemplazo(String), ReemplazarMatch, ReemplazarTodo,
    SeleccionarTodo, ExtenderSeleccion(DireccionCursor), Copiar, Cortar, Pegar,
    NuevoTab, CerrarTab, SiguienteTab, AnteriorTab, ActivarTab(usize),
    PedirHover,
    // M5+: EventoSeccion { id_seccion: String, payload: Vec<u8> }
}

// Salida de la app hacia el renderer
pub struct ContenidoRender {
    pub lineas: Vec<String>,
    pub cursor: Option<CursorRender>,
    pub tamano_fuente: f32,
    pub spans: Vec<SpanTexto>,
    pub diagnosticos: Vec<DiagnosticoRender>,
    pub matches_busqueda: Vec<(usize, usize)>,
    pub match_activo: Option<usize>,
    pub barra_estado: String,
    pub hover_texto: Option<String>,
    pub seleccion_bytes: Option<(usize, usize)>,
    pub tabs: Vec<TabInfo>,
    // M5+: secciones_plugin: Vec<SeccionContenidoRender>
}
```

### glyph-plugin-api — Contratos de plugins

```rust
pub struct Permisos {
    pub ui: bool,
    pub leer_archivos: bool,
    pub escribir_archivos: bool,
    pub ejecutar_procesos: bool,
    pub red: bool,
}

pub enum AccionPlugin {
    EstablecerTema(HashMap<String, [u8; 3]>),
    LogMensaje(String),
    // M5+:
    // RegistrarSeccion(SeccionConfig),
    // ActualizarContenidoSeccion { id: String, lineas: Vec<LineaSeccion> },
    // AbrirArchivo(String),
    // ReemplazarContenidoBuffer(String),   -- para formatter
    // DecorarLineas(Vec<DecoracionLinea>), -- para git gutter
}

pub trait Plugin: Send + 'static {
    fn nombre(&self) -> &str;
    fn permisos(&self) -> Permisos { Permisos::default() }
    fn inicializar(&mut self) -> Vec<AccionPlugin> { vec![] }
    fn al_cambiar(&mut self, ctx: &ContextoPlugin) -> Vec<AccionPlugin> { vec![] }
    fn al_guardar(&mut self, ctx: &ContextoPlugin) -> Vec<AccionPlugin> { vec![] }
    fn al_abrir(&mut self, ctx: &ContextoPlugin) -> Vec<AccionPlugin> { vec![] }
}
```

### ABI WASM — Contrato binario (versión 1, actual)

```
Exports del módulo WASM:
  glyph_alloc(size: i32) -> i32
  glyph_metadata() -> i32                  // → JSON {nombre, descripcion, permisos}
  glyph_inicializar() -> i32               // → JSON [AccionJson, ...]
  glyph_al_abrir(ptr, len) -> i32
  glyph_al_cambiar(ptr, len, version) -> i32
  glyph_al_guardar(ptr, len) -> i32

Import que el host provee:
  env::glyph_log(ptr: i32, len: i32)

Formato de strings: [u32-LE longitud][UTF-8]
Formato de retorno: puntero a [u32-LE len][UTF-8 JSON]
```

---

## 4. Estado Actual — 2026-05-02 (M5 completado)

### Tabla de features

| Feature | Crate | Estado |
|---|---|---|
| Buffer Rope (O(log n)) | glyph-core | ✅ Completo |
| Cursor + Selección | glyph-core | ✅ Completo |
| Document (buffer + cursor + historia) | glyph-core | ✅ Completo (18 tests) |
| Resaltado sintáctico Rust/JS/Python | glyph-core | ✅ Completo |
| Buscar / Reemplazar | glyph-core | ✅ Completo |
| Ventana wgpu + winit, renderer GPU | glyph-renderer | ✅ Completo |
| Barra de tabs multi-buffer | glyph-renderer | ✅ Completo |
| Gutter, statusbar, hover popup | glyph-renderer | ✅ Completo |
| Click de ratón, scroll | glyph-renderer | ✅ Completo |
| Búsqueda/reemplazo visual | glyph-renderer | ✅ Completo |
| GestorTabs multi-buffer | glyph-app | ✅ Completo |
| ConfigApp (config.toml) | glyph-app | ✅ Completo |
| Portapapeles (arboard) | glyph-app | ✅ Completo |
| LSP (diagnósticos + hover) | glyph-lsp | ✅ Completo |
| Runtime Lua con sandbox | glyph-plugin-host | ✅ Completo |
| Runtime WASM (wasmtime 20) | glyph-plugin-host | ✅ Completo |
| Sistema de permisos declarativos | glyph-plugin-api | ✅ Completo |
| plugin-theme (Lua, Wallbash) | plugin-theme | ✅ Completo |
| plugin-wasm-monokai (referencia) | plugin-wasm-monokai | ✅ Completo |
| QuadRenderer (rects wgpu) | glyph-renderer | ✅ Completo M5 |
| SeccionUI + LayoutManager | glyph-renderer | ✅ Completo M5 |
| API secciones en plugins (Lua/WASM) | glyph-plugin-api | ✅ Completo M5 |
| plugin-sidebar (file explorer) | plugin-sidebar | ✅ Completo M5 |
| plugin-git (gutter + panel) | plugin-git | 🔲 Pendiente M6 |
| plugin-formatter (rustfmt/prettier) | plugin-formatter | 🔲 Pendiente M6 |
| Contrato WIT ai-provider | glyph-plugin-api | 🔲 Pendiente M7 |
| plugin-ai-ollama | — | 🔲 Pendiente M7 |
| Tree-sitter incremental | glyph-core | 🔲 Pendiente M8 |
| Multi-cursor real | glyph-core | 🔲 Pendiente M8 |
| LSP multi-archivo (multi-tab) | glyph-lsp | 🔲 Pendiente M8 |
| SDK de plugins documentado | — | 🔲 Pendiente M9 |
| Empaquetado multiplataforma | — | 🔲 Pendiente M9 |

### Atajos implementados

```
Ctrl+S  guardar       Ctrl+Z  deshacer       Ctrl+Y  rehacer
Ctrl+F  búsqueda      Ctrl+H  reemplazo       Ctrl+K  hover LSP
Ctrl+A  seleccionar todo
Ctrl+C  copiar        Ctrl+X  cortar          Ctrl+V  pegar
Ctrl+T  nuevo tab     Ctrl+W  cerrar tab
Ctrl+Tab siguiente tab        Ctrl+Shift+Tab  tab anterior
Flechas, Home/End, RePág/AvPág, Ctrl+Home/End
Shift+Flechas/Home/End  — extender selección
Tab → 4 espacios (configurable por lenguaje)
Click izquierdo → mover cursor o activar tab
Scroll → desplazar vista
```

### Decisiones técnicas fijas (no reabrir)

| Decisión | Alternativas descartadas | Razón |
|---|---|---|
| Rust | Flutter, Electron/Tauri | Rendimiento nativo, sin GC |
| wgpu | OpenGL directo | Fallback automático, multiplataforma |
| ropey (Rope) | String, Gap Buffer | O(log n) garantizado |
| Lua + WASM | Solo WASM, Solo Lua | Lua rápido, WASM potente |
| Apache 2.0 | MIT, GPL v3 | Protección patentes + apertura comercial |
| ContenidoRender como DTO | glyph-core en renderer | Desacoplamiento total |
| Hilo LSP separado | LSP en event loop | winit no puede bloquearse |
| IA como plugin | IA hardcodeada | Flexibilidad + privacidad |

---

## 5. Plan de Milestones

> M0–M5 completados. Plan desde M6.

---

### M5 — Quad Renderer + UI Sections ← PRÓXIMO

**Objetivo:** El renderer soporta un sistema de secciones visuales dinámicas. Cada componente visual es una `SeccionUI`. Los plugins pueden declarar sus propias secciones. La sidebar de archivos es el primer plugin visual real.

**Criterio de "hecho":** La sidebar de archivos funciona como plugin declarando una sección. Tabs y statusbar son secciones del LayoutManager. Un click en la sidebar abre el archivo en un tab.

#### Fase M5.1 — QuadRenderer

Nuevo módulo `glyph-renderer/src/quads.rs`:

```rust
pub struct QuadRenderer {
    pipeline: wgpu::RenderPipeline,
    vertex_buffer: wgpu::Buffer,
    quads: Vec<QuadVertex>,
}

pub struct RectPx { pub x: f32, pub y: f32, pub ancho: f32, pub alto: f32 }
pub struct ColorRgba { pub r: f32, pub g: f32, pub b: f32, pub a: f32 }

impl QuadRenderer {
    pub fn nuevo(dispositivo: &wgpu::Device, formato: wgpu::TextureFormat) -> Self;
    pub fn limpiar(&mut self);
    pub fn agregar_quad(&mut self, rect: RectPx, color: ColorRgba);
    pub fn preparar(&mut self, dispositivo: &wgpu::Device, cola: &wgpu::Queue, ancho: u32, alto: u32);
    pub fn renderizar_en_pase<'pass>(&'pass self, pase: &mut wgpu::RenderPass<'pass>);
}
```

Orden de renderizado en `renderizar_frame()`:
1. Clear (color de fondo ventana)
2. `QuadRenderer::renderizar_en_pase()` — fondos de secciones
3. `RendererTexto::renderizar_en_pase()` — texto encima

#### Fase M5.2 — SeccionUI y LayoutManager

Nuevo módulo `glyph-renderer/src/seccion.rs`:

```rust
pub enum TamanoPref { Fijo(f32), Flex(f32), Minmax(f32, f32) }
pub enum LadoLayout { Izquierda, Derecha, Arriba, Abajo, Centro }

pub struct SeccionUI {
    pub id: String,
    pub lado: LadoLayout,
    pub tamano_pref: TamanoPref,
    pub visible: bool,
    pub z_order: i32,
    pub color_fondo: Option<ColorRgba>,
}

pub struct LayoutManager { ... }

impl LayoutManager {
    pub fn registrar(&mut self, seccion: SeccionUI);
    pub fn quitar(&mut self, id: &str);
    pub fn calcular(&mut self, ancho: f32, alto: f32) -> &[GeometriaSolucion];
    pub fn seccion_en_posicion(&self, x: f32, y: f32) -> Option<&String>;
}
```

Secciones de sistema registradas al inicio:

| id | lado | tamaño |
|---|---|---|
| "tabs" | Arriba | Fijo(32px) |
| "sidebar" | Izquierda | Fijo(240px, togglable) |
| "gutter" | Izquierda | Minmax(40, 80) |
| "editor" | Centro | Flex(1.0) |
| "statusbar" | Abajo | Fijo(22px) |

#### Fase M5.3 — API de plugins para secciones

Nuevas variantes en `AccionPlugin`:

```rust
RegistrarSeccion(SeccionConfig),
ActualizarContenidoSeccion { id: String, lineas: Vec<LineaSeccion> },
QuitarSeccion(String),
AbrirArchivo(String),
MostrarNotificacion { mensaje: String, nivel: NivelNotificacion },
```

Nuevos eventos en `EventoEditor`:

```rust
EventoSeccion { id_seccion: String, payload: Vec<u8> }
// Disparado cuando el usuario hace click en una sección de plugin
```

API Lua (nuevas funciones expuestas por el host):

```lua
glyph.registrar_seccion({ id, lado, tamano, color_bg })
glyph.abrir_archivo(ruta)
-- callbacks del plugin:
function M.renderizar_seccion(estado) -> [LineaSeccion]
function M.click_seccion(x, y, payload)
```

#### Fase M5.4 — plugin-sidebar

Nuevo crate `plugins/plugin-sidebar/` (Lua, permisos: `ui + leer_archivos`):

- Muestra árbol de archivos del directorio del archivo activo
- Carpetas expandibles/colapsables con `▾`/`▸`
- Click en archivo → `glyph.abrir_archivo(ruta)`
- Activar/desactivar con `Ctrl+B`
- Visual:
  ```
  EXPLORADOR
  ▾ mi-proyecto/
    ▾ src/
        main.rs   ← archivo activo (resaltado)
        lib.rs
    ▸ tests/
      Cargo.toml
  ```

**Crates afectados:** `glyph-renderer` (quads.rs, seccion.rs), `glyph-plugin-api` (AccionPlugin), `glyph-plugin-host` (Lua API), `glyph-app` (EventoSeccion), nuevo `plugin-sidebar`

---

### M6 — Plugins Oficiales Completados

**Objetivo:** `plugin-git` y `plugin-formatter` funcionales como plugins WASM.

- **plugin-formatter:** invoca rustfmt/prettier/black vía `ejecutar_procesos`, reemplaza el buffer. `AccionPlugin::ReemplazarContenidoBuffer(String)`.
- **plugin-git:** indicadores de cambios en el gutter (líneas añadidas/modificadas/borradas), estado en statusbar. `AccionPlugin::DecorarLineas(Vec<DecoracionLinea>)`.
- Criterio: `rustfmt` se ejecuta al guardar `.rs`. Gutter muestra diff git en repos con cambios.

---

### M7 — IA como Plugin (Provider Pattern)

**Objetivo:** IA completamente desacoplada como plugins WASM intercambiables.

- WIT `ai-provider.wit`: `completar(AiContexto) -> AiRespuesta`, `chat(Vec<AiMensaje>) -> AiRespuesta`
- Panel AI Chat como sección (usando M5)
- `plugin-ai-ollama`: cliente Ollama local (HTTP localhost, permiso `red`)
- `plugin-ai-claude`: cliente Anthropic API (permiso `red` + API key en config)
- Router de IA en `glyph-app`: por extensión/path, con reglas de bloqueo para archivos sensibles

```toml
[ia.router]
defecto = "ollama"

[[ia.router.reglas]]
pattern = "*.rs"
proveedor = "ollama"

[[ia.router.bloqueo]]
pattern = "*.env"
pattern = "*secret*"
```

---

### M8 — Rendimiento y Calidad de Edición

- **Tree-sitter incremental:** parsear solo el rango modificado. `Document` mantiene `arbol_sintactico: Option<tree_sitter::Tree>`, actualizado en cada edición. Elimina el re-parseo completo en cada frame.
- **Multi-cursor real:** `Document` ya tiene `cursores: Vec<Cursor>`, implementar edición simultánea. Alt+Click añade cursor. Ctrl+Alt+↑/↓.
- **LSP multi-archivo:** notificar al LSP al cambiar de tab, no solo al arrancar.
- Archivos grandes (>1MB): deshabilitar resaltado, advertir al usuario.
- Detección de archivos binarios.

---

### M9 — Ecosistema y Publicación v1.0

- SDK documentado para plugins Lua y WASM
- Estabilización del ABI WASM v1.0 (sin romper compatibilidad después)
- Marketplace: repositorio JSON/TOML en GitHub con metadatos de plugins
- `glyph --instalar-plugin <nombre>`
- Empaquetado: Linux (x86_64, ARM64), macOS (ARM64), Windows (x86_64) vía GitHub Actions
- Sitio web con documentación de usuario y contribuidores

---

## 6. Diseño de M5 — API de Secciones para Plugins (detalle)

### Ciclo de vida de una sección de plugin

```
Plugin.inicializar()
  → AccionPlugin::RegistrarSeccion({ id: "sidebar", lado: "izquierda", tamano: 240 })
  → LayoutManager registra la sección
  → QuadRenderer dibuja el fondo en cada frame

Plugin.renderizar_seccion(estado)  ← llamado por el renderer cada frame (o al invalidar)
  → retorna Vec<LineaSeccion>
  → renderer dibuja el texto encima del quad de fondo

Usuario hace click en coordenada (x, y)
  → LayoutManager::seccion_en_posicion(x, y) → "sidebar"
  → EventoEditor::EventoSeccion { id_seccion: "sidebar", payload: [y_relativa] }
  → glyph-app lo pasa al HostPlugins
  → Plugin.click_seccion(x, y_relativa) → AccionPlugin::AbrirArchivo("/ruta/archivo.rs")
  → glyph-app abre el archivo en un nuevo tab
```

### Formato de LineaSeccion

```rust
pub struct LineaSeccion {
    pub texto: String,
    pub color: Option<[u8; 3]>,         // None = color por defecto de la sección
    pub negrita: bool,
    pub payload: Option<Vec<u8>>,        // datos opacos devueltos en click_seccion
}
```

### Sincronización plugin ↔ renderer (M5: reactivo síncrono)

En M5, los plugins solo pueden actualizar su sección cuando reciben un hook (`al_abrir`, `al_cambiar`, `al_guardar`) o un `EventoSeccion`. El renderer llama a `renderizar_seccion()` en cada frame y cachea el resultado hasta que el plugin señale invalidación con `AccionPlugin::ActualizarContenidoSeccion`.

*En M7 (IA async), se añadirá un canal `Arc<Mutex<Vec<AccionPlugin>>>` para que workers async puedan invalidar secciones sin esperar a un hook.*

---

## 7. Decisiones Técnicas Abiertas

| Pregunta | Opciones | Estado |
|---|---|---|
| ¿WASM o Lua para plugin-sidebar? | Lua (más rápido, host expone `glyph.leer_directorio`). WASM (más potente, necesita WASI sandbox) | Provisional: **Lua** |
| ¿WIT o ABI manual para secciones? | JSON manual (compatibilidad con plugins actuales). WIT component-model (type-safe, rompe ABI actual) | Decidir en M5: **WIT en M9, JSON en M5** |
| ¿Cómo el formatter evita loop al_cambiar? | Campo `origen_plugin: String` en `ReemplazarContenidoBuffer` para filtrar re-notificación | **Provisional aprobada** |
| ¿Plugins async (IA)? | `async_trait`, Worker task separado + canales | Para M7: **Worker task + canales** |
| ¿Cómo renderizar selección como quad? | Quad de fondo por línea seleccionada (reemplaza el cambio de color actual) | Para M5 si el QuadRenderer lo permite |

---

## 8. No Hacer — Fuera del Alcance

- Framework de UI con widgets (botones, inputs, dropdowns) — las secciones de plugin son listas de texto coloreado
- Formateo enriquecido (Markdown renderizado, HTML) en el editor principal
- Split de paneles dentro del editor — múltiples ventanas del OS para ver dos archivos
- Terminal integrado (pty, emulación ANSI)
- Gestor de workspace como VS Code — Glyph es un editor, no un IDE completo
- Extensiones JavaScript/TypeScript — el ecosistema es Lua + WASM
- Sincronización en la nube de configuración o archivos
- Auto-update del binario — el gestor de paquetes del usuario se encarga
- Emulación de Vim (modo modal) — el sistema de keybindings es reconfiguable, pero sin modo modal nativo
- Depurador (DAP) — fuera del alcance de los milestones actuales

---

## 9. Convenciones del Proyecto

### Código

- Comentarios y nombres de variables en **español** cuando el contexto lo permite claramente
- `cargo fmt` antes de cada commit
- `cargo clippy -- -D warnings` sin errores en main
- Tests unitarios para toda función con lógica de negocio nueva en `glyph-core`
- Header en cada archivo nuevo:
  ```rust
  // Copyright 2026 Franz (frantsiles)
  // Licensed under the Apache License, Version 2.0
  ```

### Commits (Conventional Commits)

```
feat(crate):     nueva funcionalidad
fix(crate):      corrección de bug
refactor(crate): sin cambio de comportamiento observable
test(crate):     agregar o corregir tests
docs:            solo documentación
chore:           mantenimiento (deps, config, CI)
perf(crate):     mejora de rendimiento medible
```

### Stack de versiones (fijadas, no actualizar sin revisión)

```toml
wgpu         = "0.19"
winit        = "0.29"
glyphon      = "0.5"
cosmic-text  = "0.11"
tree-sitter  = "0.22"
wasmtime     = "20"
mlua         = "0.9"   # lua54, vendored
ropey        = "1.6"
tokio        = "1"
arboard      = "3"
dirs         = "5"
```

---

*Fin del blueprint. Actualizar la sección "Estado Actual" al completar cada milestone.*
