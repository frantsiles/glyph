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
//! - Aplicar el tema de colores (One Dark) sobre los spans del resaltador

use anyhow::Result;
use glyph_core::{
    resaltado::{Lenguaje, Resaltador, TipoResaltado},
    Document,
};
use glyph_renderer::{
    ColorRender, ConfigRenderer, ContenidoRender, CursorRender, DireccionCursor, EventoEditor,
    SpanTexto,
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

    // ── Resaltador + lenguaje ─────────────────────────────────────────────
    let resaltador = Resaltador::nuevo();
    let lenguaje = lenguaje_del_doc(&ruta_archivo);

    // ── Contenido inicial ─────────────────────────────────────────────────
    let contenido_inicial = documento_a_contenido(&documento, &resaltador, lenguaje);

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
                return None;
            }
        }

        Some(documento_a_contenido(&documento, &resaltador, lenguaje))
    })
}

// ------------------------------------------------------------------
// Conversión Document → ContenidoRender
// ------------------------------------------------------------------

/// Único punto de contacto entre glyph-core y glyph-renderer.
fn documento_a_contenido(
    doc: &Document,
    resaltador: &Resaltador,
    lenguaje: Lenguaje,
) -> ContenidoRender {
    let cursor = doc.cursor_principal();
    let texto_completo = doc.buffer.contenido_completo();

    let lineas: Vec<String> = texto_completo.lines().map(|l| l.to_string()).collect();

    let lineas = if texto_completo.ends_with('\n') {
        let mut v = lineas;
        v.push(String::new());
        v
    } else {
        lineas
    };

    let spans: Vec<SpanTexto> = resaltador
        .resaltar(&texto_completo, lenguaje)
        .into_iter()
        .map(|s| SpanTexto {
            inicio_byte: s.inicio_byte,
            fin_byte: s.fin_byte,
            color: tipo_a_color(s.tipo),
        })
        .collect();

    ContenidoRender {
        lineas,
        cursor: Some(CursorRender {
            linea: cursor.posicion.linea as u32,
            columna: cursor.posicion.columna as u32,
        }),
        tamano_fuente: 16.0,
        spans,
    }
}

// ------------------------------------------------------------------
// Tema One Dark
// ------------------------------------------------------------------

fn tipo_a_color(tipo: TipoResaltado) -> ColorRender {
    match tipo {
        TipoResaltado::PalabraClave   => ColorRender::rgb(0xC6, 0x78, 0xDD), // morado
        TipoResaltado::CadenaTexto    => ColorRender::rgb(0x98, 0xC3, 0x79), // verde
        TipoResaltado::Comentario     => ColorRender::rgb(0x5C, 0x63, 0x70), // gris
        TipoResaltado::Funcion        => ColorRender::rgb(0x61, 0xAF, 0xEF), // azul
        TipoResaltado::Tipo           => ColorRender::rgb(0xE5, 0xC0, 0x7B), // amarillo
        TipoResaltado::Numero         => ColorRender::rgb(0xD1, 0x9A, 0x66), // naranja
        TipoResaltado::Operador       => ColorRender::rgb(0x56, 0xB6, 0xC2), // cian
        TipoResaltado::Variable       => ColorRender::rgb(0xE0, 0x6C, 0x75), // rojo
        TipoResaltado::Constante      => ColorRender::rgb(0xD1, 0x9A, 0x66), // naranja
        TipoResaltado::Puntuacion     => ColorRender::rgb(0xAB, 0xB2, 0xBF), // gris claro
        TipoResaltado::Atributo       => ColorRender::rgb(0xE5, 0xC0, 0x7B), // amarillo
        TipoResaltado::Predeterminado => ColorRender::rgb(0xAB, 0xB2, 0xBF), // gris claro
    }
}

// ------------------------------------------------------------------
// Helpers
// ------------------------------------------------------------------

fn lenguaje_del_doc(ruta: &Option<PathBuf>) -> Lenguaje {
    ruta.as_ref()
        .and_then(|p| p.extension())
        .and_then(|e| e.to_str())
        .map(Lenguaje::desde_extension)
        .unwrap_or(Lenguaje::Desconocido)
}
