// Copyright 2026 Franz (frantsiles)
// Licensed under the Apache License, Version 2.0

//! # Renderer
//!
//! Event loop principal del editor (winit 0.29).
//!
//! ## Modos del renderer
//!
//! - **Normal** — edición de texto estándar.
//! - **Busqueda** — Ctrl+F: los caracteres alimentan la consulta de búsqueda.
//! - **Reemplazo** — Ctrl+H: dos campos (buscar/reemplazar). Tab cambia el campo activo.
//!
//! ## Teclas
//!
//! | Tecla          | Modo         | Evento                          |
//! |---|---|---|
//! | Ctrl+B         | Normal       | ToggleSidebar                   |
//! | Caracteres     | Normal       | InsertarTexto                   |
//! | Enter          | Normal       | InsertarTexto("\n")             |
//! | Tab            | Normal       | InsertarTexto("    ")           |
//! | Backspace      | Normal       | BorrarAtras                     |
//! | Delete         | Normal       | BorrarAdelante                  |
//! | Flechas        | Normal       | MoverCursor                     |
//! | Home/End       | Normal       | InicioLinea/FinLinea            |
//! | PgUp/PgDn      | Normal       | PaginaArriba/PaginaAbajo        |
//! | Ctrl+Home/End  | Normal       | InicioDoc/FinDoc                |
//! | Ctrl+S/Z/Y/F/H | Normal      | Guardar/Deshacer/Rehacer/...    |
//! | Click izq.     | Normal       | MoverCursorA / EventoSeccion    |
//! | Caracteres     | Búsqueda     | ActualizarBusqueda              |
//! | Enter          | Búsqueda     | SiguienteMatch                  |
//! | Shift+Enter    | Búsqueda     | MatchAnterior                   |
//! | Escape         | Búsqueda     | TerminarBusqueda                |
//! | Caracteres     | Reemplazo    | ActualizarBusqueda/Reemplazo    |
//! | Tab            | Reemplazo    | cambia campo activo (sin evento)|
//! | Enter          | Reemplazo    | ReemplazarMatch                 |
//! | Ctrl+H         | Reemplazo    | ReemplazarTodo                  |
//! | Escape         | Reemplazo    | TerminarBusqueda                |

use std::sync::Arc;

use anyhow::Result;
use winit::{
    dpi::PhysicalSize,
    event::{ElementState, Event, MouseButton, MouseScrollDelta, WindowEvent},
    event_loop::{ControlFlow, EventLoop},
    keyboard::{Key, ModifiersState, NamedKey},
    window::{CursorIcon, WindowBuilder},
};

use crate::{
    configuracion::ConfigRenderer,
    contenido::ContenidoRender,
    eventos::{DireccionCursor, EventoEditor},
    gpu::ContextoGpu,
    quads::{ColorRgba, QuadRenderer, RectPx},
    seccion::{LadoLayout, LayoutManager, SeccionUI, TamanoPref},
    texto::RendererTexto,
};

#[derive(Debug, PartialEq, Eq)]
enum ModoRenderer {
    Normal,
    Busqueda,
    Reemplazo,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CampoReemplazo {
    Buscar,
    Reemplazar,
}

// Color del fondo de la sidebar: ligeramente más oscuro que el editor
const COLOR_SIDEBAR_FONDO: ColorRgba = ColorRgba { r: 0.098, g: 0.098, b: 0.153, a: 1.0 };
const COLOR_SIDEBAR_DIVIDER: ColorRgba = ColorRgba { r: 0.52, g: 0.56, b: 0.74, a: 0.9 };
const COLOR_SIDEBAR_HANDLE: ColorRgba = ColorRgba { r: 0.72, g: 0.76, b: 0.96, a: 0.75 };
// Color de la barra de tabs
const COLOR_TABS_FONDO: ColorRgba = ColorRgba { r: 0.078, g: 0.078, b: 0.122, a: 1.0 };
// Color de la statusbar
const COLOR_STATUS_FONDO: ColorRgba = ColorRgba { r: 0.078, g: 0.078, b: 0.122, a: 1.0 };

const ALTURA_TABS_PX: f32 = 32.0;
const ALTURA_STATUS_PX: f32 = 22.0;
const ANCHO_SIDEBAR_PX: f32 = 240.0;

pub struct Renderer {
    config: ConfigRenderer,
    contenido: ContenidoRender,
}

impl Renderer {
    pub fn nuevo(config: ConfigRenderer, contenido: ContenidoRender) -> Self {
        Self { config, contenido }
    }

