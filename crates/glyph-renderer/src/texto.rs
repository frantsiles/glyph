// Copyright 2026 Franz (frantsiles)
// Licensed under the Apache License, Version 2.0

//! # RendererTexto
//!
//! Renderiza texto usando `glyphon` (integración de `cosmic-text` con wgpu).
//!
//! ## Responsabilidades
//! - Mantener el `FontSystem` (carga de fuentes del sistema) y la caché de formas.
//! - Actualizar el `Buffer` de texto cuando el contenido cambia.
//! - Llamar a `prepare` antes del render pass y `render` dentro de él.
//!
//! ## Separación prepare / render
//! `prepare` actualiza el atlas de glifos en la GPU (operación de escritura).
//! `render` emite draw calls dentro del render pass (solo lectura del atlas).
//! Ambas fases deben mantenerse separadas por la API de wgpu.

use anyhow::{anyhow, Result};
use glyphon::{
    Attrs, Buffer, Color, Family, FontSystem, Metrics, Resolution, Shaping, SwashCache, TextArea,
    TextAtlas, TextBounds, TextRenderer,
};

/// Encapsula el pipeline de renderizado de texto
pub struct RendererTexto {
    sistema_fuentes: FontSystem,
    cache_formas: SwashCache,
    atlas: TextAtlas,
    renderer: TextRenderer,
    buffer: Buffer,
    metricas: Metrics,
}

impl RendererTexto {
    /// Crea el renderer de texto.
    ///
    /// # Parámetros
    /// - `formato`: debe coincidir con el formato de la superficie wgpu.
    /// - `tamano_fuente`: puntos de fuente.
    /// - `multiplicador_linea`: factor de altura de línea (ej: 1.4).
    pub fn nuevo(
        dispositivo: &wgpu::Device,
        cola: &wgpu::Queue,
        formato: wgpu::TextureFormat,
        tamano_fuente: f32,
        multiplicador_linea: f32,
    ) -> Self {
        let mut sistema_fuentes = FontSystem::new();
        let cache_formas = SwashCache::new();
        let mut atlas = TextAtlas::new(dispositivo, cola, formato);
        let renderer = TextRenderer::new(
            &mut atlas,
            dispositivo,
            wgpu::MultisampleState::default(),
            None,
        );

        let metricas = Metrics::new(tamano_fuente, tamano_fuente * multiplicador_linea);
        let mut buffer = Buffer::new(&mut sistema_fuentes, metricas);
        // Tamaño inicial genérico — se actualiza en cada frame
        buffer.set_size(&mut sistema_fuentes, 1280.0, 720.0);

        Self {
            sistema_fuentes,
            cache_formas,
            atlas,
            renderer,
            buffer,
            metricas,
        }
    }

    /// Actualiza el texto y el tamaño del viewport del buffer.
    ///
    /// Llamar cuando el contenido o el tamaño de la ventana cambien.
    pub fn actualizar_contenido(&mut self, texto: &str, ancho: f32, alto: f32) {
        // Split borrow — Rust permite acceso simultáneo a campos distintos
        let Self {
            sistema_fuentes,
            buffer,
            metricas,
            ..
        } = self;

        buffer.set_metrics(sistema_fuentes, *metricas);
        buffer.set_size(sistema_fuentes, ancho, alto);
        buffer.set_text(
            sistema_fuentes,
            texto,
            Attrs::new().family(Family::Monospace),
            Shaping::Advanced,
        );
        buffer.shape_until_scroll(sistema_fuentes);
    }

    /// Prepara el atlas de glifos para el frame actual.
    ///
    /// Debe llamarse **antes** de iniciar cualquier render pass.
    pub fn preparar(
        &mut self,
        dispositivo: &wgpu::Device,
        cola: &wgpu::Queue,
        ancho: u32,
        alto: u32,
    ) -> Result<()> {
        // Liberar glifos no usados en el frame anterior
        self.atlas.trim();

        // Split borrow para que el borrow checker permita acceder a todos los campos
        let Self {
            renderer,
            sistema_fuentes,
            atlas,
            buffer,
            cache_formas,
            ..
        } = self;

        renderer
            .prepare(
                dispositivo,
                cola,
                sistema_fuentes,
                atlas,
                Resolution {
                    width: ancho,
                    height: alto,
                },
                [TextArea {
                    buffer,
                    left: 8.0,
                    top: 8.0,
                    scale: 1.0,
                    bounds: TextBounds {
                        left: 0,
                        top: 0,
                        right: ancho as i32,
                        bottom: alto as i32,
                    },
                    default_color: Color::rgb(0xCC, 0xCC, 0xCC),
                }],
                cache_formas,
            )
            .map_err(|e| anyhow!("glyphon prepare falló: {e:?}"))
    }

    /// Emite los draw calls de texto dentro de un render pass activo.
    ///
    /// Debe llamarse **dentro** de un render pass, después de `preparar`.
    pub fn renderizar_en_pase<'pass>(
        &'pass self,
        pase: &mut wgpu::RenderPass<'pass>,
    ) -> Result<()> {
        self.renderer
            .render(&self.atlas, pase)
            .map_err(|e| anyhow!("glyphon render falló: {e:?}"))
    }
}
