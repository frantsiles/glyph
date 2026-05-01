// Copyright 2026 Franz (frantsiles)
// Licensed under the Apache License, Version 2.0

//! # RendererTexto
//!
//! Renderiza texto usando `glyphon` (cosmic-text + wgpu).
//!
//! ## Jerarquía de color por fragmento
//!
//! ```text
//! 1. COLOR_TEXTO   (predeterminado)
//! 2. Span sintáctico  (tree-sitter)
//! 3. Diagnóstico LSP  (sobreescribe sintaxis en su rango)
//! 4. Cursor           (sobreescribe todo en su carácter)
//! ```
//!
//! ## Algoritmo de barrido de fronteras
//!
//! Todos los extremos de spans, diagnósticos y cursor se convierten en
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

fn color_diagnostico(severidad: SeveridadRender) -> Color {
    match severidad {
        SeveridadRender::Error       => Color::rgb(0xFF, 0x6B, 0x6B),
        SeveridadRender::Aviso       => Color::rgb(0xFF, 0xBF, 0x69),
        SeveridadRender::Informacion => Color::rgb(0x61, 0xAF, 0xEF),
        SeveridadRender::Sugerencia  => Color::rgb(0x98, 0x98, 0x98),
    }
}

/// Encapsula el pipeline de renderizado de texto.
pub struct RendererTexto {
    sistema_fuentes: FontSystem,
    cache_formas: SwashCache,
    atlas: TextAtlas,
    renderer: TextRenderer,
    buffer: Buffer,
    metricas: Metrics,
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

        Self { sistema_fuentes, cache_formas, atlas, renderer, buffer, metricas }
    }

    /// Actualiza el buffer de texto con el contenido del frame.
    pub fn actualizar_contenido(&mut self, contenido: &ContenidoRender, ancho: f32, alto: f32) {
        let Self { sistema_fuentes, buffer, metricas, .. } = self;

        buffer.set_metrics(sistema_fuentes, *metricas);
        buffer.set_size(sistema_fuentes, ancho, alto);

        let texto = contenido.texto_completo();
        let cursor_byte = contenido.cursor.map(|c| cursor_byte_offset(&contenido.lineas, c));

        let fragmentos = construir_spans_glyphon(
            &texto,
            &contenido.spans,
            &contenido.diagnosticos,
            cursor_byte,
        );

        let refs: Vec<(&str, Attrs)> =
            fragmentos.iter().map(|(s, a)| (s.as_str(), *a)).collect();

        buffer.set_rich_text(sistema_fuentes, refs, Shaping::Advanced);
        buffer.shape_until_scroll(sistema_fuentes);
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

        let Self { renderer, sistema_fuentes, atlas, buffer, cache_formas, .. } = self;

        renderer
            .prepare(
                dispositivo,
                cola,
                sistema_fuentes,
                atlas,
                Resolution { width: ancho, height: alto },
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
/// predeterminado < sintaxis < diagnóstico < cursor.
///
/// Garantiza corrección incluso con spans y diagnósticos solapados:
/// todos los extremos se convierten en fronteras y cada segmento
/// resultante recibe un único color según la prioridad más alta.
fn construir_spans_glyphon(
    texto: &str,
    spans: &[SpanTexto],
    diagnosticos: &[DiagnosticoRender],
    cursor_byte: Option<usize>,
) -> Vec<(String, Attrs<'static>)> {
    let total = texto.len();
    if total == 0 {
        // Texto vacío: solo muestra el cursor si lo hay
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

    // ── Paso 2: asignar color a cada segmento ────────────────────────────
    // El rango del cursor (para comparaciones rápidas)
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
            (total, total) // marcador para cursor past-end
        }
    });

    let mut resultado: Vec<(String, Attrs<'static>)> = Vec::new();

    for w in fronteras.windows(2) {
        let (seg_ini, seg_fin) = (w[0], w[1]);
        if seg_ini >= seg_fin || seg_fin > total {
            continue;
        }

        // Punto representativo del segmento (siempre en su interior)
        let mid = seg_ini;

        // Prioridad 4: cursor
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

        // Prioridad 3: diagnóstico (último que cubre mid gana)
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
