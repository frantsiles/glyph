// Copyright 2026 Franz (frantsiles)
// Licensed under the Apache License, Version 2.0

//! # RendererTexto
//!
//! Renderiza texto usando `glyphon` (cosmic-text + wgpu).
//!
//! ## Jerarquía de color por fragmento
//!
//! ```text
//! 1. COLOR_TEXTO      (predeterminado)
//! 2. Span sintáctico  (tree-sitter)
//! 3. Match inactivo   (búsqueda)
//! 4. Match activo     (búsqueda — match seleccionado)
//! 5. Diagnóstico LSP  (sobreescribe sintaxis y matches en su rango)
//! 6. Cursor           (sobreescribe todo en su carácter)
//! ```
//!
//! ## Algoritmo de barrido de fronteras
//!
//! Todos los extremos de spans, diagnósticos, matches y cursor se convierten en
//! "fronteras". El texto queda partido en segmentos entre fronteras
//! consecutivas. Cada segmento recibe el color de mayor prioridad que
//! lo cubre. Esto garantiza corrección con cualquier combinación de
//! rangos solapados.

use anyhow::{anyhow, Result};
use glyphon::{
    Attrs, Buffer, Color, Family, FontSystem, Metrics, Resolution, Shaping, SwashCache, TextArea,
    TextAtlas, TextBounds, TextRenderer,
};

use crate::contenido::{ContenidoRender, CursorRender, DiagnosticoRender, SeveridadRender, SpanTexto};

const COLOR_TEXTO: Color = Color::rgb(0xCC, 0xCC, 0xCC);
const COLOR_CURSOR: Color = Color::rgb(0xFF, 0xCC, 0x00);
const COLOR_MATCH: Color = Color::rgb(0xFF, 0xCC, 0x00);
const COLOR_MATCH_ACTIVO: Color = Color::rgb(0xFF, 0xFF, 0xFF);
const COLOR_TEXTO_BARRA: Color = Color::rgb(0x98, 0xA0, 0xAD);

const ALTURA_BARRA: f32 = 22.0;

fn color_diagnostico(severidad: SeveridadRender) -> Color {
    match severidad {
        SeveridadRender::Error       => Color::rgb(0xFF, 0x6B, 0x6B),
        SeveridadRender::Aviso       => Color::rgb(0xFF, 0xBF, 0x69),
        SeveridadRender::Informacion => Color::rgb(0x61, 0xAF, 0xEF),
        SeveridadRender::Sugerencia  => Color::rgb(0x98, 0x98, 0x98),
    }
}

/// Encapsula el pipeline de renderizado de texto (editor + barra de estado).
pub struct RendererTexto {
    sistema_fuentes: FontSystem,
    cache_formas: SwashCache,
    atlas: TextAtlas,
    renderer: TextRenderer,
    buffer: Buffer,
    buffer_barra: Buffer,
    metricas: Metrics,
    metricas_barra: Metrics,
}

impl RendererTexto {
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

        let metricas_barra = Metrics::new(13.0, ALTURA_BARRA);
        let mut buffer_barra = Buffer::new(&mut sistema_fuentes, metricas_barra);
        buffer_barra.set_size(&mut sistema_fuentes, 1280.0, ALTURA_BARRA);

