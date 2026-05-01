// Copyright 2026 Franz (frantsiles)
// Licensed under the Apache License, Version 2.0

//! # RendererTexto
//!
//! Renderiza texto usando `glyphon` (cosmic-text + wgpu).
//!
//! ## Cursor
//!
//! El carácter en la posición del cursor se resalta con un color de acento
//! usando `Buffer::set_rich_text`. Esto evita necesitar un pipeline de
//! rectángulos separado para el cursor en Milestone 2.

use anyhow::{anyhow, Result};
use glyphon::{
    Attrs, Buffer, Color, Family, FontSystem, Metrics, Resolution, Shaping, SwashCache, TextArea,
    TextAtlas, TextBounds, TextRenderer,
};

use crate::contenido::{ContenidoRender, CursorRender};

// Color por defecto del texto
const COLOR_TEXTO: Color = Color::rgb(0xCC, 0xCC, 0xCC);
// Color de acento para resaltar el carácter bajo el cursor
const COLOR_CURSOR: Color = Color::rgb(0xFF, 0xCC, 0x00);

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

    /// Actualiza el buffer de texto con el contenido actual y resalta el cursor.
    ///
    /// Debe llamarse antes de `preparar`, cuando el contenido o el tamaño cambian.
    pub fn actualizar_contenido(&mut self, contenido: &ContenidoRender, ancho: f32, alto: f32) {
        let Self {
            sistema_fuentes,
            buffer,
            metricas,
            ..
        } = self;

        buffer.set_metrics(sistema_fuentes, *metricas);
        buffer.set_size(sistema_fuentes, ancho, alto);

        let texto = contenido.texto_completo();

        if let Some(cursor) = contenido.cursor {
            let offset = offset_cursor(&contenido.lineas, cursor);
            let total = texto.chars().count();

            let attrs_normal = Attrs::new().family(Family::Monospace).color(COLOR_TEXTO);
            let attrs_cursor = Attrs::new().family(Family::Monospace).color(COLOR_CURSOR);

            // Dividir el texto en tres partes: antes | bajo_cursor | después
            let (antes, bajo_cursor, despues) = dividir_en_cursor(&texto, offset, total);

            buffer.set_rich_text(
                sistema_fuentes,
                [
                    (antes.as_str(), attrs_normal),
                    (bajo_cursor.as_str(), attrs_cursor),
                    (despues.as_str(), attrs_normal),
                ],
                Shaping::Advanced,
            );
        } else {
            let attrs = Attrs::new().family(Family::Monospace).color(COLOR_TEXTO);
            buffer.set_text(sistema_fuentes, &texto, attrs, Shaping::Advanced);
        }

        buffer.shape_until_scroll(sistema_fuentes);
    }

    /// Prepara el atlas de glifos para el frame actual.
    /// Debe llamarse **antes** de iniciar cualquier render pass.
    pub fn preparar(
        &mut self,
        dispositivo: &wgpu::Device,
        cola: &wgpu::Queue,
        ancho: u32,
        alto: u32,
    ) -> Result<()> {
        self.atlas.trim();

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
                    default_color: COLOR_TEXTO,
                }],
                cache_formas,
            )
            .map_err(|e| anyhow!("glyphon prepare falló: {e:?}"))
    }

    /// Emite los draw calls de texto dentro de un render pass activo.
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

// ------------------------------------------------------------------
// Helpers privados
// ------------------------------------------------------------------

/// Calcula el offset lineal del cursor en el texto completo (post-join con \n).
fn offset_cursor(lineas: &[String], cursor: CursorRender) -> usize {
    let linea = (cursor.linea as usize).min(lineas.len().saturating_sub(1));
    let mut offset = 0;
    for i in 0..linea {
        offset += lineas[i].chars().count() + 1; // +1 por el \n que se inserta al join
    }
    let max_col = lineas.get(linea).map(|l| l.chars().count()).unwrap_or(0);
    offset += (cursor.columna as usize).min(max_col);
    offset
}

/// Divide el texto en tres fragmentos: antes del cursor, bajo el cursor, después.
/// Si el cursor está al final, el carácter bajo el cursor es un espacio sintético.
fn dividir_en_cursor(texto: &str, offset: usize, total: usize) -> (String, String, String) {
    if offset < total {
        let antes: String = texto.chars().take(offset).collect();
        let bajo_cursor: String = texto.chars().nth(offset).map(|c| {
            // Los saltos de línea no son visibles — mostrar espacio como indicador
            if c == '\n' { '\n' } else { c }
        }).into_iter().collect();
        let despues: String = texto.chars().skip(offset + 1).collect();
        (antes, bajo_cursor, despues)
    } else {
        // Cursor al final del documento
        (texto.to_string(), " ".to_string(), String::new())
    }
}
