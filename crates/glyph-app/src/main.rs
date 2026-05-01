// Copyright 2026 Franz (frantsiles)
// Licensed under the Apache License, Version 2.0

//! # glyph-app
//!
//! Entry point del editor Glyph.
//!
//! Responsabilidades de esta capa:
//! - Inicializar logging
//! - Crear el Document desde glyph-core
//! - Convertirlo a ContenidoRender (sin exponer tipos del core al renderer)
//! - Lanzar el renderer

use anyhow::Result;
use glyph_core::Document;
use glyph_renderer::{ConfigRenderer, ContenidoRender, CursorRender};

fn main() -> Result<()> {
    // Logging estructurado — respeta la variable de entorno RUST_LOG
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    tracing::info!("Iniciando Glyph — Every character matters");

    // ── Capa core: crear documento ──────────────────────────────────────
    // En Milestone 2 este documento vendrá de leer un archivo real.
    // Por ahora usamos contenido de prueba para validar el renderer.
    let documento = Document::desde_archivo(
        "// Glyph — Every character matters\n\nfn main() {\n    println!(\"¡Hola, Glyph!\");\n}\n",
        std::path::PathBuf::from("main.rs"),
    );

    // ── Conversión core → renderer (sin acoplamiento directo) ───────────
    // El renderer no conoce Document ni Buffer — recibe ContenidoRender.
    let contenido = documento_a_contenido(&documento);

    // ── Capa renderer: arrancar la ventana ──────────────────────────────
    let config = ConfigRenderer::default();
    glyph_renderer::ejecutar(config, contenido)
}

/// Convierte un Document del core en el DTO que necesita el renderer.
///
/// Este es el único punto del sistema donde ambas capas se tocan.
/// En el futuro esta función vivirá en un módulo `editor` dedicado
/// que gestione el estado completo del editor (pestañas, layout…).
fn documento_a_contenido(doc: &Document) -> ContenidoRender {
    let cursor_principal = doc.cursor_principal();

    let lineas: Vec<String> = doc
        .buffer
        .contenido_completo()
        .lines()
        .map(|l| l.to_string())
        .collect();

    ContenidoRender {
        lineas,
        cursor: Some(CursorRender {
            linea: cursor_principal.posicion.linea as u32,
            columna: cursor_principal.posicion.columna as u32,
        }),
        tamano_fuente: 16.0,
    }
}