    pub fn ejecutar<F>(self, mut manejador: F) -> Result<()>
    where
        F: FnMut(EventoEditor) -> Option<ContenidoRender> + 'static,
    {
        let Self { config, contenido } = self;

        let event_loop = EventLoop::new()?;
        let window = Arc::new(
            WindowBuilder::new()
                .with_title(&config.titulo)
                .with_inner_size(PhysicalSize::new(config.ancho, config.alto))
                .build(&event_loop)?,
        );

        let mut gpu = pollster::block_on(ContextoGpu::nuevo(window.clone()))?;
        let mut texto = RendererTexto::nuevo(
            &gpu.dispositivo,
            &gpu.cola,
            gpu.config_superficie.format,
            config.tamano_fuente,
            config.multiplicador_linea,
            config.familia_fuente.as_deref(),
        );
        let mut quads = QuadRenderer::nuevo(&gpu.dispositivo, gpu.config_superficie.format);

        // Layout manager con secciones del sistema
        let mut layout = LayoutManager::nuevo();
        layout.registrar(SeccionUI {
            id: "tabs".into(), lado: LadoLayout::Arriba,
            tamano_pref: TamanoPref::Fijo(ALTURA_TABS_PX),
            visible: true, z_order: 0, color_fondo: Some(COLOR_TABS_FONDO),
            con_foco: false,
        });
        layout.registrar(SeccionUI {
            id: "statusbar".into(), lado: LadoLayout::Abajo,
            tamano_pref: TamanoPref::Fijo(ALTURA_STATUS_PX),
            visible: true, z_order: 1, color_fondo: Some(COLOR_STATUS_FONDO),
            con_foco: false,
        });
        layout.registrar(SeccionUI {
            id: "sidebar".into(), lado: LadoLayout::Izquierda,
            tamano_pref: TamanoPref::Fijo(ANCHO_SIDEBAR_PX),
            visible: false, z_order: 10, color_fondo: Some(COLOR_SIDEBAR_FONDO),
            con_foco: false,
        });
        layout.registrar(SeccionUI {
            id: "editor_area".into(), lado: LadoLayout::Centro,
            tamano_pref: TamanoPref::Flex(1.0),
            visible: true, z_order: 2, color_fondo: None,
            con_foco: true,
        });

        let tamano_tab = config.tamano_tab;
        let linea_alto = config.tamano_fuente * config.multiplicador_linea;
        let mostrar_borde_foco = config.mostrar_borde_foco;

        tracing::info!(
            "Glyph iniciado — {}×{} | {}pt",
            config.ancho, config.alto, config.tamano_fuente
        );

        let mut mods = ModifiersState::default();
        let mut contenido = contenido;
        let mut modo = ModoRenderer::Normal;
        let mut consulta = String::new();
        let mut reemplazo = String::new();
        let mut campo = CampoReemplazo::Buscar;
        let mut pos_raton = (0.0f32, 0.0f32);
        let mut sidebar_resizing = false;
        let mut sidebar_ancho = ANCHO_SIDEBAR_PX;
        let mut seccion_con_foco = "editor_area".to_string();

        window.request_redraw();

        event_loop.run(move |event, elwt| {
            elwt.set_control_flow(ControlFlow::Wait);

            match event {
                Event::WindowEvent { event, window_id } if window_id == window.id() => {
                    match event {
                        WindowEvent::CloseRequested => {
                            tracing::info!("Ventana cerrada — saliendo");
                            elwt.exit();
                        }
                        WindowEvent::Resized(t) => {
                            gpu.redimensionar(t.width, t.height);
                            window.request_redraw();
                        }
                        WindowEvent::ScaleFactorChanged { .. } => {
                            let t = window.inner_size();
                            gpu.redimensionar(t.width, t.height);
                            window.request_redraw();
                        }
                        WindowEvent::ModifiersChanged(m) => {
                            mods = m.state();
                        }

                        // ── Posición del ratón ────────────────────────────
                        WindowEvent::CursorMoved { position, .. } => {
                            pos_raton = (position.x as f32, position.y as f32);
                            if sidebar_resizing {
                                let (mx, _) = pos_raton;
                                if let Some(rect) = layout.rect_seccion("sidebar") {
                                    let nuevo_ancho = (mx - rect.x).clamp(120.0, 560.0);
                                    sidebar_ancho = nuevo_ancho;
                                    if let Some(sec) = layout.secciones().iter().find(|s| s.id == "sidebar").cloned() {
                                        layout.registrar(SeccionUI {
                                            id: sec.id.clone(),
                                            lado: sec.lado.clone(),
                                            tamano_pref: TamanoPref::Fijo(sidebar_ancho),
                                            visible: sec.visible,
                                            z_order: sec.z_order,
                                            color_fondo: sec.color_fondo,
                                            con_foco: sec.con_foco,
                                        });
                                    }
                                    window.request_redraw();
                                }
                            } else {
                                // Cambiar cursor si está sobre el borde de redimensionamiento
                                let (mx, my) = pos_raton;
                                let ancho = gpu.config_superficie.width as f32;
                                let alto = gpu.config_superficie.height as f32;
                                let _ = layout.calcular(ancho, alto);
                                let mut cursor_resize = false;
                                if let Some(rect) = layout.rect_seccion("sidebar").copied() {
                                    let borde_izq = rect.x + rect.ancho - 8.0;
                                    let borde_der = rect.x + rect.ancho + 2.0;
                                    if mx >= borde_izq && mx <= borde_der && my >= rect.y && my <= rect.y + rect.alto {
                                        cursor_resize = true;
                                    }
                                }
                                if cursor_resize {
                                    window.set_cursor_icon(CursorIcon::EwResize);
                                } else {
                                    window.set_cursor_icon(CursorIcon::Default);
                                }
                            }
                        }

                        // ── Rueda del ratón → scroll ──────────────────────
                        WindowEvent::MouseWheel { delta, .. }
                            if modo == ModoRenderer::Normal =>
                        {
                            let lineas = match delta {
                                MouseScrollDelta::LineDelta(_, y) => {
                                    if y > 0.01 { -3 } else if y < -0.01 { 3 } else { 0 }
                                }
                                MouseScrollDelta::PixelDelta(pos) => {
                                    if pos.y > 0.001 { 3 } else if pos.y < -0.001 { -3 } else { 0 }
                                }
                            };
                            if lineas != 0 {
                                let ancho = gpu.config_superficie.width as f32;
                                let alto = gpu.config_superficie.height as f32;
                                let _ = layout.calcular(ancho, alto);
                                if let Some(id) = layout.seccion_en_posicion(pos_raton.0, pos_raton.1).cloned() {
                                    if id == "sidebar" {
                                        texto.ajustar_scroll_sidebar(lineas);
                                    } else {
                                        texto.ajustar_scroll(lineas);
                                    }
                                } else {
                                    texto.ajustar_scroll(lineas);
                                }
                                window.request_redraw();
                            }
                        }

                        // ── Click izquierdo ───────────────────────────────
                        WindowEvent::MouseInput {
                            state: ElementState::Pressed,
                            button: MouseButton::Left,
                            ..
                        } if modo == ModoRenderer::Normal => {
                            let (mx, my) = pos_raton;
                            let ancho = gpu.config_superficie.width as f32;
                            let alto = gpu.config_superficie.height as f32;

                            // Calcular layout para saber qué sección se clickeó
                            let soluciones = layout.calcular(ancho, alto);
                            let _ = soluciones; // calcular actualiza el estado interno

                            let mut atajo_redimension = false;
                            if let Some(rect) = layout.rect_seccion("sidebar").copied() {
                                let borde_izq = rect.x + rect.ancho - 8.0;
                                let borde_der = rect.x + rect.ancho + 2.0;
                                if mx >= borde_izq && mx <= borde_der && my >= rect.y && my <= rect.y + rect.alto {
                                    sidebar_resizing = true;
                                    atajo_redimension = true;
                                }
                            }

                            if !atajo_redimension {
                                if let Some(id) = layout.seccion_en_posicion(mx, my).cloned() {
                                    // Cambiar foco si se clickeó una sección diferente
                                    if id != seccion_con_foco {
                                        // Quitar foco de la sección anterior
                                        if let Some(sec) = layout.secciones().iter().find(|s| s.id == seccion_con_foco).cloned() {
                                            layout.registrar(SeccionUI {
                                                con_foco: false,
                                                ..sec
                                            });
                                        }
                                        
                                        // Establecer foco en la nueva sección
                                        if let Some(sec) = layout.secciones().iter().find(|s| s.id == id).cloned() {
                                            layout.registrar(SeccionUI {
                                                con_foco: true,
                                                ..sec
                                            });
                                        }
                                        
                                        seccion_con_foco = id.clone();
                                        
                                        // Emitir evento de cambio de foco
                                        if let Some(nuevo) = manejador(EventoEditor::CambioFoco(id.clone())) {
                                            contenido = nuevo;
                                            window.request_redraw();
                                        }
                                    }
                                    
                                    if id == "tabs" {
                                        // Click en la barra de tabs
                                        if let Some(idx) = texto.tab_en_posicion(mx) {
                                            if let Some(nuevo) = manejador(EventoEditor::ActivarTab(idx)) {
                                                contenido = nuevo;
                                                window.request_redraw();
                                            }
                                        }
                                    } else if id == "editor_area" {
                                        // Click en el área del editor (gutter + editor)
                                        let sidebar_ancho = sidebar_ancho_actual(&layout);
                                        let gutter = texto.ancho_gutter();
                                        let scroll = texto.scroll_linea();
                                        let char_ancho = config.tamano_fuente * 0.601;
                                        let tx = mx - sidebar_ancho - gutter - 4.0;
                                        let ty = my - texto.altura_tabs() - 8.0;
                                        if tx >= 0.0 && ty >= 0.0 {
                                            let linea = (ty / linea_alto) as i32 + scroll;
                                            let col = (tx / char_ancho) as u32;
                                            let ev = EventoEditor::MoverCursorA {
                                                linea: linea.max(0) as u32,
                                                columna: col,
                                            };
                                            if let Some(nuevo) = manejador(ev) {
                                                contenido = nuevo;
                                                window.request_redraw();
                                            }
                                        }
                                    } else {
                                        // Click en una sección de plugin
                                        if let Some(rect) = layout.rect_seccion(&id).copied() {
                                            let y_rel = my - rect.y;
                                            let linea = (y_rel / linea_alto) as u32;
                                            let ev = EventoEditor::EventoSeccion {
                                                id_seccion: id,
                                                linea,
                                            };
                                            if let Some(nuevo) = manejador(ev) {
                                                contenido = nuevo;
                                                window.request_redraw();
                                            }
                                        }
                                    }
                                }
                            }
                        }

                        WindowEvent::MouseInput {
                            state: ElementState::Released,
                            button: MouseButton::Left,
                            ..
                        } if modo == ModoRenderer::Normal => {
                            sidebar_resizing = false;
                        }

                        // ── Teclado ──────────────────────────────────────
                        WindowEvent::KeyboardInput { event: ev, .. }
                            if ev.state == ElementState::Pressed =>
                        {
                            let key = &ev.logical_key;
                            let text = ev.text.as_deref();

                            let evento_opt = match modo {
                                ModoRenderer::Busqueda => {
                                    procesar_tecla_busqueda(
                                        key, text, mods, &mut modo, &mut consulta,
                                    )
                                }
                                ModoRenderer::Reemplazo => {
                                    procesar_tecla_reemplazo(
                                        key, text, mods,
                                        &mut modo, &mut consulta, &mut reemplazo, &mut campo,
                                    )
                                }
                                ModoRenderer::Normal => {
                                    let opt = resolver_evento(key, text, mods, tamano_tab, &seccion_con_foco);
                                    match &opt {
                                        Some(EventoEditor::IniciarBusqueda) => {
                                            modo = ModoRenderer::Busqueda;
                                            consulta.clear();
                                        }
                                        Some(EventoEditor::IniciarReemplazo) => {
                                            modo = ModoRenderer::Reemplazo;
                                            consulta.clear();
                                            reemplazo.clear();
                                            campo = CampoReemplazo::Buscar;
                                        }
                                        Some(EventoEditor::ToggleSidebar) => {
                                            let visible_actual = layout.secciones().iter()
                                                .find(|s| s.id == "sidebar")
                                                .map(|s| s.visible)
                                                .unwrap_or(false);
                                            layout.establecer_visible("sidebar", !visible_actual);
                                        }
                                        _ => {}
                                    }
                                    opt
                                }
                            };

                            if let Some(evento) = evento_opt {
                                // ToggleSidebar no se reenvía a la app
                                if matches!(evento, EventoEditor::ToggleSidebar) {
                                    window.request_redraw();
                                    return;
                                }
                                if let Some(nuevo) = manejador(evento) {
                                    contenido = nuevo;

                                    // Sincronizar secciones de plugin en el layout
                                    sincronizar_secciones_plugin(&mut layout, &contenido, sidebar_ancho);

                                    window.request_redraw();
                                }
                            }
                        }

                        WindowEvent::RedrawRequested => {
                            let ancho = gpu.config_superficie.width;
                            let alto = gpu.config_superficie.height;
                            renderizar_frame(
                                &window,
                                &mut gpu,
                                &mut texto,
                                &mut quads,
                                &mut layout,
                                &contenido,
                                ancho,
                                alto,
                                mostrar_borde_foco,
                            );
                        }

                        _ => {}
                    }
                }
                _ => {}
            }
        })?;

        Ok(())
    }
}

/// Devuelve el ancho actual de la sidebar (0.0 si no visible).
fn sidebar_ancho_actual(layout: &LayoutManager) -> f32 {
    layout.rect_seccion("sidebar").map(|r| r.ancho).unwrap_or(0.0)
}

/// Registra en el LayoutManager las secciones declaradas por plugins en ContenidoRender.
fn sincronizar_secciones_plugin(layout: &mut LayoutManager, contenido: &ContenidoRender, sidebar_ancho: f32) {
    // Identificar IDs de secciones de plugin actuales en layout (z_order >= 10)
    let ids_actuales: Vec<String> = layout.secciones().iter()
        .filter(|s| s.z_order >= 10)
        .map(|s| s.id.clone())
        .collect();

    let ids_nuevas: Vec<String> = contenido.secciones_plugin.iter()
        .map(|s| s.id.clone())
        .collect();

    // Quitar secciones que ya no existen
    for id in &ids_actuales {
        if !ids_nuevas.contains(id) {
            layout.quitar(id);
        }
    }

    // Añadir/actualizar secciones del contenido
    for (i, sec) in contenido.secciones_plugin.iter().enumerate() {
        let lado = match sec.lado.as_str() {
            "derecha" => LadoLayout::Derecha,
            "arriba"  => LadoLayout::Arriba,
            "abajo"   => LadoLayout::Abajo,
            _         => LadoLayout::Izquierda,
        };
        let color_fondo = sec.color_fondo.map(|[r, g, b]| ColorRgba::rgb_u8(r, g, b));
        let tamano_pref = if sec.id == "sidebar" && lado == LadoLayout::Izquierda {
            TamanoPref::Fijo(sidebar_ancho)
        } else {
            TamanoPref::Fijo(sec.tamano)
        };
        // Preservar el estado de foco si la sección ya existe
        let con_foco = layout.secciones().iter()
            .find(|s| s.id == sec.id)
            .map(|s| s.con_foco)
            .unwrap_or(false);
        layout.registrar(SeccionUI {
            id: sec.id.clone(),
            lado,
            tamano_pref,
            visible: true,
            z_order: 10 + i as i32,
            color_fondo,
            con_foco,
        });
    }
}

// ------------------------------------------------------------------
// Procesado de teclas en modo búsqueda
// ------------------------------------------------------------------

fn procesar_tecla_busqueda(
    key: &Key,
    text: Option<&str>,
    mods: ModifiersState,
    modo: &mut ModoRenderer,
    consulta: &mut String,
) -> Option<EventoEditor> {
    if mods.control_key() {
        return None;
    }
    match key {
        Key::Named(NamedKey::Escape) => {
            *modo = ModoRenderer::Normal;
            consulta.clear();
            Some(EventoEditor::TerminarBusqueda)
        }
        Key::Named(NamedKey::Enter) if mods.shift_key() => Some(EventoEditor::MatchAnterior),
        Key::Named(NamedKey::Enter) => Some(EventoEditor::SiguienteMatch),
        Key::Named(NamedKey::Backspace) => {
            consulta.pop();
            Some(EventoEditor::ActualizarBusqueda(consulta.clone()))
        }
        _ => text
            .filter(|t| !t.is_empty())
            .map(|t| {
                consulta.push_str(t);
                EventoEditor::ActualizarBusqueda(consulta.clone())
            }),
    }
}

// ------------------------------------------------------------------
// Procesado de teclas en modo reemplazo
// ------------------------------------------------------------------

fn procesar_tecla_reemplazo(
    key: &Key,
    text: Option<&str>,
    mods: ModifiersState,
    modo: &mut ModoRenderer,
    consulta: &mut String,
    reemplazo: &mut String,
    campo: &mut CampoReemplazo,
) -> Option<EventoEditor> {
    if mods.control_key() {
        if let Key::Character(c) = key {
            if c.as_str() == "h" || c.as_str() == "H" {
                return Some(EventoEditor::ReemplazarTodo);
            }
        }
        return None;
    }

    match key {
        Key::Named(NamedKey::Escape) => {
            *modo = ModoRenderer::Normal;
            consulta.clear();
            reemplazo.clear();
            Some(EventoEditor::TerminarBusqueda)
        }
        Key::Named(NamedKey::Tab) => {
            *campo = match campo {
                CampoReemplazo::Buscar => CampoReemplazo::Reemplazar,
                CampoReemplazo::Reemplazar => CampoReemplazo::Buscar,
            };
            None
        }
        Key::Named(NamedKey::Enter) => Some(EventoEditor::ReemplazarMatch),
        Key::Named(NamedKey::Backspace) => match campo {
            CampoReemplazo::Buscar => {
                consulta.pop();
                Some(EventoEditor::ActualizarBusqueda(consulta.clone()))
            }
            CampoReemplazo::Reemplazar => {
                reemplazo.pop();
                Some(EventoEditor::ActualizarReemplazo(reemplazo.clone()))
            }
        },
        _ => text.filter(|t| !t.is_empty()).map(|t| match campo {
            CampoReemplazo::Buscar => {
                consulta.push_str(t);
                EventoEditor::ActualizarBusqueda(consulta.clone())
            }
            CampoReemplazo::Reemplazar => {
                reemplazo.push_str(t);
                EventoEditor::ActualizarReemplazo(reemplazo.clone())
            }
        }),
    }
}

// ------------------------------------------------------------------
// Traducción de teclas → EventoEditor (modo Normal)
// ------------------------------------------------------------------

fn resolver_evento(key: &Key, text: Option<&str>, mods: ModifiersState, tamano_tab: usize, seccion_con_foco: &str) -> Option<EventoEditor> {
    // Las secciones no-editor siempre ignoran edición de texto
    let es_editor = seccion_con_foco == "editor_area";
    
    if mods.control_key() {
        return match key {
            Key::Character(c) => match c.as_str() {
                "s" | "S" => Some(EventoEditor::Guardar),
                "z" | "Z" => Some(EventoEditor::Deshacer),
                "y" | "Y" => Some(EventoEditor::Rehacer),
                "f" | "F" => Some(EventoEditor::IniciarBusqueda),
                "h" | "H" => Some(EventoEditor::IniciarReemplazo),
                "k" | "K" => Some(EventoEditor::PedirHover),
                "a" | "A" => Some(EventoEditor::SeleccionarTodo),
                "c" | "C" => Some(EventoEditor::Copiar),
                "v" | "V" => Some(EventoEditor::Pegar),
                "x" | "X" => Some(EventoEditor::Cortar),
                "t" | "T" => Some(EventoEditor::NuevoTab),
                "w" | "W" => Some(EventoEditor::CerrarTab),
                "b" | "B" => Some(EventoEditor::ToggleSidebar),
                _ => None,
            },
            Key::Named(NamedKey::Tab) => {
                return if mods.shift_key() {
                    Some(EventoEditor::AnteriorTab)
                } else {
                    Some(EventoEditor::SiguienteTab)
                };
            }
            Key::Named(NamedKey::Home) => {
                Some(EventoEditor::MoverCursor(DireccionCursor::InicioDoc))
            }
            Key::Named(NamedKey::End) => {
                Some(EventoEditor::MoverCursor(DireccionCursor::FinDoc))
            }
            _ => None,
        };
    }

    let shift = mods.shift_key();

    match key {
        Key::Named(NamedKey::Enter) => {
            if es_editor {
                return Some(EventoEditor::InsertarTexto("\n".to_string()));
            } else {
                // En secciones de plugin, u32::MAX indica "activar línea enfocada internamente"
                return Some(EventoEditor::EventoSeccion {
                    id_seccion: seccion_con_foco.to_string(),
                    linea: u32::MAX,
                });
            }
        }
        Key::Named(NamedKey::Tab) => {
            if es_editor {
                return Some(EventoEditor::InsertarTexto(" ".repeat(tamano_tab)));
            } else {
                return None; // Tab ignora en secciones no-editor
            }
        }
        Key::Named(NamedKey::Backspace) => {
            return if es_editor { Some(EventoEditor::BorrarAtras) } else { None };
        }
        Key::Named(NamedKey::Delete) => {
            return if es_editor { Some(EventoEditor::BorrarAdelante) } else { None };
        }
        Key::Named(NamedKey::ArrowLeft) => {
            if es_editor {
                return if shift {
                    Some(EventoEditor::ExtenderSeleccion(DireccionCursor::Izquierda))
                } else {
                    Some(EventoEditor::MoverCursor(DireccionCursor::Izquierda))
                };
            } else {
                return Some(EventoEditor::NavegacionSeccion {
                    id_seccion: seccion_con_foco.to_string(),
                    direccion: DireccionCursor::Izquierda,
                });
            }
        }
        Key::Named(NamedKey::ArrowRight) => {
            if es_editor {
                return if shift {
                    Some(EventoEditor::ExtenderSeleccion(DireccionCursor::Derecha))
                } else {
                    Some(EventoEditor::MoverCursor(DireccionCursor::Derecha))
                };
            } else {
                return Some(EventoEditor::NavegacionSeccion {
                    id_seccion: seccion_con_foco.to_string(),
                    direccion: DireccionCursor::Derecha,
                });
            }
        }
        Key::Named(NamedKey::ArrowUp) => {
            if es_editor {
                return if shift {
                    Some(EventoEditor::ExtenderSeleccion(DireccionCursor::Arriba))
                } else {
                    Some(EventoEditor::MoverCursor(DireccionCursor::Arriba))
                };
            } else {
                // En sidebar u otras secciones, arriba = subir en la lista
                return Some(EventoEditor::NavegacionSeccion {
                    id_seccion: seccion_con_foco.to_string(),
                    direccion: DireccionCursor::Arriba,
                });
            }
        }
        Key::Named(NamedKey::ArrowDown) => {
            if es_editor {
                return if shift {
                    Some(EventoEditor::ExtenderSeleccion(DireccionCursor::Abajo))
                } else {
                    Some(EventoEditor::MoverCursor(DireccionCursor::Abajo))
                };
            } else {
                // En sidebar u otras secciones, abajo = bajar en la lista
                return Some(EventoEditor::NavegacionSeccion {
                    id_seccion: seccion_con_foco.to_string(),
                    direccion: DireccionCursor::Abajo,
                });
            }
        }
        Key::Named(NamedKey::Home) => {
            if es_editor {
                return if shift {
                    Some(EventoEditor::ExtenderSeleccion(DireccionCursor::InicioLinea))
                } else {
                    Some(EventoEditor::MoverCursor(DireccionCursor::InicioLinea))
                };
            } else {
                return Some(EventoEditor::NavegacionSeccion {
                    id_seccion: seccion_con_foco.to_string(),
                    direccion: DireccionCursor::InicioLinea,
                });
            }
        }
        Key::Named(NamedKey::End) => {
            if es_editor {
                return if shift {
                    Some(EventoEditor::ExtenderSeleccion(DireccionCursor::FinLinea))
                } else {
                    Some(EventoEditor::MoverCursor(DireccionCursor::FinLinea))
                };
            } else {
                return Some(EventoEditor::NavegacionSeccion {
                    id_seccion: seccion_con_foco.to_string(),
                    direccion: DireccionCursor::FinLinea,
                });
            }
        }
        Key::Named(NamedKey::PageUp) => {
            if es_editor {
                return Some(EventoEditor::MoverCursor(DireccionCursor::PaginaArriba));
            } else {
                return Some(EventoEditor::NavegacionSeccion {
                    id_seccion: seccion_con_foco.to_string(),
                    direccion: DireccionCursor::PaginaArriba,
                });
            }
        }
        Key::Named(NamedKey::PageDown) => {
            if es_editor {
                return Some(EventoEditor::MoverCursor(DireccionCursor::PaginaAbajo));
            } else {
                return Some(EventoEditor::NavegacionSeccion {
                    id_seccion: seccion_con_foco.to_string(),
                    direccion: DireccionCursor::PaginaAbajo,
                });
            }
        }
        _ => {}
    }

    // En secciones no-editor, ignorar entrada de texto
    if !es_editor {
        return None;
    }

    text.filter(|t| !t.is_empty())
        .map(|t| EventoEditor::InsertarTexto(t.to_string()))
}

// ------------------------------------------------------------------
// Renderizado de un frame
// ------------------------------------------------------------------

#[allow(clippy::too_many_arguments)]
fn renderizar_frame(
    ventana: &winit::window::Window,
    gpu: &mut ContextoGpu,
    texto: &mut RendererTexto,
    quads: &mut QuadRenderer,
    layout: &mut LayoutManager,
    contenido: &ContenidoRender,
    ancho: u32,
    alto: u32,
    mostrar_borde_foco: bool,
) {
    let af = ancho as f32;
    let hf = alto as f32;

    // 1. Calcular layout
    let soluciones = layout.calcular(af, hf);

    // 2. Preparar quads (fondos de sección)
    // Clonar soluciones para liberar el borrow mutable de layout antes de leer secciones()
    let soluciones_vec = soluciones.to_vec();
    quads.limpiar();
    for sol in &soluciones_vec {
        let color = layout.secciones().iter()
            .find(|s| s.id == sol.id)
            .and_then(|s| s.color_fondo);
        if let Some(c) = color {
            quads.agregar_quad(sol.rect, c);
        }
    }
    
    // Añadir borde visual para la sección con foco (desactivado por defecto)
    if mostrar_borde_foco {
        const BORDE_GROSOR: f32 = 2.0;
        const COLOR_BORDE_FOCO: ColorRgba = ColorRgba { r: 0.8, g: 0.8, b: 1.0, a: 0.8 };
        if let Some(sec_foco) = layout.secciones().iter().find(|s| s.con_foco).cloned() {
            if let Some(rect) = layout.rect_seccion(&sec_foco.id) {
                // Borde superior
                quads.agregar_quad(RectPx {
                    x: rect.x,
                    y: rect.y,
                    ancho: rect.ancho,
                    alto: BORDE_GROSOR,
                }, COLOR_BORDE_FOCO);
                
                // Borde inferior
                quads.agregar_quad(RectPx {
                    x: rect.x,
                    y: rect.y + rect.alto - BORDE_GROSOR,
                    ancho: rect.ancho,
                    alto: BORDE_GROSOR,
                }, COLOR_BORDE_FOCO);
                
                // Borde izquierdo
                quads.agregar_quad(RectPx {
                    x: rect.x,
                    y: rect.y,
                    ancho: BORDE_GROSOR,
                    alto: rect.alto,
                }, COLOR_BORDE_FOCO);
                
                // Borde derecho
                quads.agregar_quad(RectPx {
                    x: rect.x + rect.ancho - BORDE_GROSOR,
                    y: rect.y,
                    ancho: BORDE_GROSOR,
                    alto: rect.alto,
                }, COLOR_BORDE_FOCO);
            }
        }
    }
    
    if let Some(rect) = layout.rect_seccion("sidebar") {
        if rect.ancho > 40.0 {
            // Divider line at the resizable edge
            let divider = RectPx {
                x: rect.x + rect.ancho - 2.0,
                y: rect.y + 12.0,
                ancho: 2.0,
                alto: rect.alto - 24.0,
            };
            quads.agregar_quad(divider, COLOR_SIDEBAR_DIVIDER);

            // Visual handle in the center
            let handle = RectPx {
                x: rect.x + rect.ancho - 9.0,
                y: rect.y + (rect.alto * 0.5) - 16.0,
                ancho: 6.0,
                alto: 32.0,
            };
            quads.agregar_quad(handle, COLOR_SIDEBAR_HANDLE);
        }
    }

    // Fondos de líneas individuales de la sidebar (indicador de foco, etc.)
    // El texto de la sidebar arranca en ALTURA_TABS_PX + 8px dentro del rect,
    // así que aplicamos el mismo offset de 8px a los quads de fondo.
    if let Some(rect) = layout.rect_seccion("sidebar") {
        let line_h = texto.alto_linea();
        let scroll = texto.sidebar_scroll();
        const OFFSET_TEXTO: f32 = 8.0;
        let sidebar_sec = contenido.secciones_plugin.iter().find(|s| s.lado == "izquierda");
        if let Some(sec) = sidebar_sec {
            for (i, linea) in sec.lineas.iter().enumerate() {
                if let Some(fondo) = linea.fondo {
                    let y_offset = (i as i32 - scroll) as f32 * line_h;
                    let y = rect.y + OFFSET_TEXTO + y_offset;
                    if y >= rect.y && y < rect.y + rect.alto {
                        quads.agregar_quad(RectPx {
                            x: rect.x,
                            y,
                            ancho: rect.ancho - 2.0,
                            alto: line_h,
                        }, ColorRgba::rgb_u8(fondo[0], fondo[1], fondo[2]));
                    }
                }
            }
        }
    }

    quads.preparar(&gpu.dispositivo, &gpu.cola, ancho, alto);

    // 3. Preparar texto
    let sidebar_ancho = layout.rect_seccion("sidebar").map(|r| r.ancho).unwrap_or(0.0);
    texto.actualizar_contenido(contenido, af, hf, sidebar_ancho);

    if let Err(e) = texto.preparar(&gpu.dispositivo, &gpu.cola, ancho, alto) {
        tracing::error!("Error preparando atlas de texto: {e}");
        return;
    }

    // 4. Render pass
    let frame = match gpu.superficie.get_current_texture() {
        Ok(f) => f,
        Err(wgpu::SurfaceError::Lost | wgpu::SurfaceError::Outdated) => {
            gpu.redimensionar(ancho, alto);
            ventana.request_redraw();
            return;
        }
        Err(e) => {
            tracing::error!("Error obteniendo textura: {e}");
            return;
        }
    };

    let vista = frame.texture.create_view(&wgpu::TextureViewDescriptor::default());
    let mut encoder = gpu.dispositivo.create_command_encoder(
        &wgpu::CommandEncoderDescriptor { label: Some("encoder_principal") },
    );

    {
        let mut pase = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("pase_principal"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &vista,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color {
                        // Catppuccin Mocha base: #1E1E2E
                        r: 0.118, g: 0.118, b: 0.180, a: 1.0,
                    }),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
        });

        // Quads primero (fondos), texto encima
        quads.renderizar_en_pase(&mut pase);

        if let Err(e) = texto.renderizar_en_pase(&mut pase) {
            tracing::error!("Error renderizando texto: {e}");
        }
    }

    gpu.cola.submit([encoder.finish()]);
    frame.present();
    ventana.request_redraw();
}
