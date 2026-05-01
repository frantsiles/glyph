// Copyright 2026 Franz (frantsiles)
// Licensed under the Apache License, Version 2.0

//! # glyph-core
//!
//! Núcleo del editor Glyph — sin ninguna dependencia de UI.
//!
//! El resto del sistema (renderer, LSP, plugins) interactúa con
//! los tipos de este crate, nunca al revés.

pub mod buffer;
pub mod cursor;
pub mod document;
pub mod history;
pub mod resaltado;

pub use buffer::{Buffer, Codificacion, FinDeLinea};
pub use cursor::{Cursor, Posicion, Selection};
pub use document::Document;
pub use history::{Historia, Operacion};
pub use resaltado::{Lenguaje, Resaltador, SpanSintactico, TipoResaltado};
