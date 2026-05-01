# GLYPH — Blueprint del Proyecto

> Pega este archivo al inicio de cualquier conversación nueva con Claude para continuar
> el desarrollo sin perder contexto. Actualízalo conforme el proyecto avance.

---

## Identidad del proyecto

| Campo | Valor |
|---|---|
| Nombre | Glyph |
| Tagline | *Every character matters* |
| Tipo | Editor de código de escritorio multiplataforma |
| Repositorio | https://github.com/frantsiles/glyph |
| Licencia | Apache 2.0 |
| Estado actual | Milestone 1 — en desarrollo |

---

## Filosofía de diseño (NO negociable)

1. **El core hace lo mínimo** — toda feature es un plugin, incluso las nativas
2. **Los plugins son ciudadanos de primera clase** — usan la misma API interna
3. **No limitamos el ecosistema** — Lua para plugins simples, WASM para complejos
4. **La IA es un plugin más** — provider pattern, múltiples agentes sin conflicto
5. **Privacidad por diseño** — Ollama local para código sensitivo, cloud opt-in

---

## Stack técnico definitivo

| Componente | Tecnología | Versión | Razón |
|---|---|---|---|
| Lenguaje | Rust | 1.75+ | Rendimiento + seguridad de memoria |
| Renderizado | wgpu + winit | 0.19 / 0.29 | GPU nativo, fallback automático a software |
| UI Framework | iced | 0.12 | El más maduro para Rust desktop |
| Buffer de texto | ropey (Rope) | 1.6 | Edición O(log n), eficiente en archivos grandes |
| Renderizado de texto | cosmic-text | 0.11 | RTL, ligaduras, unicode completo |
| Syntax highlighting | tree-sitter | 0.22 | El mismo que Neovim, Zed, GitHub |
| LSP client | tower-lsp | 0.20 | Autocompletado, diagnósticos, hover |
| Plugins simples | Lua vía mlua | 0.9 | Scripts sin compilar, recarga en caliente |
| Plugins complejos | WASM vía wasmtime | 20 | Sandboxed, cualquier lenguaje compilable |
| Contratos de plugins | WIT (wit-bindgen) | 0.25 | Interface types para WASM |
| Async runtime | tokio | 1 | Standard de la industria en Rust |
| Serialización | serde + toml | 1 / 0.8 | Configuración y estado |
| IDs únicos | uuid v4 | 1 | Buffers, plugins, cursores |
| File watcher | notify | 6 | Detectar cambios externos en archivos |

---

## Arquitectura de crates (Workspace de Cargo)

```
glyph/                          ← workspace root
├── Cargo.toml                  ← dependencias centralizadas en [workspace.dependencies]
├── crates/
│   ├── glyph-core/             ← CERO dependencias de UI
│   │   ├── buffer.rs           ← Buffer basado en Rope, operaciones O(log n)
│   │   ├── cursor.rs           ← Cursor + Selection, multi-cursor support
│   │   ├── document.rs         ← Integra buffer + cursores + historia
│   │   └── history.rs          ← Undo/redo con grupos de operaciones
│   │
│   ├── glyph-renderer/         ← wgpu + winit, renderizado GPU
│   ├── glyph-lsp/              ← Cliente LSP, tower-lsp
│   ├── glyph-plugin-host/      ← Runtime Lua (mlua) + WASM (wasmtime)
│   ├── glyph-plugin-api/       ← Contratos WIT, tipos públicos del SDK
│   └── glyph-app/              ← Entry point, integra todos los crates
│
└── plugins/                    ← Plugins oficiales (usan la misma API que terceros)
    ├── plugin-theme/            ← Temas (Lua)
    ├── plugin-git/              ← Integración Git (WASM)
    └── plugin-formatter/        ← Formateo de código (WASM)
```

---

## Sistema de plugins — Modelo híbrido

### Dos niveles, un ecosistema

```
Nivel 1 — Lua (plugins simples)
  → Sin compilar, recarga en caliente
  → Para: temas, keybindings, snippets, macros, configuración
  → Runtime: mlua con Lua 5.4

Nivel 2 — WASM (plugins complejos)
  → Compilado desde cualquier lenguaje (Rust, Go, C, Zig...)
  → Para: formatters, linters, integración git, clientes de IA
  → Runtime: wasmtime con component-model
  → Contratos definidos en WIT (WebAssembly Interface Types)
```

### Sistema de permisos declarativo

```rust
enum Permiso {
    LecturaArchivos,
    EscrituraArchivos,
    RedInterna,
    ConfiguracionEditor,
    ComandosShell,       // requiere confirmación explícita del usuario
}
```

