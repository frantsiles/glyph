// Copyright 2026 Franz (frantsiles)
// Licensed under the Apache License, Version 2.0

//! # glyph-renderer
//!
//! Renderer GPU del editor Glyph.
//!
//! ## Uso desde glyph-app
//!
//! ```no_run
//! use glyph_renderer::{ConfigRenderer, ContenidoRender, EventoEditor, ejecutar};
//!
//! let config = ConfigRenderer::default();
//! let contenido = ContenidoRender::vacio();
//!
//! ejecutar(config, contenido, |evento| {
//!     // manejar evento y devolver contenido actualizado
//!     None
//! }).unwrap();
//! ```

pub mod configuracion;
pub mod contenido;
pub mod eventos;

mod gpu;
mod renderer;
mod texto;

pub use configuracion::ConfigRenderer;
pub use contenido::{ColorRender, ContenidoRender, CursorRender, SpanTexto};
pub use eventos::{DireccionCursor, EventoEditor};
pub use renderer::Renderer;

use anyhow::Result;

/// Punto de entrada principal: inicializa el renderer y arranca el event loop.
///
/// `manejador` se llama en cada evento de teclado. Devuelve `Some(nuevo_contenido)`
/// si el editor debe redibujar, o `None` si no hay cambios.
pub fn ejecutar<F>(config: ConfigRenderer, contenido: ContenidoRender, manejador: F) -> Result<()>
where
    F: FnMut(EventoEditor) -> Option<ContenidoRender> + 'static,
{
    Renderer::nuevo(config, contenido).ejecutar(manejador)
}
