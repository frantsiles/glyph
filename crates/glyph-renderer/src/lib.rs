// Copyright 2026 Franz (frantsiles)
// Licensed under the Apache License, Version 2.0

//! # glyph-renderer
//!
//! Renderer GPU del editor Glyph.
//!
//! ## Uso desde glyph-app
//!
//! ```no_run
//! use glyph_renderer::{ConfigRenderer, ContenidoRender, ejecutar};
//!
//! let config = ConfigRenderer::default();
//! let contenido = ContenidoRender::vacio();
//! ejecutar(config, contenido).unwrap();
//! ```
//!
//! ## Arquitectura
//!
//! ```text
//! lib.rs         → API pública + función ejecutar()
//! configuracion  → ConfigRenderer (parámetros de ventana y fuente)
//! contenido      → ContenidoRender (DTO entre app y renderer)
//! gpu            → ContextoGpu (wgpu: device, queue, surface)
//! texto          → RendererTexto (glyphon + cosmic-text)
//! renderer       → Renderer (event loop winit 0.29)
//! ```

pub mod configuracion;
pub mod contenido;

mod gpu;
mod renderer;
mod texto;

pub use configuracion::ConfigRenderer;
pub use contenido::{ContenidoRender, CursorRender};
pub use renderer::Renderer;

use anyhow::Result;

/// Punto de entrada principal: inicializa el renderer y arranca el event loop.
///
/// Esta función bloquea hasta que el usuario cierra la ventana.
pub fn ejecutar(config: ConfigRenderer, contenido: ContenidoRender) -> Result<()> {
    Renderer::nuevo(config, contenido).ejecutar()
}
