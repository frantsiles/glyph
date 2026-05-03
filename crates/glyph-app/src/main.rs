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

mod config_app;

use config_app::ConfigApp;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicI32, Ordering};
use std::sync::mpsc as std_mpsc;
use std::time::Duration;

use anyhow::Result;
use arboard::Clipboard;
use glyph_core::{
    resaltado::{Lenguaje, Resaltador, TipoResaltado},
    Document,
};
use glyph_lsp::{ClienteLsp, Diagnostic, DiagnosticSeverity, Notificacion, Position, Url};
use glyph_plugin_api;
use glyph_plugin_host::HostPlugins;
use glyph_renderer::{
    ColorRender, ConfigRenderer, ContenidoRender, CursorRender, DiagnosticoRender,
    DireccionCursor, EventoEditor, LineaSeccionRender, SeccionContenidoRender,
    SeveridadRender, SpanTexto, TabInfo,
};
use tokio::sync::mpsc as tokio_mpsc;

const LINEAS_POR_PAGINA: usize = 20;

// ------------------------------------------------------------------
// Sistema de tabs
// ------------------------------------------------------------------

struct TabDoc {
    documento: Document,
    nombre: String,
    ruta: Option<PathBuf>,
    uri: Option<Url>,
    lenguaje: Lenguaje,
    en_busqueda: bool,
    en_reemplazo: bool,
    consulta: String,
    reemplazo_str: String,
    matches: Vec<(usize, usize)>,
    match_activo: usize,
    modificado: bool,
}

impl TabDoc {
    fn nuevo_vacio(nombre: String) -> Self {
        Self {
            documento: Document::nuevo(),
            nombre,
            ruta: None,
            uri: None,
            lenguaje: Lenguaje::Desconocido,
            en_busqueda: false,
            en_reemplazo: false,
            consulta: String::new(),
            reemplazo_str: String::new(),
            matches: Vec::new(),
            match_activo: 0,
            modificado: false,
        }
    }
}

struct GestorTabs {
    tabs: Vec<TabDoc>,
    activo: usize,
    contador: usize,
}

impl GestorTabs {
    fn nuevo_con(tab: TabDoc) -> Self {
        Self { tabs: vec![tab], activo: 0, contador: 1 }
    }

    fn abrir_tab_vacio(&mut self) {
        self.contador += 1;
        self.tabs.push(TabDoc::nuevo_vacio(format!("Sin título {}", self.contador)));
        self.activo = self.tabs.len() - 1;
    }

    fn cerrar_activo(&mut self) {
        if self.tabs.len() == 1 {
            self.contador += 1;
            self.tabs[0] = TabDoc::nuevo_vacio(format!("Sin título {}", self.contador));
            return;
        }
        self.tabs.remove(self.activo);
        if self.activo >= self.tabs.len() {
            self.activo = self.tabs.len() - 1;
        }
    }

    fn activar(&mut self, idx: usize) {
        if idx < self.tabs.len() {
            self.activo = idx;
        }
    }

    fn siguiente(&mut self) {
        self.activo = (self.activo + 1) % self.tabs.len();
    }

    fn anterior(&mut self) {
        self.activo = self.activo.checked_sub(1).unwrap_or(self.tabs.len() - 1);
    }

    fn tab(&self) -> &TabDoc { &self.tabs[self.activo] }
    fn tab_mut(&mut self) -> &mut TabDoc { &mut self.tabs[self.activo] }

