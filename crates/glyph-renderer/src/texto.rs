// Copyright 2026 Franz (frantsiles)
// Licensed under the Apache License, Version 2.0

//! # RendererTexto
//!
//! Renderiza texto usando `glyphon` (cosmic-text + wgpu).
//!
//! ## Modos de renderizado
//!
//! - **Plain** (`spans` vacío): texto monocolor con cursor amarillo.
//! - **Highlighted** (`spans` no vacío): cada span lleva su color semántico;
//!   el cursor se superpone como overlay cortando el span que lo contenga.

use anyhow::{anyhow, Result};
use glyphon::{
    Attrs, Buffer, Color, Family, FontSystem, Metrics, Resolution, Shaping, SwashCache, TextArea,
    TextAtlas, TextBounds, TextRenderer,
};

use crate::contenido::{ContenidoRender, CursorRender, SpanTexto};

const COLOR_TEXTO: Color = Color::rgb(0xCC, 0xCC, 0xCC);
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

    /// Actualiza el buffer de texto con el contenido actual.
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
        let cursor_byte = contenido.cursor.map(|c| cursor_byte_offset(&contenido.lineas, c));

        if contenido.spans.is_empty() {
            // Modo plain: monocolor con cursor overlay
            let attrs_normal = Attrs::new().family(Family::Monospace).color(COLOR_TEXTO);
            let attrs_cursor = Attrs::new().family(Family::Monospace).color(COLOR_CURSOR);

            match cursor_byte {
                Some(cb) if cb < texto.len() => {
                    let char_end = texto[cb..]
                        .chars()
                        .next()
                        .map(|c| cb + c.len_utf8())
                        .unwrap_or(cb + 1)
                        .min(texto.len());
                    buffer.set_rich_text(
                        sistema_fuentes,
                        [
                            (&texto[..cb], attrs_normal),
                            (&texto[cb..char_end], attrs_cursor),
                            (&texto[char_end..], attrs_normal),
                        ],
                        Shaping::Advanced,
                    );
                }
                Some(_) => {
                    // Cursor al final del documento
                    buffer.set_rich_text(
                        sistema_fuentes,
                        [
                            (texto.as_str(), attrs_normal),
                            (" ", attrs_cursor),
                            ("", attrs_normal),
                        ],
                        Shaping::Advanced,
                    );
                }
                None => {
                    buffer.set_text(sistema_fuentes, &texto, attrs_normal, Shaping::Advanced);
                }
            }
        } else {
            // Modo resaltado: spans coloreados + cursor overlay
            let fragmentos = construir_spans_glyphon(&texto, &contenido.spans, cursor_byte);
            let refs: Vec<(&str, Attrs)> =
                fragmentos.iter().map(|(s, a)| (s.as_str(), *a)).collect();
            buffer.set_rich_text(sistema_fuentes, refs, Shaping::Advanced);
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

/// Byte offset del cursor en el texto completo (texto_completo = lineas.join("\n")).
fn cursor_byte_offset(lineas: &[String], cursor: CursorRender) -> usize {
    let linea = (cursor.linea as usize).min(lineas.len().saturating_sub(1));
    let mut offset = 0usize;
    for i in 0..linea {
        offset += lineas[i].len() + 1; // +1 por el \n del join
    }
    // cursor.columna es índice de carácter — convertir a byte offset
    let linea_str = lineas.get(linea).map(|s| s.as_str()).unwrap_or("");
    let byte_col = linea_str
        .char_indices()
        .nth(cursor.columna as usize)
        .map(|(b, _)| b)
        .unwrap_or(linea_str.len());
    offset + byte_col
}

/// Construye fragmentos coloreados a partir de spans semánticos y cursor overlay.
///
/// Los gaps entre spans se rellenan con COLOR_TEXTO.
/// El carácter bajo el cursor se sobreescribe con COLOR_CURSOR.
fn construir_spans_glyphon(
    texto: &str,
    spans: &[SpanTexto],
    cursor_byte: Option<usize>,
) -> Vec<(String, Attrs<'static>)> {
    let total = texto.len();

    // Paso 1: segmentos planos (start, end, Color) rellenando gaps
    let mut segmentos: Vec<(usize, usize, Color)> = Vec::new();
    let mut pos = 0usize;

    for span in spans {
        let s = span.inicio_byte.max(pos);
        let e = span.fin_byte.min(total);
        if s > pos {
            segmentos.push((pos, s, COLOR_TEXTO));
        }
        if s < e {
            let c = span.color;
            segmentos.push((s, e, Color::rgb(c.r, c.g, c.b)));
        }
        pos = e.max(pos);
    }
    if pos < total {
        segmentos.push((pos, total, COLOR_TEXTO));
    }

    // Paso 2: rango del carácter bajo el cursor (en bytes)
    let cursor_range: Option<(usize, usize)> = cursor_byte.map(|cb| {
        if cb < total {
            let char_end = texto[cb..]
                .chars()
                .next()
                .map(|c| cb + c.len_utf8())
                .unwrap_or(cb + 1)
                .min(total);
            (cb, char_end)
        } else {
            (total, total + 1) // marcador sintético — tratado abajo
        }
    });

    // Paso 3: emitir fragmentos con overlay del cursor
    let mut resultado: Vec<(String, Attrs<'static>)> = Vec::new();

    for &(start, end, color) in &segmentos {
        let base = Attrs::new().family(Family::Monospace).color(color);

        if let Some((cs, ce)) = cursor_range {
            if ce <= start || cs >= end {
                // Sin solapamiento
                if start < end {
                    resultado.push((texto[start..end].to_string(), base));
                }
            } else {
                // Solapamiento — cortar en tres partes
                if start < cs {
                    resultado.push((texto[start..cs].to_string(), base));
                }
                let cursor_attrs = Attrs::new().family(Family::Monospace).color(COLOR_CURSOR);
                resultado.push((texto[cs.max(start)..ce.min(end)].to_string(), cursor_attrs));
                if ce < end {
                    resultado.push((texto[ce..end].to_string(), base));
                }
            }
        } else {
            if start < end {
                resultado.push((texto[start..end].to_string(), base));
            }
        }
    }

    // Cursor al final del documento: espacio sintético
    if let Some((cs, _)) = cursor_range {
        if cs >= total {
            let cursor_attrs = Attrs::new().family(Family::Monospace).color(COLOR_CURSOR);
            resultado.push((" ".to_string(), cursor_attrs));
        }
    }

    resultado
}
