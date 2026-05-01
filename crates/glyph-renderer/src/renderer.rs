// Copyright 2026 Franz (frantsiles)
// Licensed under the Apache License, Version 2.0

//! # Renderer
//!
//! Event loop principal del editor (winit 0.29).
//!
//! ## Patrón manejador
//!
//! El renderer detecta eventos de teclado y los traduce a `EventoEditor`.
//! Los delega a un closure `manejador: FnMut(EventoEditor) -> Option<ContenidoRender>`
//! provisto por `glyph-app`. Si el manejador devuelve `Some(nuevo_contenido)`,
//! el renderer actualiza la pantalla. El renderer no conoce `glyph-core`.
//!
//! ## Teclas soportadas
//!
//! | Tecla           | Evento             |
//! |---|---|
//! | Caracteres      | InsertarTexto      |
//! | Enter           | InsertarTexto("\n")|
//! | Tab             | InsertarTexto("    ") — 4 espacios |
//! | Backspace       | BorrarAtras        |
//! | Delete          | BorrarAdelante     |
//! | Flechas         | MoverCursor        |
//! | Home / End      | InicioLinea / FinLinea |
//! | Ctrl+S          | Guardar            |
//! | Ctrl+Z          | Deshacer           |
//! | Ctrl+Y          | Rehacer            |

use std::sync::Arc;

use anyhow::Result;
use winit::{
    dpi::PhysicalSize,
    event::{ElementState, Event, WindowEvent},
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

/// Renderer principal — encapsula config + contenido inicial.
pub struct Renderer {
    config: ConfigRenderer,
    contenido: ContenidoRender,
}

impl Renderer {
    pub fn nuevo(config: ConfigRenderer, contenido: ContenidoRender) -> Self {
        Self { config, contenido }
    }

    /// Inicia el event loop. Bloquea hasta que el usuario cierra la ventana.
    ///
    /// `manejador` recibe cada `EventoEditor` y devuelve `Some(ContenidoRender)`
    /// si el contenido cambió, o `None` si no hay nada que redibujar.
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
            config.ancho,
            config.alto,
            config.tamano_fuente
        );

        // Estado de modificadores (Ctrl, Shift, etc.)
        let mut mods = ModifiersState::default();
        // Contenido mutable dentro del closure
        let mut contenido = contenido;

        window.request_redraw();

        event_loop.run(move |event, elwt| {
            elwt.set_control_flow(ControlFlow::Wait);

            match event {
                Event::WindowEvent { event, window_id } if window_id == window.id() => {
                    match event {
                        // ── Cierre ──────────────────────────────────────
                        WindowEvent::CloseRequested => {
                            tracing::info!("Ventana cerrada — saliendo");
                            elwt.exit();
                        }

                        // ── Redimensionar ────────────────────────────────
                        WindowEvent::Resized(nuevo_tamaño) => {
                            gpu.redimensionar(nuevo_tamaño.width, nuevo_tamaño.height);
                            window.request_redraw();
                        }

                        WindowEvent::ScaleFactorChanged { .. } => {
                            let tamaño = window.inner_size();
                            gpu.redimensionar(tamaño.width, tamaño.height);
                            window.request_redraw();
                        }

                        // ── Modificadores (Ctrl, Shift…) ─────────────────
                        WindowEvent::ModifiersChanged(nuevos_mods) => {
                            mods = nuevos_mods.state();
                        }

                        // ── Teclado ──────────────────────────────────────
                        WindowEvent::KeyboardInput { event: ev, .. }
                            if ev.state == ElementState::Pressed =>
                        {
                            let evento_opt = resolver_evento(&ev.logical_key, ev.text.as_deref(), mods);

                            if let Some(evento) = evento_opt {
                                if let Some(nuevo) = manejador(evento) {
                                    contenido = nuevo;
                                    window.request_redraw();
                                }
                            }
                        }

                        // ── Dibujar ──────────────────────────────────────
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
// Traducción de teclas → EventoEditor
// ------------------------------------------------------------------

/// Convierte un `Key` de winit en un `EventoEditor`, teniendo en cuenta modificadores.
/// Devuelve `None` para teclas que el editor no procesa (F-keys, etc.).
fn resolver_evento(
    key: &Key,
    text: Option<&str>,
    mods: ModifiersState,
) -> Option<EventoEditor> {
    // ── Atajos con Ctrl ─────────────────────────────────────────────
    if mods.control_key() {
        return match key {
            Key::Character(c) => match c.as_str() {
                "s" | "S" => Some(EventoEditor::Guardar),
                "z" | "Z" => Some(EventoEditor::Deshacer),
                "y" | "Y" => Some(EventoEditor::Rehacer),
                _ => None,
            },
            _ => None,
        };
    }

    // ── Teclas nombradas ─────────────────────────────────────────────
    match key {
        Key::Named(NamedKey::Enter) => {
            return Some(EventoEditor::InsertarTexto("\n".to_string()));
        }
        Key::Named(NamedKey::Tab) => {
            // 4 espacios — configurable vía Lua en Milestone 3
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
        _ => {}
    }

    // ── Texto imprimible ─────────────────────────────────────────────
    // `text` ya maneja dead keys, compose y el layout del teclado del usuario
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

    let mut encoder =
        gpu.dispositivo
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("encoder_principal"),
            });

    {
        let mut pase = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("pase_principal"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &vista,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color {
                        r: 0.118,
                        g: 0.118,
                        b: 0.141,
                        a: 1.0,
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
