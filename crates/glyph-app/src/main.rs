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
//! ## Capas
//!
//! ```text
//! glyph-core        — Document, Buffer, Cursor, Resaltador
//! glyph-lsp         — ClienteLsp (hilo tokio independiente)
//! glyph-plugin-host — HostPlugins: carga y ejecuta plugins Lua
//! plugin-theme      — script Lua con el tema One Dark
//! glyph-app         — orquesta todo, sin conocer detalles internos de cada capa
//! glyph-renderer    — ventana GPU, event loop winit
//! ```

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicI32, Ordering};

use anyhow::Result;
use glyph_core::{
    resaltado::{Lenguaje, Resaltador, TipoResaltado},
    Document,
};
use glyph_lsp::{ClienteLsp, Diagnostic, DiagnosticSeverity, Notificacion, Url};
use glyph_plugin_host::HostPlugins;
use glyph_renderer::{
    ColorRender, ConfigRenderer, ContenidoRender, CursorRender, DiagnosticoRender,
    DireccionCursor, EventoEditor, SeveridadRender, SpanTexto,
};
use tokio::sync::mpsc as tokio_mpsc;

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

    // ── Plugin host — cargar tema desde Lua ───────────────────────────────
    let mut host = HostPlugins::nuevo();
    if let Err(e) = host.cargar_lua(plugin_theme::NOMBRE, plugin_theme::TEMA_SCRIPT) {
        tracing::warn!("No se pudo cargar el tema Lua: {e} — usando tema por defecto");
    }
    host.inicializar();
    host.al_abrir(ruta_archivo.as_ref().and_then(|p| p.to_str()));

    // ── Resaltador + lenguaje ─────────────────────────────────────────────
    let resaltador = Resaltador::nuevo();
    let lenguaje = lenguaje_del_doc(&ruta_archivo);

    // ── Diagnósticos compartidos entre hilo LSP y event loop ─────────────
    let diagnosticos_compartidos: Arc<Mutex<Vec<Diagnostic>>> =
        Arc::new(Mutex::new(Vec::new()));
    let diag_escritor = Arc::clone(&diagnosticos_compartidos);

    // ── Canal LSP: cambios del editor → hilo LSP ─────────────────────────
    let (tx_lsp, rx_lsp) = tokio_mpsc::unbounded_channel::<(Url, String, i32)>();

    // ── Hilo LSP ──────────────────────────────────────────────────────────
    if let Some(ref ruta) = ruta_archivo {
        let ruta_lsp = ruta.canonicalize().unwrap_or_else(|_| ruta.clone());
        let uri = Url::from_file_path(&ruta_lsp).ok();
        let raiz = ruta_lsp
            .parent()
            .unwrap_or_else(|| std::path::Path::new("."))
            .to_path_buf();
        let texto_inicial = documento.buffer.contenido_completo();

        std::thread::spawn(move || {
            let rt = tokio::runtime::Runtime::new().expect("runtime tokio para LSP");
            rt.block_on(hilo_lsp(uri, texto_inicial, raiz, rx_lsp, diag_escritor));
        });
    }

    // ── Versión del documento ─────────────────────────────────────────────
    let version_doc = Arc::new(AtomicI32::new(1));

    // ── Contenido inicial ─────────────────────────────────────────────────
    let contenido_inicial = documento_a_contenido(
        &documento,
        &resaltador,
        lenguaje,
        &host,
        &diagnosticos_compartidos.lock().unwrap(),
    );

    // ── Título de ventana ─────────────────────────────────────────────────
    let titulo = ruta_archivo
        .as_ref()
        .and_then(|p| p.file_name())
        .map(|n| format!("{} — Glyph", n.to_string_lossy()))
        .unwrap_or_else(|| "Sin título — Glyph".to_string());

    let config = ConfigRenderer {
        titulo,
        ..ConfigRenderer::default()
    };

    // ── URI del documento para el LSP ─────────────────────────────────────
    let uri_doc: Option<Url> = ruta_archivo
        .as_ref()
        .and_then(|r| r.canonicalize().ok())
        .and_then(|r| Url::from_file_path(r).ok());

    let ruta_str: Option<String> = ruta_archivo
        .as_ref()
        .and_then(|p| p.to_str())
        .map(|s| s.to_string());

    // ── Event loop ────────────────────────────────────────────────────────
    glyph_renderer::ejecutar(config, contenido_inicial, move |evento| {
        let modifica_texto = matches!(
            evento,
            EventoEditor::InsertarTexto(_)
                | EventoEditor::BorrarAtras
                | EventoEditor::BorrarAdelante
                | EventoEditor::Deshacer
                | EventoEditor::Rehacer
        );

        match evento {
            EventoEditor::InsertarTexto(texto) => {
                if let Err(e) = documento.insertar_en_cursor(&texto) {
                    tracing::error!("Error insertando texto: {e}");
                    return None;
                }
            }
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
            EventoEditor::MoverCursor(direccion) => match direccion {
                DireccionCursor::Izquierda => documento.mover_cursor_izquierda(),
                DireccionCursor::Derecha => documento.mover_cursor_derecha(),
                DireccionCursor::Arriba => documento.mover_cursor_arriba(),
                DireccionCursor::Abajo => documento.mover_cursor_abajo(),
                DireccionCursor::InicioLinea => documento.mover_cursor_inicio_linea(),
                DireccionCursor::FinLinea => documento.mover_cursor_fin_linea(),
            },
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
            EventoEditor::Guardar => {
                let ruta = documento.buffer.ruta.clone();
                match ruta {
                    Some(ruta) => {
                        let contenido = documento.buffer.contenido_completo();
                        match std::fs::write(&ruta, contenido.as_bytes()) {
                            Ok(()) => {
                                documento.buffer.marcar_guardado();
                                tracing::info!("Guardado: {}", ruta.display());
                                host.al_guardar(ruta.to_str());
                            }
                            Err(e) => tracing::error!("Error guardando {}: {e}", ruta.display()),
                        }
                    }
                    None => tracing::warn!("Sin ruta — abre un archivo con: glyph <archivo>"),
                }
                return None;
            }
        }

        // Notificar cambio al LSP y al plugin host
        if modifica_texto {
            if let Some(ref uri) = uri_doc {
                let version = version_doc.fetch_add(1, Ordering::Relaxed);
                let texto = documento.buffer.contenido_completo();
                let _ = tx_lsp.send((uri.clone(), texto, version));
                host.al_cambiar(ruta_str.as_deref(), version as u32);
            }
        }

        let diags = diagnosticos_compartidos.lock().unwrap();
        Some(documento_a_contenido(&documento, &resaltador, lenguaje, &host, &diags))
    })
}

