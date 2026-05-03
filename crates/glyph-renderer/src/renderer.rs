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
    window::WindowBuilder,
};

use crate::{
    configuracion::ConfigRenderer,
    contenido::ContenidoRender,
    eventos::{DireccionCursor, EventoEditor},
    gpu::ContextoGpu,
    quads::{ColorRgba, QuadRenderer},
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
        });
        layout.registrar(SeccionUI {
            id: "statusbar".into(), lado: LadoLayout::Abajo,
            tamano_pref: TamanoPref::Fijo(ALTURA_STATUS_PX),
            visible: true, z_order: 1, color_fondo: Some(COLOR_STATUS_FONDO),
        });
        layout.registrar(SeccionUI {
            id: "sidebar".into(), lado: LadoLayout::Izquierda,
            tamano_pref: TamanoPref::Fijo(ANCHO_SIDEBAR_PX),
            visible: false, z_order: 10, color_fondo: Some(COLOR_SIDEBAR_FONDO),
        });
        layout.registrar(SeccionUI {
            id: "editor_area".into(), lado: LadoLayout::Centro,
            tamano_pref: TamanoPref::Flex(1.0),
            visible: true, z_order: 2, color_fondo: None,
        });

        let tamano_tab = config.tamano_tab;
        let linea_alto = config.tamano_fuente * config.multiplicador_linea;

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
                                texto.ajustar_scroll(lineas);
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

                            if let Some(id) = layout.seccion_en_posicion(mx, my).cloned() {
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
                                    let opt = resolver_evento(key, text, mods, tamano_tab);
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
                                    sincronizar_secciones_plugin(&mut layout, &contenido);

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
fn sincronizar_secciones_plugin(layout: &mut LayoutManager, contenido: &ContenidoRender) {
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
        layout.registrar(SeccionUI {
            id: sec.id.clone(),
            lado,
            tamano_pref: TamanoPref::Fijo(sec.tamano),
            visible: true,
            z_order: 10 + i as i32,
            color_fondo,
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

fn resolver_evento(key: &Key, text: Option<&str>, mods: ModifiersState, tamano_tab: usize) -> Option<EventoEditor> {
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
            return Some(EventoEditor::InsertarTexto("\n".to_string()));
        }
        Key::Named(NamedKey::Tab) => {
            return Some(EventoEditor::InsertarTexto(" ".repeat(tamano_tab)));
        }
        Key::Named(NamedKey::Backspace) => return Some(EventoEditor::BorrarAtras),
        Key::Named(NamedKey::Delete) => return Some(EventoEditor::BorrarAdelante),
        Key::Named(NamedKey::ArrowLeft) => {
            return if shift {
                Some(EventoEditor::ExtenderSeleccion(DireccionCursor::Izquierda))
            } else {
                Some(EventoEditor::MoverCursor(DireccionCursor::Izquierda))
            };
        }
        Key::Named(NamedKey::ArrowRight) => {
            return if shift {
                Some(EventoEditor::ExtenderSeleccion(DireccionCursor::Derecha))
            } else {
                Some(EventoEditor::MoverCursor(DireccionCursor::Derecha))
            };
        }
        Key::Named(NamedKey::ArrowUp) => {
            return if shift {
                Some(EventoEditor::ExtenderSeleccion(DireccionCursor::Arriba))
            } else {
                Some(EventoEditor::MoverCursor(DireccionCursor::Arriba))
            };
        }
        Key::Named(NamedKey::ArrowDown) => {
            return if shift {
                Some(EventoEditor::ExtenderSeleccion(DireccionCursor::Abajo))
            } else {
                Some(EventoEditor::MoverCursor(DireccionCursor::Abajo))
            };
        }
        Key::Named(NamedKey::Home) => {
            return if shift {
                Some(EventoEditor::ExtenderSeleccion(DireccionCursor::InicioLinea))
            } else {
                Some(EventoEditor::MoverCursor(DireccionCursor::InicioLinea))
            };
        }
        Key::Named(NamedKey::End) => {
            return if shift {
                Some(EventoEditor::ExtenderSeleccion(DireccionCursor::FinLinea))
            } else {
                Some(EventoEditor::MoverCursor(DireccionCursor::FinLinea))
            };
        }
        Key::Named(NamedKey::PageUp) => {
            return Some(EventoEditor::MoverCursor(DireccionCursor::PaginaArriba));
        }
        Key::Named(NamedKey::PageDown) => {
            return Some(EventoEditor::MoverCursor(DireccionCursor::PaginaAbajo));
        }
        _ => {}
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