        Self { sistema_fuentes, cache_formas, atlas, renderer, buffer, buffer_barra, metricas, metricas_barra }
    }

    /// Actualiza el buffer de texto con el contenido del frame.
    pub fn actualizar_contenido(&mut self, contenido: &ContenidoRender, ancho: f32, alto: f32) {
        let Self { sistema_fuentes, buffer, buffer_barra, metricas, metricas_barra, .. } = self;

        // — Editor principal —
        buffer.set_metrics(sistema_fuentes, *metricas);
        buffer.set_size(sistema_fuentes, ancho, (alto - ALTURA_BARRA).max(1.0));

        let texto = contenido.texto_completo();
        let cursor_byte = contenido.cursor.map(|c| cursor_byte_offset(&contenido.lineas, c));

        let fragmentos = construir_spans_glyphon(
            &texto,
            &contenido.spans,
            &contenido.matches_busqueda,
            contenido.match_activo,
            &contenido.diagnosticos,
            cursor_byte,
        );

        let refs: Vec<(&str, Attrs)> =
            fragmentos.iter().map(|(s, a)| (s.as_str(), *a)).collect();

        buffer.set_rich_text(sistema_fuentes, refs, Shaping::Advanced);
        buffer.shape_until_scroll(sistema_fuentes);

        // — Barra de estado —
        buffer_barra.set_metrics(sistema_fuentes, *metricas_barra);
        buffer_barra.set_size(sistema_fuentes, ancho, ALTURA_BARRA);

        let barra_attrs = Attrs::new().family(Family::Monospace).color(COLOR_TEXTO_BARRA);
        buffer_barra.set_rich_text(
            sistema_fuentes,
            std::iter::once((contenido.barra_estado.as_str(), barra_attrs)),
            Shaping::Advanced,
        );
        buffer_barra.shape_until_scroll(sistema_fuentes);
    }

    /// Prepara el atlas de glifos para el frame actual (antes del render pass).
    pub fn preparar(
        &mut self,
        dispositivo: &wgpu::Device,
        cola: &wgpu::Queue,
        ancho: u32,
        alto: u32,
    ) -> Result<()> {
        self.atlas.trim();

        let alto_editor = (alto as i32) - ALTURA_BARRA as i32;
        let Self { renderer, sistema_fuentes, atlas, buffer, buffer_barra, cache_formas, .. } = self;

        renderer
            .prepare(
                dispositivo,
                cola,
                sistema_fuentes,
                atlas,
                Resolution { width: ancho, height: alto },
                [
                    TextArea {
                        buffer,
                        left: 8.0,
                        top: 8.0,
                        scale: 1.0,
                        bounds: TextBounds {
                            left: 0,
                            top: 0,
                            right: ancho as i32,
                            bottom: alto_editor,
                        },
                        default_color: COLOR_TEXTO,
                    },
                    TextArea {
                        buffer: buffer_barra,
                        left: 8.0,
                        top: alto as f32 - ALTURA_BARRA,
                        scale: 1.0,
                        bounds: TextBounds {
                            left: 0,
                            top: alto_editor,
                            right: ancho as i32,
                            bottom: alto as i32,
                        },
                        default_color: COLOR_TEXTO_BARRA,
                    },
                ],
                cache_formas,
            )
            .map_err(|e| anyhow!("glyphon prepare falló: {e:?}"))
    }

    /// Emite draw calls de texto dentro de un render pass activo.
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
// Algoritmo de barrido de fronteras
// ------------------------------------------------------------------

