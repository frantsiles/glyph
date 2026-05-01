// Copyright 2026 Franz (frantsiles)
// Licensed under the Apache License, Version 2.0

//! # glyph-lsp
//!
//! Cliente LSP asíncrono para el editor Glyph.
//!
//! ## Uso básico
//!
//! ```no_run
//! use glyph_lsp::{ClienteLsp, Notificacion};
//! use std::path::Path;
//!
//! #[tokio::main]
//! async fn main() {
//!     let mut cliente = ClienteLsp::conectar(
//!         "rust-analyzer",
//!         &[],
//!         Path::new("/mi/proyecto"),
//!     ).await.unwrap();
//!
//!     // Escuchar diagnósticos
//!     while let Some(notif) = cliente.rx_notificacion.recv().await {
//!         match notif {
//!             Notificacion::Diagnosticos(d) => println!("{d:?}"),
//!         }
//!     }
//! }
//! ```

pub mod cliente;
mod transporte;

pub use cliente::{ClienteLsp, Notificacion};

// Re-exportamos los tipos LSP más usados para que los consumidores
// no necesiten declarar lsp-types como dependencia directa.
pub use lsp_types::{Diagnostic, DiagnosticSeverity, Position, PublishDiagnosticsParams, Url};
