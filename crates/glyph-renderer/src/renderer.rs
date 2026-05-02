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
//! | Click izq.     | Normal       | MoverCursorA                    |
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
    event::{ElementState, Event, MouseButton, WindowEvent},
    event_loop::{ControlFlow, EventLoop},
    keyboard::{Key, ModifiersState, NamedKey},
    window::WindowBuilder,
};

use crate::{
    configuracion::ConfigRenderer,
    contenido::ContenidoRender,
    eventos::{DireccionCursor, EventoEditor},
    gpu::ContextoGpu,
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
        );

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

        // Constantes de geometría para conversión click → (línea, col)
        let char_ancho = config.tamano_fuente * 0.601;
        let linea_alto = config.tamano_fuente * config.multiplicador_linea;

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

                        // ── Click izquierdo → mover cursor ────────────────
                        WindowEvent::MouseInput {
                            state: ElementState::Pressed,
                            button: MouseButton::Left,
                            ..
                        } if modo == ModoRenderer::Normal => {
                            let gutter = texto.ancho_gutter();
                            let scroll = texto.scroll_linea();
                            let tx = pos_raton.0 - gutter - 4.0;
                            let ty = pos_raton.1 - 8.0;
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
                                    let opt = resolver_evento(key, text, mods);
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
                                        _ => {}
                                    }
                                    opt
                                }
                            };

                            if let Some(evento) = evento_opt {
                                if let Some(nuevo) = manejador(evento) {
                                    contenido = nuevo;
                                    window.request_redraw();
                                }
                            }
                        }

                        WindowEvent::RedrawRequested => {
                            renderizar_frame(&window, &mut gpu, &mut texto, &contenido);
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
        // Ctrl+H — reemplazar todo
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

fn resolver_evento(key: &Key, text: Option<&str>, mods: ModifiersState) -> Option<EventoEditor> {
    if mods.control_key() {
        return match key {
            Key::Character(c) => match c.as_str() {
                "s" | "S" => Some(EventoEditor::Guardar),
                "z" | "Z" => Some(EventoEditor::Deshacer),
                "y" | "Y" => Some(EventoEditor::Rehacer),
                "f" | "F" => Some(EventoEditor::IniciarBusqueda),
                "h" | "H" => Some(EventoEditor::IniciarReemplazo),
                _ => None,
            },
            Key::Named(NamedKey::Home) => {
                Some(EventoEditor::MoverCursor(DireccionCursor::InicioDoc))
            }
            Key::Named(NamedKey::End) => {
                Some(EventoEditor::MoverCursor(DireccionCursor::FinDoc))
            }
            _ => None,
        };
    }

    match key {
        Key::Named(NamedKey::Enter) => {
            return Some(EventoEditor::InsertarTexto("\n".to_string()));
        }
        Key::Named(NamedKey::Tab) => {
            return Some(EventoEditor::InsertarTexto("    ".to_string()));
        }
        Key::Named(NamedKey::Backspace) => return Some(EventoEditor::BorrarAtras),
        Key::Named(NamedKey::Delete) => return Some(EventoEditor::BorrarAdelante),
        Key::Named(NamedKey::ArrowLeft) => {
            return Some(EventoEditor::MoverCursor(DireccionCursor::Izquierda));
        }
        Key::Named(NamedKey::ArrowRight) => {
            return Some(EventoEditor::MoverCursor(DireccionCursor::Derecha));
        }
        Key::Named(NamedKey::ArrowUp) => {
            return Some(EventoEditor::MoverCursor(DireccionCursor::Arriba));
        }
        Key::Named(NamedKey::ArrowDown) => {
            return Some(EventoEditor::MoverCursor(DireccionCursor::Abajo));
        }
        Key::Named(NamedKey::Home) => {
            return Some(EventoEditor::MoverCursor(DireccionCursor::InicioLinea));
        }
        Key::Named(NamedKey::End) => {
            return Some(EventoEditor::MoverCursor(DireccionCursor::FinLinea));
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

fn renderizar_frame(
    ventana: &winit::window::Window,
    gpu: &mut ContextoGpu,
    texto: &mut RendererTexto,
    contenido: &ContenidoRender,
) {
    let ancho = gpu.config_superficie.width;
    let alto = gpu.config_superficie.height;

    texto.actualizar_contenido(contenido, ancho as f32, alto as f32);

    if let Err(e) = texto.preparar(&gpu.dispositivo, &gpu.cola, ancho, alto) {
        tracing::error!("Error preparando atlas de texto: {e}");
        return;
    }

    let frame = match gpu.superficie.get_current_texture() {
        Ok(f) => f,
        Err(wgpu::SurfaceError::Lost | wgpu::SurfaceError::Outdated) => {
            gpu.redimensionar(ancho, alto);
            ventana.request_redraw();
            return;
        }
        Err(e) => {
            tracing::error!("Error obteniendo textura de superficie: {e}");
            return;
        }
    };

    let vista = frame
        .texture
        .create_view(&wgpu::TextureViewDescriptor::default());
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
                        r: 0.118, g: 0.118, b: 0.141, a: 1.0,
                    }),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
        });

        if let Err(e) = texto.renderizar_en_pase(&mut pase) {
            tracing::error!("Error renderizando texto en pase GPU: {e}");
        }
    }

    gpu.cola.submit([encoder.finish()]);
    frame.present();
    ventana.request_redraw();
}