/// Construye fragmentos `(texto, Attrs)` respetando la jerarquía de color:
/// predeterminado < sintaxis < match inactivo < match activo < diagnóstico < cursor.
fn construir_spans_glyphon(
    texto: &str,
    spans: &[SpanTexto],
    matches: &[(usize, usize)],
    match_activo: Option<usize>,
    diagnosticos: &[DiagnosticoRender],
    cursor_byte: Option<usize>,
) -> Vec<(String, Attrs<'static>)> {
    let total = texto.len();
    if total == 0 {
        if cursor_byte.is_some() {
            return vec![(
                " ".to_string(),
                Attrs::new().family(Family::Monospace).color(COLOR_CURSOR),
            )];
        }
        return vec![];
    }

    // ── Paso 1: recopilar todas las fronteras ────────────────────────────
    let mut fronteras: Vec<usize> = vec![0, total];

    for s in spans {
        fronteras.push(s.inicio_byte.min(total));
        fronteras.push(s.fin_byte.min(total));
    }
    for d in diagnosticos {
        fronteras.push(d.inicio_byte.min(total));
        fronteras.push(d.fin_byte.min(total));
    }
    for &(ini, fin) in matches {
        fronteras.push(ini.min(total));
        fronteras.push(fin.min(total));
    }
    if let Some(cb) = cursor_byte {
        let cb = cb.min(total);
        fronteras.push(cb);
        if cb < total {
            let fin_char = texto[cb..]
                .chars()
                .next()
                .map(|c| cb + c.len_utf8())
                .unwrap_or(cb + 1)
                .min(total);
            fronteras.push(fin_char);
        }
    }

    fronteras.sort_unstable();
    fronteras.dedup();

    // ── Paso 2: precomputar rango del cursor ──────────────────────────────
    let cursor_range: Option<(usize, usize)> = cursor_byte.map(|cb| {
        let cb = cb.min(total);
        if cb < total {
            let fin = texto[cb..]
                .chars()
                .next()
                .map(|c| cb + c.len_utf8())
                .unwrap_or(cb + 1)
                .min(total);
            (cb, fin)
        } else {
            (total, total)
        }
    });

    // Rango del match activo (para comparación rápida)
    let rango_match_activo: Option<(usize, usize)> = match_activo
        .and_then(|idx| matches.get(idx))
        .copied();

    // ── Paso 3: asignar color a cada segmento ─────────────────────────────
    let mut resultado: Vec<(String, Attrs<'static>)> = Vec::new();

    for w in fronteras.windows(2) {
        let (seg_ini, seg_fin) = (w[0], w[1]);
        if seg_ini >= seg_fin || seg_fin > total {
            continue;
        }

        let mid = seg_ini;

        // Prioridad 6: cursor
        let es_cursor = cursor_range
            .map(|(cs, ce)| mid >= cs && mid < ce)
            .unwrap_or(false);

        if es_cursor {
            resultado.push((
                texto[seg_ini..seg_fin].to_string(),
                Attrs::new().family(Family::Monospace).color(COLOR_CURSOR),
            ));
            continue;
        }

        // Prioridad 5: diagnóstico (último que cubre mid gana)
        let color_diag = diagnosticos
            .iter()
            .rev()
            .find(|d| d.inicio_byte <= mid && d.fin_byte > mid)
            .map(|d| color_diagnostico(d.severidad));

        if let Some(color) = color_diag {
            resultado.push((
                texto[seg_ini..seg_fin].to_string(),
                Attrs::new().family(Family::Monospace).color(color),
            ));
            continue;
        }

        // Prioridad 4: match activo
        if let Some((ms, me)) = rango_match_activo {
            if mid >= ms && mid < me {
                resultado.push((
                    texto[seg_ini..seg_fin].to_string(),
                    Attrs::new().family(Family::Monospace).color(COLOR_MATCH_ACTIVO),
                ));
                continue;
            }
        }

        // Prioridad 3: match inactivo
        let es_match = matches.iter().any(|&(ms, me)| mid >= ms && mid < me);
        if es_match {
            resultado.push((
                texto[seg_ini..seg_fin].to_string(),
                Attrs::new().family(Family::Monospace).color(COLOR_MATCH),
            ));
            continue;
        }

        // Prioridad 2: span sintáctico (último que cubre mid gana)
        let color_sintax = spans
            .iter()
            .rev()
            .find(|s| s.inicio_byte <= mid && s.fin_byte > mid)
            .map(|s| Color::rgb(s.color.r, s.color.g, s.color.b));

        let color = color_sintax.unwrap_or(COLOR_TEXTO);
        resultado.push((
            texto[seg_ini..seg_fin].to_string(),
            Attrs::new().family(Family::Monospace).color(color),
        ));
    }

    // Cursor al final del documento (past-end): espacio sintético
    if let Some((cs, _)) = cursor_range {
        if cs >= total {
            resultado.push((
                " ".to_string(),
                Attrs::new().family(Family::Monospace).color(COLOR_CURSOR),
            ));
        }
    }

    resultado
}

// ------------------------------------------------------------------
// Helpers
// ------------------------------------------------------------------

/// Byte offset del cursor en `lineas.join("\n")`.
/// `cursor.columna` es índice de carácter (no byte); se convierte correctamente.
fn cursor_byte_offset(lineas: &[String], cursor: CursorRender) -> usize {
    let linea = (cursor.linea as usize).min(lineas.len().saturating_sub(1));
    let offset: usize = lineas[..linea].iter().map(|l| l.len() + 1).sum();

    let linea_str = lineas.get(linea).map(|s| s.as_str()).unwrap_or("");
    let byte_col = linea_str
        .char_indices()
        .nth(cursor.columna as usize)
        .map(|(b, _)| b)
        .unwrap_or(linea_str.len());

    offset + byte_col
}