// ------------------------------------------------------------------
// Hilo LSP (runtime Tokio independiente)
// ------------------------------------------------------------------

async fn hilo_lsp(
    uri: Option<Url>,
    texto_inicial: String,
    raiz: PathBuf,
    mut rx: tokio_mpsc::UnboundedReceiver<(Url, String, i32)>,
    diag_escritor: Arc<Mutex<Vec<Diagnostic>>>,
) {
    let mut cliente = match ClienteLsp::conectar("rust-analyzer", &[], &raiz).await {
        Ok(c) => c,
        Err(e) => {
            tracing::warn!("LSP no disponible (rust-analyzer no encontrado): {e}");
            return;
        }
    };

    if let Some(ref uri) = uri {
        if let Err(e) = cliente.abrir_documento(uri.clone(), &texto_inicial, 0).await {
            tracing::warn!("LSP didOpen falló: {e}");
        }
    }

    loop {
        tokio::select! {
            msg = rx.recv() => {
                match msg {
                    Some((uri, texto, version)) => {
                        if let Err(e) = cliente.cambiar_documento(uri, &texto, version).await {
                            tracing::warn!("LSP didChange falló: {e}");
                        }
                    }
                    None => break,
                }
            }
            Some(notif) = cliente.rx_notificacion.recv() => {
                match notif {
                    Notificacion::Diagnosticos(params) => {
                        registrar_diagnosticos(&params.diagnostics);
                        *diag_escritor.lock().unwrap() = params.diagnostics;
                    }
                }
            }
        }
    }
}

