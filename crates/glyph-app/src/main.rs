// Copyright 2026 Franz (frantsiles)
// Licensed under the Apache License, Version 2.0

//! # glyph-app
//!
//! Entry point del editor Glyph.
//!
//! ## Uso
//!
//! ```
//! glyph                  # abre un buffer vacío
//! glyph archivo.rs       # abre un archivo existente (o crea uno nuevo)
//! ```
//!
//! ## Responsabilidades de esta capa
//!
//! - Inicializar logging
//! - Cargar el archivo desde la CLI (o crear buffer vacío)
//! - Traducir `EventoEditor` → operaciones en `Document` → `ContenidoRender`
//! - Guardar el archivo en disco (el core no tiene I/O)

use anyhow::Result;
use glyph_core::Document;
use glyph_renderer::{
    ConfigRenderer, ContenidoRender, CursorRender, DireccionCursor, EventoEditor,
};
use std::path::PathBuf;

fn main() -> Result<()> {
    // ── Logging ──────────────────────────────────────────────────────────
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    tracing::info!("Glyph — Every character matters");

    // ── Argumento de archivo ──────────────────────────────────────────────
    let ruta_archivo: Option<PathBuf> = std::env::args().nth(1).map(PathBuf::from);

    // ── Cargar documento ──────────────────────────────────────────────────
    let mut documento = if let Some(ref ruta) = ruta_archivo {
        if ruta.exists() {
            let contenido = std::fs::read_to_string(ruta)?;
            tracing::info!("Abriendo: {}", ruta.display());
            Document::desde_archivo(&contenido, ruta.clone())
        } else {
            tracing::info!("Archivo nuevo: {}", ruta.display());
            Document::desde_archivo("", ruta.clone())
        }
    } else {
        tracing::info!("Buffer vacío (sin archivo)");
        Document::nuevo()
    };

    // ── Contenido inicial ─────────────────────────────────────────────────
    let contenido_inicial = documento_a_contenido(&documento);

    // ── Título de ventana con nombre de archivo ───────────────────────────
    let titulo = ruta_archivo
        .as_ref()
        .and_then(|p| p.file_name())
        .map(|n| format!("{} — Glyph", n.to_string_lossy()))
        .unwrap_or_else(|| "Sin título — Glyph".to_string());

    let config = ConfigRenderer {
        titulo,
        ..ConfigRenderer::default()
    };

    // ── Event loop ────────────────────────────────────────────────────────
    // El manejador vive aquí y captura `documento` por move.
    // Es el único punto donde glyph-core y glyph-renderer se tocan.
    glyph_renderer::ejecutar(config, contenido_inicial, move |evento| {
        match evento {
            // ── Inserción ──────────────────────────────────────────────
            EventoEditor::InsertarTexto(texto) => {
                if let Err(e) = documento.insertar_en_cursor(&texto) {
                    tracing::error!("Error insertando texto: {e}");
                    return None;
                }
            }

            // ── Borrado ────────────────────────────────────────────────
            EventoEditor::BorrarAtras => {
                if let Err(e) = documento.borrar_antes_cursor() {
                    tracing::error!("Error borrando: {e}");
                    return None;
                }
            }

            EventoEditor::BorrarAdelante => {
                if let Err(e) = documento.borrar_despues_cursor() {
                    tracing::error!("Error borrando: {e}");
                    return None;
                }
            }

            // ── Movimiento ─────────────────────────────────────────────
            EventoEditor::MoverCursor(direccion) => {
                match direccion {
                    DireccionCursor::Izquierda => documento.mover_cursor_izquierda(),
                    DireccionCursor::Derecha => documento.mover_cursor_derecha(),
                    DireccionCursor::Arriba => documento.mover_cursor_arriba(),
                    DireccionCursor::Abajo => documento.mover_cursor_abajo(),
                    DireccionCursor::InicioLinea => documento.mover_cursor_inicio_linea(),
                    DireccionCursor::FinLinea => documento.mover_cursor_fin_linea(),
                }
            }

            // ── Undo / Redo ────────────────────────────────────────────
            EventoEditor::Deshacer => {
                if let Err(e) = documento.deshacer() {
                    tracing::error!("Error al deshacer: {e}");
                    return None;
                }
            }

            EventoEditor::Rehacer => {
                if let Err(e) = documento.rehacer() {
                    tracing::error!("Error al rehacer: {e}");
                    return None;
                }
            }

            // ── Guardar ────────────────────────────────────────────────
            EventoEditor::Guardar => {
                // Clonar la ruta para liberar el borrow antes de llamar marcar_guardado()
                let ruta = documento.buffer.ruta.clone();
                match ruta {
                    Some(ruta) => {
                        let contenido = documento.buffer.contenido_completo();
                        match std::fs::write(&ruta, contenido.as_bytes()) {
                            Ok(()) => {
                                documento.buffer.marcar_guardado();
                                tracing::info!("Guardado: {}", ruta.display());
                            }
                            Err(e) => tracing::error!("Error guardando {}: {e}", ruta.display()),
                        }
                    }
                    None => tracing::warn!("Sin ruta — abre un archivo con: glyph <archivo>"),
                }
                return None; // guardar no necesita redibujado
            }
        }

        // Cualquier cambio en el documento produce nuevo contenido para el renderer
        Some(documento_a_contenido(&documento))
    })
}

// ------------------------------------------------------------------
// Conversión Document → ContenidoRender
// ------------------------------------------------------------------

/// Único punto de contacto entre glyph-core y glyph-renderer.
///
/// Construye el DTO que el renderer necesita sin exponer tipos del core.
fn documento_a_contenido(doc: &Document) -> ContenidoRender {
    let cursor = doc.cursor_principal();
    let lineas: Vec<String> = doc
        .buffer
        .contenido_completo()
        .lines()
        .map(|l| l.to_string())
        .collect();

    // Si el documento termina en \n, lines() omite la línea vacía final.
    // La agregamos para que el cursor pueda posicionarse en ella.
    let lineas = if doc.buffer.contenido_completo().ends_with('\n') {
        let mut v = lineas;
        v.push(String::new());
        v
    } else {
        lineas
    };

    ContenidoRender {
        lineas,
        cursor: Some(CursorRender {
            linea: cursor.posicion.linea as u32,
            columna: cursor.posicion.columna as u32,
        }),
        tamano_fuente: 16.0,
    }
}
