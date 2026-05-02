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

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicI32, Ordering};
use std::sync::mpsc as std_mpsc;
use std::time::Duration;

use anyhow::Result;
use glyph_core::{
    resaltado::{Lenguaje, Resaltador, TipoResaltado},
    Document,
};
use glyph_lsp::{ClienteLsp, Diagnostic, DiagnosticSeverity, Notificacion, Position, Url};
use glyph_plugin_host::HostPlugins;
use glyph_renderer::{
    ColorRender, ConfigRenderer, ContenidoRender, CursorRender, DiagnosticoRender,
    DireccionCursor, EventoEditor, SeveridadRender, SpanTexto,
};
use tokio::sync::mpsc as tokio_mpsc;

const LINEAS_POR_PAGINA: usize = 20;

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    tracing::info!("Glyph — Every character matters");

    let ruta_archivo: Option<PathBuf> = std::env::args().nth(1).map(PathBuf::from);

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

    let resaltador = Resaltador::nuevo();
    let lenguaje = lenguaje_del_doc(&ruta_archivo);

    let diagnosticos_compartidos: Arc<Mutex<Vec<Diagnostic>>> =
        Arc::new(Mutex::new(Vec::new()));
    let diag_escritor = Arc::clone(&diagnosticos_compartidos);

    let (tx_lsp, rx_lsp) = tokio_mpsc::unbounded_channel::<(Url, String, i32)>();
    let (tx_hover_req, rx_hover_req) =
        tokio_mpsc::unbounded_channel::<(Url, Position, std_mpsc::SyncSender<Option<String>>)>();

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
            rt.block_on(hilo_lsp(uri, texto_inicial, raiz, rx_lsp, rx_hover_req, diag_escritor));
        });
    }

    let version_doc = Arc::new(AtomicI32::new(1));

    let nombre_archivo: Option<String> = ruta_archivo
        .as_ref()
        .and_then(|p| p.file_name())
        .map(|n| n.to_string_lossy().into_owned());

    let barra_inicial = construir_barra_estado(
        &documento,
        nombre_archivo.as_deref(),
        false,
        false,
        "",
        "",
        &[],
        0,
        0,
    );

    let contenido_inicial = documento_a_contenido(
        &documento,
        &resaltador,
        lenguaje,
        &host,
        &diagnosticos_compartidos.lock().unwrap(),
        vec![],
        None,
        barra_inicial,
        None,
    );

    let titulo = nombre_archivo
        .as_ref()
        .map(|n| format!("{n} — Glyph"))
        .unwrap_or_else(|| "Sin título — Glyph".to_string());

    let config = ConfigRenderer {
        titulo,
        ..ConfigRenderer::default()
    };

    let uri_doc: Option<Url> = ruta_archivo
        .as_ref()
        .and_then(|r| r.canonicalize().ok())
        .and_then(|r| Url::from_file_path(r).ok());

    let ruta_str: Option<String> = ruta_archivo
        .as_ref()
        .and_then(|p| p.to_str())
        .map(|s| s.to_string());

    // ── Estado de búsqueda/reemplazo (vive en el closure del event loop) ──
    let mut en_busqueda = false;
    let mut en_reemplazo = false;
    let mut consulta_actual = String::new();
    let mut reemplazo_actual = String::new();
    let mut matches_actuales: Vec<(usize, usize)> = Vec::new();
    let mut match_activo: usize = 0;

    // ── Estado de hover (vive en el closure del event loop) ────────────
    let mut hover_actual: Option<String> = None;

    glyph_renderer::ejecutar(config, contenido_inicial, move |evento| {
        let modifica_texto = matches!(
            evento,
            EventoEditor::InsertarTexto(_)
                | EventoEditor::BorrarAtras
                | EventoEditor::BorrarAdelante
                | EventoEditor::Deshacer
                | EventoEditor::Rehacer
                | EventoEditor::ReemplazarMatch
                | EventoEditor::ReemplazarTodo
        );

        // Cualquier acción que mueve el cursor o modifica el texto descarta el hover
        let descarta_hover = modifica_texto
            || matches!(
                evento,
                EventoEditor::MoverCursor(_) | EventoEditor::MoverCursorA { .. }
            );
        if descarta_hover {
            hover_actual = None;
        }

        match evento {
            EventoEditor::InsertarTexto(texto) => {
                if let Err(e) = documento.insertar_en_cursor(&texto) {
                    tracing::error!("Error insertando texto: {e}");
                    return None;
                }
                // Actualizar matches si hay búsqueda activa
                if en_busqueda {
                    matches_actuales = documento.buscar(&consulta_actual);
                    match_activo = match_activo.min(matches_actuales.len().saturating_sub(1));
                }
            }
            EventoEditor::BorrarAtras => {
                if let Err(e) = documento.borrar_antes_cursor() {
                    tracing::error!("Error borrando: {e}");
                    return None;
                }
                if en_busqueda {
                    matches_actuales = documento.buscar(&consulta_actual);
                    match_activo = match_activo.min(matches_actuales.len().saturating_sub(1));
                }
            }
            EventoEditor::BorrarAdelante => {
                if let Err(e) = documento.borrar_despues_cursor() {
                    tracing::error!("Error borrando: {e}");
                    return None;
                }
                if en_busqueda {
                    matches_actuales = documento.buscar(&consulta_actual);
                    match_activo = match_activo.min(matches_actuales.len().saturating_sub(1));
                }
            }
            EventoEditor::MoverCursor(direccion) => match direccion {
                DireccionCursor::Izquierda => documento.mover_cursor_izquierda(),
                DireccionCursor::Derecha => documento.mover_cursor_derecha(),
                DireccionCursor::Arriba => documento.mover_cursor_arriba(),
                DireccionCursor::Abajo => documento.mover_cursor_abajo(),
                DireccionCursor::InicioLinea => documento.mover_cursor_inicio_linea(),
                DireccionCursor::FinLinea => documento.mover_cursor_fin_linea(),
                DireccionCursor::PaginaArriba => documento.mover_cursor_pagina_arriba(LINEAS_POR_PAGINA),
                DireccionCursor::PaginaAbajo => documento.mover_cursor_pagina_abajo(LINEAS_POR_PAGINA),
                DireccionCursor::InicioDoc => documento.mover_cursor_inicio_doc(),
                DireccionCursor::FinDoc => documento.mover_cursor_fin_doc(),
            },
            EventoEditor::Deshacer => {
                if let Err(e) = documento.deshacer() {
                    tracing::error!("Error al deshacer: {e}");
                    return None;
                }
                if en_busqueda {
                    matches_actuales = documento.buscar(&consulta_actual);
                    match_activo = match_activo.min(matches_actuales.len().saturating_sub(1));
                }
            }
            EventoEditor::Rehacer => {
                if let Err(e) = documento.rehacer() {
                    tracing::error!("Error al rehacer: {e}");
                    return None;
                }
                if en_busqueda {
                    matches_actuales = documento.buscar(&consulta_actual);
                    match_activo = match_activo.min(matches_actuales.len().saturating_sub(1));
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

            // ── Búsqueda ──────────────────────────────────────────────
            EventoEditor::IniciarBusqueda => {
                en_busqueda = true;
                consulta_actual.clear();
                matches_actuales.clear();
                match_activo = 0;
            }
            EventoEditor::ActualizarBusqueda(consulta) => {
                consulta_actual = consulta;
                matches_actuales = documento.buscar(&consulta_actual);
                match_activo = 0;
                if let Some(&(ini, _)) = matches_actuales.first() {
                    documento.mover_cursor_a_byte(ini);
                }
            }
            EventoEditor::SiguienteMatch => {
                if !matches_actuales.is_empty() {
                    match_activo = (match_activo + 1) % matches_actuales.len();
                    let (ini, _) = matches_actuales[match_activo];
                    documento.mover_cursor_a_byte(ini);
                }
            }
            EventoEditor::MatchAnterior => {
                if !matches_actuales.is_empty() {
                    match_activo = match_activo
                        .checked_sub(1)
                        .unwrap_or(matches_actuales.len() - 1);
                    let (ini, _) = matches_actuales[match_activo];
                    documento.mover_cursor_a_byte(ini);
                }
            }
            EventoEditor::TerminarBusqueda => {
                en_busqueda = false;
                en_reemplazo = false;
                consulta_actual.clear();
                reemplazo_actual.clear();
                matches_actuales.clear();
                match_activo = 0;
            }

            // ── Reemplazo ─────────────────────────────────────────────
            EventoEditor::IniciarReemplazo => {
                en_busqueda = true;
                en_reemplazo = true;
                consulta_actual.clear();
                reemplazo_actual.clear();
                matches_actuales.clear();
                match_activo = 0;
            }
            EventoEditor::ActualizarReemplazo(texto) => {
                reemplazo_actual = texto;
            }
            EventoEditor::ReemplazarMatch => {
                if !matches_actuales.is_empty() {
                    let (ini, fin) = matches_actuales[match_activo];
                    if let Err(e) = documento.reemplazar_bytes(ini, fin, &reemplazo_actual) {
                        tracing::error!("Error reemplazando match: {e}");
                    }
                    matches_actuales = documento.buscar(&consulta_actual);
                    match_activo = match_activo.min(matches_actuales.len().saturating_sub(1));
                    if let Some(&(ini, _)) = matches_actuales.get(match_activo) {
                        documento.mover_cursor_a_byte(ini);
                    }
                }
            }
            EventoEditor::ReemplazarTodo => {
                if !matches_actuales.is_empty() {
                    if let Err(e) = documento.reemplazar_todo_bytes(&matches_actuales.clone(), &reemplazo_actual) {
                        tracing::error!("Error en reemplazar todo: {e}");
                    }
                    matches_actuales = documento.buscar(&consulta_actual);
                    match_activo = 0;
                }
            }

            // ── Click de ratón ────────────────────────────────────────
            EventoEditor::MoverCursorA { linea, columna } => {
                documento.mover_cursor_a(linea as usize, columna as usize);
            }

            // ── Hover LSP (Ctrl+K) ────────────────────────────────────
            EventoEditor::PedirHover => {
                if let Some(ref uri) = uri_doc {
                    let pos = documento.cursor_principal().posicion;
                    let position = Position {
                        line: pos.linea as u32,
                        character: pos.columna as u32,
                    };
                    let (tx_resp, rx_resp) = std_mpsc::sync_channel::<Option<String>>(1);
                    if tx_hover_req.send((uri.clone(), position, tx_resp)).is_ok() {
                        hover_actual = rx_resp
                            .recv_timeout(Duration::from_millis(350))
                            .ok()
                            .flatten();
                    } else {
                        tracing::debug!("LSP no disponible para hover");
                    }
                }
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

        let n_errores = {
            let diags = diagnosticos_compartidos.lock().unwrap();
            diags.iter().filter(|d| d.severity == Some(DiagnosticSeverity::ERROR)).count()
        };

        let barra = construir_barra_estado(
            &documento,
            nombre_archivo.as_deref(),
            en_busqueda,
            en_reemplazo,
            &consulta_actual,
            &reemplazo_actual,
            &matches_actuales,
            match_activo,
            n_errores,
        );

        let (m_busqueda, m_activo) = if en_busqueda && !matches_actuales.is_empty() {
            (matches_actuales.clone(), Some(match_activo))
        } else if en_busqueda {
            (vec![], None)
        } else {
            (vec![], None)
        };

        let diags = diagnosticos_compartidos.lock().unwrap();
        Some(documento_a_contenido(
            &documento,
            &resaltador,
            lenguaje,
            &host,
            &diags,
            m_busqueda,
            m_activo,
            barra,
            hover_actual.clone(),
        ))
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
    mut rx_hover: tokio_mpsc::UnboundedReceiver<(Url, Position, std_mpsc::SyncSender<Option<String>>)>,
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
            req = rx_hover.recv() => {
                if let Some((uri, posicion, tx_resp)) = req {
                    let resultado = cliente.hover(uri, posicion).await;
                    let texto = resultado.ok().flatten();
                    tracing::debug!("Hover: {:?}", texto.as_deref().map(|s| &s[..s.len().min(60)]));
                    let _ = tx_resp.send(texto);
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
    matches_busqueda: Vec<(usize, usize)>,
    match_activo: Option<usize>,
    barra_estado: String,
    hover_texto: Option<String>,
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
            color: tipo_a_color(s.tipo, host),
        })
        .collect();

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
        matches_busqueda,
        match_activo,
        barra_estado,
        hover_texto,
    }
}

// ------------------------------------------------------------------
// Barra de estado
// ------------------------------------------------------------------

fn construir_barra_estado(
    doc: &Document,
    nombre_archivo: Option<&str>,
    en_busqueda: bool,
    en_reemplazo: bool,
    consulta: &str,
    reemplazo: &str,
    matches: &[(usize, usize)],
    match_activo: usize,
    n_errores: usize,
) -> String {
    if en_reemplazo {
        let resultados = if matches.is_empty() {
            if consulta.is_empty() {
                String::new()
            } else {
                " — sin resultados".to_string()
            }
        } else {
            format!(" — {}/{}", match_activo + 1, matches.len())
        };
        format!(
            "Buscar: \"{consulta}\"  →  \"{reemplazo}\"{resultados} | Enter: reemplazar, Ctrl+H: todo, Esc: salir"
        )
    } else if en_busqueda {
        if matches.is_empty() {
            if consulta.is_empty() {
                "Buscar: _ | Enter: siguiente, Esc: salir".to_string()
            } else {
                format!("Buscar: \"{consulta}\" — sin resultados | Esc: salir")
            }
        } else {
            format!(
                "Buscar: \"{consulta}\" — {}/{} | Enter: siguiente, Shift+Enter: anterior, Esc: salir",
                match_activo + 1,
                matches.len()
            )
        }
    } else {
        let nombre = nombre_archivo.unwrap_or("Sin título");
        let pos = doc.cursor_principal().posicion;
        let errores = if n_errores > 0 {
            format!(" | {n_errores} error(es) LSP")
        } else {
            String::new()
        };
        format!("{nombre} | Ln {}, Col {}{}", pos.linea + 1, pos.columna + 1, errores)
    }
}

// ------------------------------------------------------------------
// Tema — usa colores del HostPlugins
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