fn registrar_diagnosticos(diags: &[Diagnostic]) {
    for d in diags {
        let nivel = match d.severity {
            Some(DiagnosticSeverity::ERROR)       => "ERROR",
            Some(DiagnosticSeverity::WARNING)     => "WARN",
            Some(DiagnosticSeverity::INFORMATION) => "INFO",
            _                                     => "HINT",
        };
        let l = d.range.start.line + 1;
        let c = d.range.start.character + 1;
        tracing::warn!("[LSP {nivel}] {l}:{c} — {}", d.message);
    }
}

// ------------------------------------------------------------------
// Conversión Document → ContenidoRender
// ------------------------------------------------------------------

fn documento_a_contenido(
    doc: &Document,
    resaltador: &Resaltador,
    lenguaje: Lenguaje,
    host: &HostPlugins,
    diagnosticos_lsp: &[Diagnostic],
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

    // Sintaxis con colores del tema activo en el host
    let spans: Vec<SpanTexto> = resaltador
        .resaltar(&texto_completo, lenguaje)
        .into_iter()
        .map(|s| SpanTexto {
            inicio_byte: s.inicio_byte,
            fin_byte: s.fin_byte,
            color: tipo_a_color(s.tipo, host),
        })
        .collect();

    // Diagnósticos LSP → byte positions
    let diagnosticos: Vec<DiagnosticoRender> = diagnosticos_lsp
        .iter()
        .map(|d| {
            let inicio_byte =
                posicion_lsp_a_byte(&lineas, d.range.start.line, d.range.start.character);
            let fin_byte =
                posicion_lsp_a_byte(&lineas, d.range.end.line, d.range.end.character);
            DiagnosticoRender {
                inicio_byte,
                fin_byte: fin_byte.max(inicio_byte + 1),
                severidad: match d.severity {
                    Some(DiagnosticSeverity::ERROR)       => SeveridadRender::Error,
                    Some(DiagnosticSeverity::WARNING)     => SeveridadRender::Aviso,
                    Some(DiagnosticSeverity::INFORMATION) => SeveridadRender::Informacion,
                    _                                     => SeveridadRender::Sugerencia,
                },
                mensaje: d.message.clone(),
            }
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
        diagnosticos,
    }
}

// ------------------------------------------------------------------
// Tema — usa colores del HostPlugins en lugar de valores hardcodeados
// ------------------------------------------------------------------

fn tipo_a_color(tipo: TipoResaltado, host: &HostPlugins) -> ColorRender {
    let clave = match tipo {
        TipoResaltado::PalabraClave   => "keyword",
        TipoResaltado::CadenaTexto    => "string",
        TipoResaltado::Comentario     => "comment",
        TipoResaltado::Funcion        => "function",
        TipoResaltado::Tipo           => "type",
        TipoResaltado::Numero         => "number",
        TipoResaltado::Operador       => "operator",
        TipoResaltado::Variable       => "variable",
        TipoResaltado::Constante      => "constant",
        TipoResaltado::Puntuacion     => "punctuation",
        TipoResaltado::Atributo       => "attribute",
        TipoResaltado::Predeterminado => "default",
    };
    let [r, g, b] = host.color(clave);
    ColorRender::rgb(r, g, b)
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

/// Convierte una posición LSP (línea, carácter UTF-16) a byte offset
/// en el string `lineas.join("\n")`.
fn posicion_lsp_a_byte(lineas: &[String], linea: u32, caracter_utf16: u32) -> usize {
    let li = (linea as usize).min(lineas.len().saturating_sub(1));
    let mut offset: usize = lineas[..li].iter().map(|l| l.len() + 1).sum();

    if let Some(linea_str) = lineas.get(li) {
        let mut utf16_count = 0u32;
        for (byte_idx, ch) in linea_str.char_indices() {
            if utf16_count >= caracter_utf16 {
                return offset + byte_idx;
            }
            utf16_count += ch.len_utf16() as u32;
        }
        offset += linea_str.len();
    }
    offset
}