### Principio de diseño del SDK

> El editor en sí es su primer usuario del SDK.
> Si las features internas usan la misma API que los plugins externos,
> la API es buena por necesidad.

---

## IA como plugin — Provider Pattern

```
No hay ningún proveedor de IA hardcodeado en el core.
Cada modelo/servicio es un plugin que implementa el contrato ai-provider.

Proveedores planeados:
→ plugin-ai-ollama    (local, privado, sin internet)
→ plugin-ai-claude    (Anthropic)
→ plugin-ai-openai    (OpenAI)
→ plugin-ai-gemini    (Google)

Routing inteligente (configurable por usuario):
→ archivos .rs con código privado  → Ollama local
→ documentación .md               → Claude
→ código público                  → cualquier provider
```

---

## Roadmap por Milestones

### ✅ Milestone 0 — Fundación (COMPLETADO)
- [x] Repositorio creado en GitHub
- [x] Workspace de Cargo con 6 crates
- [x] glyph-core: buffer, cursor, document, history
- [x] README, CONTRIBUTING, .gitignore, Apache 2.0
- [x] Conventional Commits adoptado

### 🔄 Milestone 1 — Editor funcional (EN CURSO)
- [ ] glyph-renderer: ventana básica con wgpu
- [ ] Renderizado de texto con cosmic-text
- [ ] Abrir, editar y guardar archivos
- [ ] glyph-app: entry point funcional

### Milestone 2 — Inteligencia
- [ ] tree-sitter: syntax highlighting
- [ ] glyph-lsp: cliente LSP básico
- [ ] Autocompletado y diagnósticos
- [ ] Búsqueda y reemplazo

### Milestone 3 — Plugin System
- [ ] glyph-plugin-api: contratos WIT v1
- [ ] glyph-plugin-host: runtime Lua
- [ ] glyph-plugin-host: runtime WASM
- [ ] Sistema de permisos
- [ ] Plugin manager (instalar/desinstalar)
- [ ] plugin-theme oficial en Lua
- [ ] plugin-formatter oficial en WASM

### Milestone 4 — IA como plugin
- [ ] Contrato WIT ai-provider
- [ ] plugin-ai-ollama
- [ ] Router de IA por contexto de archivo
- [ ] Múltiples agentes sin conflicto

### Milestone 5 — Pulido y ecosistema
- [ ] Temas personalizables
- [ ] Documentación del SDK de plugins
- [ ] Marketplace de plugins

---

## Convenciones del proyecto

### Código
- Comentarios y documentación en **español**
- Identificadores en **snake_case en español** cuando sea semánticamente claro
- `cargo fmt` antes de cada commit
- `cargo clippy` sin warnings
- Tests unitarios obligatorios en código nuevo

### Commits (Conventional Commits)
```
feat(crate):     nueva funcionalidad
fix(crate):      corrección de bug
docs:            solo documentación
refactor(crate): sin cambio de comportamiento
test(crate):     agregar o corregir tests
chore:           mantenimiento (deps, config)
perf(crate):     mejora de rendimiento
```

### Header de copyright en cada archivo
```rust
// Copyright 2026 Franz (frantsiles)
// Licensed under the Apache License, Version 2.0
```

---

## Decisiones de diseño ya tomadas (no reabrir sin razón)

| Decisión | Alternativas descartadas | Razón |
|---|---|---|
| Rust como lenguaje | Flutter, Electron/Tauri | Rendimiento nativo, referencia Warp/Zed |
| wgpu para renderizado | OpenGL directo | Fallback automático, multiplataforma |
| ropey (Rope) para buffer | String, Gap Buffer, Piece Table | O(log n), maduro, usado en producción |
| Plugins híbridos Lua+WASM | Solo WASM, Solo Lua | No limitar el ecosistema de developers |
| Apache 2.0 | MIT, GPL v3 | Protección de patentes + máxima apertura |
| WIT para contratos WASM | ABI nativo, protobuf | Type-safe, cross-language, sin ABI hell |
| IA como plugin (provider pattern) | IA hardcodeada en core | Flexibilidad, privacidad, múltiples agentes |

---

## Contexto del desarrollador

- Senior developer, enfoque en buenas prácticas y código limpio
- Prefiere soluciones modernas, escalables y parametrizables
- Le gusta que se propongan mejoras cuando hay formas más modernas de hacer algo
- Editor actual: Visual Studio Code
- Repo: https://github.com/frantsiles/glyph