// Copyright 2026 Franz (frantsiles)
// Licensed under the Apache License, Version 2.0

//! # Renderer
//!
//! Event loop principal del editor basado en winit 0.29.
//!
//! ## Ciclo de vida
//!
//! ```text
//! Renderer::ejecutar()
//!   ├─ Crea la ventana (WindowBuilder)
//!   ├─ Inicializa ContextoGpu (wgpu)
//!   ├─ Inicializa RendererTexto (glyphon)
//!   └─ EventLoop::run(closure)
//!       ├─ WindowEvent::CloseRequested  → elwt.exit()
//!       ├─ WindowEvent::Resized         → gpu.redimensionar() + request_redraw()
//!       └─ Event::RedrawRequested       → renderizar_frame()
//! ```

use std::sync::Arc;

use anyhow::Result;
use winit::{
    dpi::PhysicalSize,
    event::{Event, WindowEvent},
    event_loop::{ControlFlow, EventLoop},
    window::WindowBuilder,
};

use crate::{
    configuracion::ConfigRenderer,
    contenido::ContenidoRender,
    gpu::ContextoGpu,
    texto::RendererTexto,
};

/// Renderer principal — encapsula config + contenido inicial.
///
/// Llamar `ejecutar()` para lanzar la ventana y el event loop.
pub struct Renderer {
    config: ConfigRenderer,
    contenido: ContenidoRender,
}

impl Renderer {
    pub fn nuevo(config: ConfigRenderer, contenido: ContenidoRender) -> Self {
        Self { config, contenido }
    }

    /// Inicia la ventana y el event loop. Bloquea hasta que el usuario cierra la ventana.
    pub fn ejecutar(self) -> Result<()> {
        let Self { config, contenido } = self;

        // ── Crear event loop ─────────────────────────────────────────────
        let event_loop = EventLoop::new()?;

        // ── Crear ventana antes de run() (válido en Linux/macOS/Windows) ─
        let window = Arc::new(
            WindowBuilder::new()
                .with_title(&config.titulo)
                .with_inner_size(PhysicalSize::new(config.ancho, config.alto))
                .build(&event_loop)?,
        );

        // ── Inicializar contexto GPU ─────────────────────────────────────
        let mut gpu = pollster::block_on(ContextoGpu::nuevo(window.clone()))?;

        // ── Inicializar renderer de texto ────────────────────────────────
        let mut texto = RendererTexto::nuevo(
            &gpu.dispositivo,
            &gpu.cola,
            gpu.config_superficie.format,
            config.tamano_fuente,
            config.multiplicador_linea,
        );

        tracing::info!(
            "Glyph iniciado — ventana {}×{} | fuente {}pt",
            config.ancho,
            config.alto,
            config.tamano_fuente
        );

        // Solicitar el primer frame
        window.request_redraw();

        // ── Event loop ───────────────────────────────────────────────────
        // winit 0.29: clausura (Event, &EventLoopWindowTarget)
        // ControlFlow::Wait + request_redraw() = loop dirigido por eventos
        event_loop.run(move |event, elwt| {
            elwt.set_control_flow(ControlFlow::Wait);

            match event {
                Event::WindowEvent { event, window_id } if window_id == window.id() => {
                    match event {
                        WindowEvent::CloseRequested => {
                            tracing::info!("Ventana cerrada — saliendo del editor");
                            elwt.exit();
                        }

                        WindowEvent::Resized(nuevo_tamaño) => {
                            gpu.redimensionar(nuevo_tamaño.width, nuevo_tamaño.height);
                            window.request_redraw();
                        }

                        // En algunos OS el factor de escala cambia el tamaño efectivo
                        WindowEvent::ScaleFactorChanged { .. } => {
                            let tamaño = window.inner_size();
                            gpu.redimensionar(tamaño.width, tamaño.height);
                            window.request_redraw();
                        }

                        // En winit 0.29 RedrawRequested es un WindowEvent (no Event de nivel raíz)
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
// Función de renderizado de un frame
// ------------------------------------------------------------------

fn renderizar_frame(
    ventana: &winit::window::Window,
    gpu: &mut ContextoGpu,
    texto: &mut RendererTexto,
    contenido: &ContenidoRender,
) {
    let texto_completo = contenido.texto_completo();
    let ancho = gpu.config_superficie.width;
    let alto = gpu.config_superficie.height;

    // ── Actualizar buffer de texto ───────────────────────────────────
    texto.actualizar_contenido(&texto_completo, ancho as f32, alto as f32);

    // ── Preparar atlas fuera del render pass ─────────────────────────
    if let Err(e) = texto.preparar(&gpu.dispositivo, &gpu.cola, ancho, alto) {
        tracing::error!("Error preparando atlas de texto: {e}");
        return;
    }

    // ── Obtener frame de la superficie ───────────────────────────────
    let frame = match gpu.superficie.get_current_texture() {
        Ok(f) => f,
        Err(wgpu::SurfaceError::Lost | wgpu::SurfaceError::Outdated) => {
            // Superficie invalidada — reconfigurar y reintentar en el siguiente frame
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

    // ── Render pass: fondo + texto ───────────────────────────────────
    {
        let mut pase = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("pase_principal"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &vista,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color {
                        r: 0.118, // fondo oscuro neutro — ajustable vía tema en Milestone 3
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

    // ── Enviar comandos y presentar ──────────────────────────────────
    gpu.cola.submit([encoder.finish()]);
    frame.present();

    // Siguiente frame — en Milestone 2 esto será event-driven
    ventana.request_redraw();
}
