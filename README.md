# Glyph

> Every character matters.

A fast, native code editor built in Rust. Extend it with Lua, WASM, or whatever you know.

![License](https://img.shields.io/badge/license-Apache%202.0-blue.svg)
![Rust](https://img.shields.io/badge/rust-1.75%2B-orange.svg)
![Status](https://img.shields.io/badge/status-early%20development-yellow.svg)

---

## ¿Qué es Glyph?

Glyph es un editor de código multiplataforma de escritorio construido desde cero en Rust, con renderizado GPU, soporte nativo de LSP y un sistema de plugins híbrido que no te obliga a aprender un lenguaje específico.

## Filosofía

- **El core hace lo mínimo** — todo lo demás es un plugin, incluso las features nativas
- **Los plugins son ciudadanos de primera clase** — usan la misma API que el editor internamente
- **No limitamos el ecosistema** — escribe plugins en Lua, Rust (WASM), Go, C, Zig o cualquier lenguaje que compile a WASM
- **La IA es un plugin más** — no un feature hardcodeado, soporta múltiples providers sin conflicto

## Stack técnico

| Componente | Tecnología | Razón |
|---|---|---|
| Lenguaje | Rust | Rendimiento + seguridad de memoria |
| Renderizado | wgpu + winit | GPU nativo, fallback automático |
| UI | iced | Framework maduro en Rust |
| Buffer de texto | ropey (Rope) | Edición O(log n) |
| Syntax | tree-sitter | El mismo que Neovim y Zed |
| LSP | tower-lsp | Autocompletado inteligente |
| Plugins simples | Lua (mlua) | Scripts sin compilar, recarga en caliente |
| Plugins complejos | WASM (wasmtime) | Sandboxed, cualquier lenguaje |

## Estructura del proyecto

```
glyph/
├── crates/
│   ├── glyph-core/         # Buffer, cursores, historial — sin dependencias de UI
│   ├── glyph-renderer/     # Renderizado GPU con wgpu
│   ├── glyph-lsp/          # Cliente LSP
│   ├── glyph-plugin-host/  # Runtime de plugins (Lua + WASM)
│   ├── glyph-plugin-api/   # Contratos WIT — API pública para plugins
│   └── glyph-app/          # Entry point — integra todo
├── plugins/                # Plugins oficiales (usan la misma API que terceros)
│   ├── plugin-theme/
│   ├── plugin-git/
│   └── plugin-formatter/
└── docs/
    └── plugin-sdk.md       # Guía para desarrolladores de plugins
```

## Roadmap

### Milestone 1 — Editor funcional
- [ ] Buffer con Rope (glyph-core)
- [ ] Renderizado básico con wgpu
- [ ] Abrir, editar y guardar archivos
- [ ] Undo/redo

### Milestone 2 — Inteligencia
- [ ] Syntax highlighting con tree-sitter
- [ ] Cliente LSP (autocompletado, diagnósticos)
- [ ] Búsqueda y reemplazo

### Milestone 3 — Plugin System
- [ ] Plugin host con wasmtime
- [ ] Scripting Lua embebido
- [ ] WIT API v1
- [ ] Sistema de permisos declarativo
- [ ] Plugin manager

### Milestone 4 — IA como plugin
- [ ] Provider pattern para modelos de IA
- [ ] Soporte Ollama (local/privado)
- [ ] Routing de IA por tipo de archivo
- [ ] Múltiples agentes sin conflicto

### Milestone 5 — Pulido
- [ ] Temas personalizables
- [ ] Documentación del SDK de plugins
- [ ] Marketplace de plugins

## Contribuir

Glyph está en desarrollo activo y acepta contribuciones. Por favor lee [CONTRIBUTING.md](CONTRIBUTING.md) antes de abrir un PR.

El proyecto usa **Apache 2.0** — puedes usar el código, hacer forks, incluso versiones comerciales. Los cambios no necesitan ser abiertos, pero deben documentarse.

## Licencia

Apache License 2.0 — ver [LICENSE](LICENSE) para detalles.

---

*Glyph — porque cada carácter importa.*