    fn infos_tabs(&self) -> Vec<TabInfo> {
        self.tabs.iter().enumerate().map(|(i, t)| TabInfo {
            nombre: t.nombre.clone(),
            activo: i == self.activo,
            modificado: t.modificado,
        }).collect()
    }
}

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    tracing::info!("Glyph — Every character matters");

    let cfg = ConfigApp::cargar();

    let ruta_archivo: Option<PathBuf> = std::env::args().nth(1).map(PathBuf::from);

    let tab_inicial = if let Some(ref ruta) = ruta_archivo {
        let contenido_fs = if ruta.exists() {
            tracing::info!("Abriendo: {}", ruta.display());
            std::fs::read_to_string(ruta)?
        } else {
            tracing::info!("Archivo nuevo: {}", ruta.display());
            String::new()
        };
        let nombre = ruta.file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "Sin título".into());
        let uri = ruta.canonicalize().ok()
            .and_then(|r| Url::from_file_path(r).ok());
        let lenguaje = Lenguaje::desde_extension(
            ruta.extension().and_then(|e| e.to_str()).unwrap_or(""),
        );
        TabDoc {
            documento: Document::desde_archivo(&contenido_fs, ruta.clone()),
            nombre,
            ruta: Some(ruta.clone()),
            uri,
            lenguaje,
            en_busqueda: false, en_reemplazo: false,
            consulta: String::new(), reemplazo_str: String::new(),
            matches: Vec::new(), match_activo: 0, modificado: false,
        }
    } else {
        tracing::info!("Buffer vacío (sin archivo)");
        TabDoc::nuevo_vacio("Sin título 1".into())
    };

    let mut gestor = GestorTabs::nuevo_con(tab_inicial);

    // ── Plugin host ───────────────────────────────────────────────────────
    let mut host = HostPlugins::nuevo();
    if let Err(e) = host.cargar_lua(plugin_theme::NOMBRE, plugin_theme::TEMA_SCRIPT) {
        tracing::warn!("No se pudo cargar el tema Lua: {e} — usando tema por defecto");
    }
    if let Err(e) = host.cargar_lua(plugin_sidebar::NOMBRE, plugin_sidebar::SCRIPT) {
        tracing::warn!("No se pudo cargar plugin-sidebar: {e}");
    }
    host.inicializar();
    host.al_abrir(gestor.tab().ruta.as_ref().and_then(|p| p.to_str()));

    let resaltador = Resaltador::nuevo();

    let diagnosticos_compartidos: Arc<Mutex<Vec<Diagnostic>>> =
        Arc::new(Mutex::new(Vec::new()));
    let diag_escritor = Arc::clone(&diagnosticos_compartidos);

    let (tx_lsp, rx_lsp) = tokio_mpsc::unbounded_channel::<(Url, String, i32)>();
    let (tx_hover_req, rx_hover_req) =
        tokio_mpsc::unbounded_channel::<(Url, Position, std_mpsc::SyncSender<Option<String>>)>();

    // LSP solo para el tab inicial (si tiene archivo)
    if let Some(ref uri) = gestor.tab().uri {
        let ruta = gestor.tab().ruta.clone().unwrap();
        let ruta_lsp = ruta.canonicalize().unwrap_or_else(|_| ruta.clone());
        let uri_lsp = uri.clone();
        let raiz = ruta_lsp
            .parent()
            .unwrap_or_else(|| std::path::Path::new("."))
            .to_path_buf();
        let texto_inicial = gestor.tab().documento.buffer.contenido_completo();

        std::thread::spawn(move || {
            let rt = tokio::runtime::Runtime::new().expect("runtime tokio para LSP");
            rt.block_on(hilo_lsp(Some(uri_lsp), texto_inicial, raiz, rx_lsp, rx_hover_req, diag_escritor));
        });
    }

    let version_doc = Arc::new(AtomicI32::new(1));

    let barra_inicial = construir_barra_estado(
        &gestor.tab().documento,
        Some(&gestor.tab().nombre),
        false, false, "", "", &[], 0, 0,
    );

    let contenido_inicial = {
        let diags = diagnosticos_compartidos.lock().unwrap();
        documento_a_contenido(
            &gestor.tab().documento,
            &resaltador,
            gestor.tab().lenguaje,
            &host,
            &diags,
            vec![],
            None,
            barra_inicial,
            None,
            gestor.infos_tabs(),
        )
    };

    let titulo = format!("{} — Glyph", gestor.tab().nombre);

    // Determinar configuración por tipo de archivo
    let ext = ruta_archivo
        .as_ref()
        .and_then(|p| p.extension())
        .and_then(|e| e.to_str())
        .unwrap_or("");
    let cfg_lang = cfg.para_extension(ext);

    let config = ConfigRenderer {
        titulo,
        ancho: cfg.ventana.ancho,
        alto: cfg.ventana.alto,
        tamano_fuente: cfg_lang.tamano_fuente.unwrap_or(cfg.editor.tamano_fuente),
        multiplicador_linea: cfg.editor.interlineado,
        familia_fuente: cfg_lang.familia_fuente.or(cfg.editor.familia_fuente),
        tamano_tab: cfg_lang.tamano_tab.unwrap_or(4),
    };

    // ── Estado de hover (vive en el closure del event loop) ────────────
    let mut hover_actual: Option<String> = None;

    // ── Portapapeles del sistema ────────────────────────────────────────
    let mut clipboard = Clipboard::new().ok();

    glyph_renderer::ejecutar(config, contenido_inicial, move |evento| {
        // ── Eventos de navegación de tabs ─────────────────────────────────
        match evento {
            EventoEditor::NuevoTab => {
                gestor.abrir_tab_vacio();
                hover_actual = None;
                return Some(construir_contenido(&gestor, &resaltador, &host,
                    &diagnosticos_compartidos, None));
            }
            EventoEditor::CerrarTab => {
                gestor.cerrar_activo();
                hover_actual = None;
                return Some(construir_contenido(&gestor, &resaltador, &host,
                    &diagnosticos_compartidos, None));
            }
            EventoEditor::SiguienteTab => {
                gestor.siguiente();
                hover_actual = None;
                return Some(construir_contenido(&gestor, &resaltador, &host,
                    &diagnosticos_compartidos, None));
            }
            EventoEditor::AnteriorTab => {
                gestor.anterior();
                hover_actual = None;
                return Some(construir_contenido(&gestor, &resaltador, &host,
                    &diagnosticos_compartidos, None));
            }
            EventoEditor::ActivarTab(idx) => {
                gestor.activar(idx);
                hover_actual = None;
                return Some(construir_contenido(&gestor, &resaltador, &host,
                    &diagnosticos_compartidos, None));
            }
            _ => {}
        }

        let modifica_texto = matches!(
            evento,
            EventoEditor::InsertarTexto(_)
                | EventoEditor::BorrarAtras
                | EventoEditor::BorrarAdelante
                | EventoEditor::Deshacer
                | EventoEditor::Rehacer
                | EventoEditor::ReemplazarMatch
                | EventoEditor::ReemplazarTodo
                | EventoEditor::Cortar
                | EventoEditor::Pegar
        );

        // Cualquier acción que mueve el cursor o modifica el texto descarta el hover
        let descarta_hover = modifica_texto
            || matches!(
                evento,
                EventoEditor::MoverCursor(_)
                    | EventoEditor::MoverCursorA { .. }
                    | EventoEditor::ExtenderSeleccion(_)
                    | EventoEditor::SeleccionarTodo
            );
        if descarta_hover {
            hover_actual = None;
        }

        match evento {
            EventoEditor::InsertarTexto(texto) => {
                let tab = gestor.tab_mut();
                if let Err(e) = tab.documento.insertar_en_cursor(&texto) {
                    tracing::error!("Error insertando texto: {e}");
                    return None;
                }
                tab.modificado = true;
                if tab.en_busqueda {
                    let consulta = tab.consulta.clone();
                    tab.matches = tab.documento.buscar(&consulta);
                    tab.match_activo = tab.match_activo.min(tab.matches.len().saturating_sub(1));
                }
            }
            EventoEditor::BorrarAtras => {
                let tab = gestor.tab_mut();
                if let Err(e) = tab.documento.borrar_antes_cursor() {
                    tracing::error!("Error borrando: {e}");
                    return None;
                }
                tab.modificado = true;
                if tab.en_busqueda {
                    let consulta = tab.consulta.clone();
                    tab.matches = tab.documento.buscar(&consulta);
                    tab.match_activo = tab.match_activo.min(tab.matches.len().saturating_sub(1));
                }
            }
            EventoEditor::BorrarAdelante => {
                let tab = gestor.tab_mut();
                if let Err(e) = tab.documento.borrar_despues_cursor() {
                    tracing::error!("Error borrando: {e}");
                    return None;
                }
                tab.modificado = true;
                if tab.en_busqueda {
                    let consulta = tab.consulta.clone();
                    tab.matches = tab.documento.buscar(&consulta);
                    tab.match_activo = tab.match_activo.min(tab.matches.len().saturating_sub(1));
                }
            }
            EventoEditor::MoverCursor(direccion) => {
                let doc = &mut gestor.tab_mut().documento;
                match direccion {
                    DireccionCursor::Izquierda   => doc.mover_cursor_izquierda(),
                    DireccionCursor::Derecha      => doc.mover_cursor_derecha(),
                    DireccionCursor::Arriba       => doc.mover_cursor_arriba(),
                    DireccionCursor::Abajo        => doc.mover_cursor_abajo(),
                    DireccionCursor::InicioLinea  => doc.mover_cursor_inicio_linea(),
                    DireccionCursor::FinLinea     => doc.mover_cursor_fin_linea(),
                    DireccionCursor::PaginaArriba => doc.mover_cursor_pagina_arriba(LINEAS_POR_PAGINA),
                    DireccionCursor::PaginaAbajo  => doc.mover_cursor_pagina_abajo(LINEAS_POR_PAGINA),
                    DireccionCursor::InicioDoc    => doc.mover_cursor_inicio_doc(),
                    DireccionCursor::FinDoc       => doc.mover_cursor_fin_doc(),
                }
            }
            EventoEditor::Deshacer => {
                let tab = gestor.tab_mut();
                if let Err(e) = tab.documento.deshacer() {
                    tracing::error!("Error al deshacer: {e}");
                    return None;
                }
                if tab.en_busqueda {
                    let consulta = tab.consulta.clone();
                    tab.matches = tab.documento.buscar(&consulta);
                    tab.match_activo = tab.match_activo.min(tab.matches.len().saturating_sub(1));
                }
            }
            EventoEditor::Rehacer => {
                let tab = gestor.tab_mut();
                if let Err(e) = tab.documento.rehacer() {
                    tracing::error!("Error al rehacer: {e}");
                    return None;
                }
                if tab.en_busqueda {
                    let consulta = tab.consulta.clone();
                    tab.matches = tab.documento.buscar(&consulta);
                    tab.match_activo = tab.match_activo.min(tab.matches.len().saturating_sub(1));
                }
            }
            EventoEditor::Guardar => {
                let tab = gestor.tab_mut();
                let ruta = tab.documento.buffer.ruta.clone();
                match ruta {
                    Some(ruta) => {
                        let contenido = tab.documento.buffer.contenido_completo();
                        match std::fs::write(&ruta, contenido.as_bytes()) {
                            Ok(()) => {
                                tab.documento.buffer.marcar_guardado();
                                tab.modificado = false;
                                tracing::info!("Guardado: {}", ruta.display());
                                let acciones_plugin = host.al_guardar(ruta.to_str(), None);
                                procesar_acciones_plugin(
                                    acciones_plugin,
                                    &mut gestor,
                                    &mut host,
                                    &mut hover_actual,
                                    &diagnosticos_compartidos,
                                    &resaltador,
                                    &tx_lsp,
                                    &version_doc,
                                );
                            }
                            Err(e) => tracing::error!("Error guardando {}: {e}", ruta.display()),
                        }
                    }
                    None => tracing::warn!("Sin ruta — abre un archivo con: glyph <archivo>"),
                }
                return Some(construir_contenido(&gestor, &resaltador, &host,
                    &diagnosticos_compartidos, hover_actual.clone()));
            }

            // ── Búsqueda ──────────────────────────────────────────────
            EventoEditor::IniciarBusqueda => {
                let tab = gestor.tab_mut();
                tab.en_busqueda = true;
                tab.consulta.clear();
                tab.matches.clear();
                tab.match_activo = 0;
            }
            EventoEditor::ActualizarBusqueda(consulta) => {
                let tab = gestor.tab_mut();
                tab.consulta = consulta;
                let q = tab.consulta.clone();
                tab.matches = tab.documento.buscar(&q);
                tab.match_activo = 0;
                if let Some(&(ini, _)) = tab.matches.first() {
                    tab.documento.mover_cursor_a_byte(ini);
                }
            }
            EventoEditor::SiguienteMatch => {
                let tab = gestor.tab_mut();
                if !tab.matches.is_empty() {
                    tab.match_activo = (tab.match_activo + 1) % tab.matches.len();
                    let ini = tab.matches[tab.match_activo].0;
                    tab.documento.mover_cursor_a_byte(ini);
                }
            }
            EventoEditor::MatchAnterior => {
                let tab = gestor.tab_mut();
                if !tab.matches.is_empty() {
                    tab.match_activo = tab.match_activo
                        .checked_sub(1).unwrap_or(tab.matches.len() - 1);
                    let ini = tab.matches[tab.match_activo].0;
                    tab.documento.mover_cursor_a_byte(ini);
                }
            }
            EventoEditor::TerminarBusqueda => {
                let tab = gestor.tab_mut();
                tab.en_busqueda = false;
                tab.en_reemplazo = false;
                tab.consulta.clear();
                tab.reemplazo_str.clear();
                tab.matches.clear();
                tab.match_activo = 0;
            }

            // ── Reemplazo ─────────────────────────────────────────────
            EventoEditor::IniciarReemplazo => {
                let tab = gestor.tab_mut();
                tab.en_busqueda = true;
                tab.en_reemplazo = true;
                tab.consulta.clear();
                tab.reemplazo_str.clear();
                tab.matches.clear();
                tab.match_activo = 0;
            }
            EventoEditor::ActualizarReemplazo(texto) => {
                gestor.tab_mut().reemplazo_str = texto;
            }
            EventoEditor::ReemplazarMatch => {
                let tab = gestor.tab_mut();
                if !tab.matches.is_empty() {
                    let (ini, fin) = tab.matches[tab.match_activo];
                    let reemplazo = tab.reemplazo_str.clone();
                    if let Err(e) = tab.documento.reemplazar_bytes(ini, fin, &reemplazo) {
                        tracing::error!("Error reemplazando match: {e}");
                    }
                    let consulta = tab.consulta.clone();
                    tab.matches = tab.documento.buscar(&consulta);
                    tab.match_activo = tab.match_activo.min(tab.matches.len().saturating_sub(1));
                    if let Some(&(i, _)) = tab.matches.get(tab.match_activo) {
                        tab.documento.mover_cursor_a_byte(i);
                    }
                    tab.modificado = true;
                }
            }
            EventoEditor::ReemplazarTodo => {
                let tab = gestor.tab_mut();
                if !tab.matches.is_empty() {
                    let reemplazo = tab.reemplazo_str.clone();
                    if let Err(e) = tab.documento.reemplazar_todo_bytes(&tab.matches.clone(), &reemplazo) {
                        tracing::error!("Error en reemplazar todo: {e}");
                    }
                    let consulta = tab.consulta.clone();
                    tab.matches = tab.documento.buscar(&consulta);
                    tab.match_activo = 0;
                    tab.modificado = true;
                }
            }

            // ── Click de ratón ────────────────────────────────────────
            EventoEditor::MoverCursorA { linea, columna } => {
                gestor.tab_mut().documento.mover_cursor_a(linea as usize, columna as usize);
            }

            // ── Selección y clipboard ─────────────────────────────────
            EventoEditor::SeleccionarTodo => {
                gestor.tab_mut().documento.seleccionar_todo();
            }
            EventoEditor::ExtenderSeleccion(dir) => {
                let doc = &mut gestor.tab_mut().documento;
                match dir {
                    DireccionCursor::Izquierda  => doc.mover_cursor_izquierda_sel(),
                    DireccionCursor::Derecha     => doc.mover_cursor_derecha_sel(),
                    DireccionCursor::Arriba      => doc.mover_cursor_arriba_sel(),
                    DireccionCursor::Abajo       => doc.mover_cursor_abajo_sel(),
                    DireccionCursor::InicioLinea => doc.mover_cursor_inicio_linea_sel(),
                    DireccionCursor::FinLinea    => doc.mover_cursor_fin_linea_sel(),
                    _ => {}
                }
            }
            EventoEditor::Copiar => {
                if let Some(texto) = gestor.tab().documento.texto_seleccionado() {
                    if let Some(cb) = &mut clipboard {
                        let _ = cb.set_text(&texto);
                    }
                }
                return None;
            }
            EventoEditor::Cortar => {
                let texto = gestor.tab().documento.texto_seleccionado();
                if let Some(texto) = texto {
                    if let Some(cb) = &mut clipboard {
                        let _ = cb.set_text(&texto);
                    }
                    let tab = gestor.tab_mut();
                    tab.documento.borrar_seleccion();
                    tab.modificado = true;
                    if tab.en_busqueda {
                        let consulta = tab.consulta.clone();
                        tab.matches = tab.documento.buscar(&consulta);
                        tab.match_activo = tab.match_activo.min(tab.matches.len().saturating_sub(1));
                    }
                } else {
                    return None;
                }
            }
            EventoEditor::Pegar => {
                let texto_pegado = clipboard.as_mut()
                    .and_then(|cb| cb.get_text().ok())
                    .unwrap_or_default();
                if texto_pegado.is_empty() { return None; }
                let tab = gestor.tab_mut();
                if let Err(e) = tab.documento.insertar_en_cursor(&texto_pegado) {
                    tracing::error!("Error pegando texto: {e}");
                    return None;
                }
                tab.modificado = true;
                if tab.en_busqueda {
                    let consulta = tab.consulta.clone();
                    tab.matches = tab.documento.buscar(&consulta);
                    tab.match_activo = tab.match_activo.min(tab.matches.len().saturating_sub(1));
                }
            }

            // ── Hover LSP (Ctrl+K) ────────────────────────────────────
            EventoEditor::PedirHover => {
                if let Some(ref uri) = gestor.tab().uri {
                    let pos = gestor.tab().documento.cursor_principal().posicion;
                    let position = Position {
                        line: pos.linea as u32,
                        character: pos.columna as u32,
                    };
                    let (tx_resp, rx_resp) = std_mpsc::sync_channel::<Option<String>>(1);
                    if tx_hover_req.send((uri.clone(), position, tx_resp)).is_ok() {
                        hover_actual = rx_resp
                            .recv_timeout(Duration::from_millis(350))
                            .ok().flatten();
                    }
                }
            }

            // ── Evento de sección de plugin ───────────────────────────
            EventoEditor::EventoSeccion { id_seccion, linea } => {
                let acciones = host.evento_seccion(&id_seccion, linea);
                for accion in acciones {
                    if let glyph_plugin_api::AccionPlugin::AbrirArchivo(ruta) = accion {
                        abrir_archivo_en_tab(&mut gestor, &ruta, &host, &diagnosticos_compartidos,
                            &resaltador, &tx_lsp, &mut hover_actual);
                    }
                }
                return Some(construir_contenido(&gestor, &resaltador, &host,
                    &diagnosticos_compartidos, hover_actual.clone()));
            }

            // ToggleSidebar es manejado por el renderer directamente
            EventoEditor::ToggleSidebar => {
                return Some(construir_contenido(&gestor, &resaltador, &host,
                    &diagnosticos_compartidos, hover_actual.clone()));
            }

            // Tab events already handled above
            _ => {}
        }

        // Notificar cambio al LSP y al plugin host
        if modifica_texto {
            if let Some(ref uri) = gestor.tab().uri {
                let version = version_doc.fetch_add(1, Ordering::Relaxed);
                let texto = gestor.tab().documento.buffer.contenido_completo();
                let _ = tx_lsp.send((uri.clone(), texto, version));
                let ruta_str = gestor.tab().ruta.as_ref()
                    .and_then(|p| p.to_str()).map(|s| s.to_string());
                let acciones_plugin = host.al_cambiar(ruta_str.as_deref(), version as u32, None);
                procesar_acciones_plugin(
                    acciones_plugin,
                    &mut gestor,
                    &mut host,
                    &mut hover_actual,
                    &diagnosticos_compartidos,
                    &resaltador,
                    &tx_lsp,
                    &version_doc,
                );
            }
        }

        Some(construir_contenido(&gestor, &resaltador, &host,
            &diagnosticos_compartidos, hover_actual.clone()))
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

fn construir_contenido(
    gestor: &GestorTabs,
    resaltador: &Resaltador,
    host: &HostPlugins,
    diagnosticos: &Arc<Mutex<Vec<Diagnostic>>>,
    hover: Option<String>,
) -> ContenidoRender {
    let tab = gestor.tab();
    let diags = diagnosticos.lock().unwrap();
    let n_errores = diags.iter()
        .filter(|d| d.severity == Some(DiagnosticSeverity::ERROR))
        .count();
    let barra = construir_barra_estado(
        &tab.documento,
        Some(&tab.nombre),
        tab.en_busqueda,
        tab.en_reemplazo,
        &tab.consulta,
        &tab.reemplazo_str,
        &tab.matches,
        tab.match_activo,
        n_errores,
    );
    let (m_busqueda, m_activo) = if tab.en_busqueda && !tab.matches.is_empty() {
        (tab.matches.clone(), Some(tab.match_activo))
    } else {
        (vec![], None)
    };
    documento_a_contenido(
        &tab.documento,
        resaltador,
        tab.lenguaje,
        host,
        &diags,
        m_busqueda,
        m_activo,
        barra,
        hover,
        gestor.infos_tabs(),
    )
}

/// Abre un archivo en un nuevo tab (o activa el existente si ya está abierto).
fn abrir_archivo_en_tab(
    gestor: &mut GestorTabs,
    ruta_str: &str,
    _host: &HostPlugins,
    _diagnosticos: &Arc<Mutex<Vec<Diagnostic>>>,
    _resaltador: &Resaltador,
    _tx_lsp: &tokio_mpsc::UnboundedSender<(Url, String, i32)>,
    hover_actual: &mut Option<String>,
) {
    let ruta = std::path::PathBuf::from(ruta_str);

    // Si ya está abierto, activarlo
    if let Some(idx) = gestor.tabs.iter().position(|t| t.ruta.as_ref() == Some(&ruta)) {
        gestor.activar(idx);
        *hover_actual = None;
        return;
    }

    // Leer el archivo
    let contenido_fs = if ruta.exists() {
        std::fs::read_to_string(&ruta).unwrap_or_default()
    } else {
        String::new()
    };
    let nombre = ruta.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| ruta_str.to_string());
    let uri = ruta.canonicalize().ok()
        .and_then(|r| Url::from_file_path(r).ok());
    let lenguaje = glyph_core::resaltado::Lenguaje::desde_extension(
        ruta.extension().and_then(|e| e.to_str()).unwrap_or(""),
    );
    let tab = TabDoc {
        documento: glyph_core::Document::desde_archivo(&contenido_fs, ruta.clone()),
        nombre,
        ruta: Some(ruta),
        uri,
        lenguaje,
        en_busqueda: false, en_reemplazo: false,
        consulta: String::new(), reemplazo_str: String::new(),
        matches: Vec::new(), match_activo: 0, modificado: false,
    };
    gestor.tabs.push(tab);
    gestor.activo = gestor.tabs.len() - 1;
    *hover_actual = None;
}

fn procesar_acciones_plugin(
    acciones: Vec<glyph_plugin_api::AccionPlugin>,
    gestor: &mut GestorTabs,
    host: &mut HostPlugins,
    hover_actual: &mut Option<String>,
    diagnosticos: &Arc<Mutex<Vec<Diagnostic>>>,
    resaltador: &Resaltador,
    tx_lsp: &tokio_mpsc::UnboundedSender<(Url, String, i32)>,
    version_doc: &AtomicI32,
) {
    for accion in acciones {
        match accion {
            glyph_plugin_api::AccionPlugin::AbrirArchivo(ruta) => {
                abrir_archivo_en_tab(
                    gestor,
                    &ruta,
                    host,
                    diagnosticos,
                    resaltador,
                    tx_lsp,
                    hover_actual,
                );
            }
            glyph_plugin_api::AccionPlugin::ReemplazarContenidoBuffer { contenido, origen_plugin } => {
                let tab = gestor.tab_mut();
                tab.documento.buffer.reemplazar_todo(&contenido);
                tab.modificado = true;
                if tab.en_busqueda {
                    let consulta = tab.consulta.clone();
                    tab.matches = tab.documento.buscar(&consulta);
                    tab.match_activo = tab.match_activo.min(tab.matches.len().saturating_sub(1));
                }
                if let Some(ref uri) = tab.uri {
                    let version = version_doc.fetch_add(1, Ordering::Relaxed);
                    let texto = tab.documento.buffer.contenido_completo();
                    let _ = tx_lsp.send((uri.clone(), texto, version as i32));
                    let ruta_str = tab.ruta.as_ref().and_then(|p| p.to_str()).map(|s| s.to_string());
                    let acciones_siguientes = host.al_cambiar(ruta_str.as_deref(), version as u32, Some(&origen_plugin));
                    if !acciones_siguientes.is_empty() {
                        procesar_acciones_plugin(
                            acciones_siguientes,
                            gestor,
                            host,
                            hover_actual,
                            diagnosticos,
                            resaltador,
                            tx_lsp,
                            version_doc,
                        );
                    }
                }
            }
            glyph_plugin_api::AccionPlugin::DecorarLineas(_lineas) => {
                // Por ahora solo reconocemos la acción; el renderizado se implementará en el siguiente paso.
            }
            _ => {}
        }
    }
}

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
    tabs: Vec<TabInfo>,
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

    let secciones_plugin: Vec<SeccionContenidoRender> = host.secciones_para_render()
        .into_iter()
        .map(|s| SeccionContenidoRender {
            id: s.id,
            lado: s.lado,
            tamano: s.tamano,
            color_fondo: s.color_fondo,
            lineas: s.lineas.into_iter().map(|l| LineaSeccionRender {
                texto: l.texto,
                color: l.color,
                negrita: l.negrita,
            }).collect(),
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
        seleccion_bytes: doc.seleccion_bytes(),
        tabs,
        secciones_plugin,
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
        let sel = if doc.tiene_seleccion() {
            let n = doc.texto_seleccionado().map(|t| t.chars().count()).unwrap_or(0);
            format!(" | Sel: {n}")
        } else {
            String::new()
        };
        let errores = if n_errores > 0 {
            format!(" | {n_errores} error(es)")
        } else {
            String::new()
        };
        format!("{nombre} | Ln {}, Col {}{}{}", pos.linea + 1, pos.columna + 1, sel, errores)
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
