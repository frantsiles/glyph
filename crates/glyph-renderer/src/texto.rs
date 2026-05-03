// Copyright 2026 Franz (frantsiles)
// Licensed under the Apache License, Version 2.0

//! # RendererTexto
//!
//! Renderiza texto usando `glyphon` (cosmic-text + wgpu).
//!
//! ## Tres áreas de renderizado
//!
//! ```text
//! ┌─ gutter ─┬─────── editor ──────────────────────────────┐
//! │  1       │ fn main() {                                  │
//! │  2       │     println!("hola");                        │
//! │  3  ◄──  │ }                                            │
//! │  4       │                                              │
//! ├──────────┴──────────────────────────────────────────────┤
//! │ main.rs | Ln 3, Col 1                                   │  ← barra
//! └─────────────────────────────────────────────────────────┘
//! ```
//!
//! ## Jerarquía de color por fragmento (editor)
//!
//! ```text
//! 1. COLOR_TEXTO      (predeterminado)
//! 2. Span sintáctico  (tree-sitter)
//! 3. Match inactivo   (búsqueda)
//! 4. Match activo     (búsqueda — match seleccionado)
//! 5. Diagnóstico LSP  (sobreescribe sintaxis y matches en su rango)
//! 6. Cursor           (sobreescribe todo en su carácter)
//! ```

use anyhow::{anyhow, Result};
use glyphon::{
    Attrs, Buffer, Color, Family, FontSystem, Metrics, Resolution, Shaping, SwashCache, TextArea,
    TextAtlas, TextBounds, TextRenderer,
};

use crate::contenido::{ContenidoRender, CursorRender, DiagnosticoRender, SeveridadRender, SpanTexto};


// Wallbash — paleta derivada del tema wallbash
const COLOR_TEXTO: Color = Color::rgb(0xFF, 0xFF, 0xFF);         // #FFFFFF texto principal
const COLOR_CURSOR: Color = Color::rgb(0xFF, 0xFF, 0xFF);        // #FFFFFF cursor
const COLOR_SELECCION: Color = Color::rgb(0x7A, 0xA2, 0xF7);    // #7AA2F7 azul selección
const COLOR_MATCH: Color = Color::rgb(0x9A, 0xB3, 0xE6);        // #9AB3E6 match inactivo
const COLOR_MATCH_ACTIVO: Color = Color::rgb(0xFF, 0xFF, 0xFF);  // blanco — match activo
const COLOR_TEXTO_BARRA: Color = Color::rgb(0xFF, 0xFF, 0xFF);   // #FFFFFF barra estado
const COLOR_GUTTER: Color = Color::rgb(0x65, 0x7A, 0xA3);       // #657AA3 número inactivo
const COLOR_GUTTER_ACTIVO: Color = Color::rgb(0xF0, 0xAA, 0xAC);// #F0AAAC número activo
const COLOR_HOVER: Color = Color::rgb(0xFF, 0xFF, 0xFF);         // blanco hover

const ALTURA_BARRA: f32 = 22.0;
const ALTURA_TABS: f32 = 32.0;

const COLOR_TAB_ACTIVO: Color = Color::rgb(0xFF, 0xFF, 0xFF);   // blanco — tab visible
const COLOR_TAB_INACTIVO: Color = Color::rgb(0xA2, 0xAE, 0xC8); // azul claro — tab inactivo
const COLOR_TAB_PUNTO: Color = Color::rgb(0xF0, 0xAA, 0xAC);    // rosa — punto de modificado
const MARGEN_SCROLL: i32 = 3;
// Fracción del tamaño de fuente que ocupa un carácter monoespaciado en anchura
const RATIO_CHAR: f32 = 0.601;
const PADDING_GUTTER: f32 = 10.0; // padding izquierdo + derecho del gutter
const POPUP_ANCHO: f32 = 520.0;
const POPUP_LINEAS: usize = 3;

fn color_diagnostico(severidad: SeveridadRender) -> Color {
    match severidad {
        SeveridadRender::Error       => Color::rgb(0xFF, 0x6B, 0x6B),
        SeveridadRender::Aviso       => Color::rgb(0xFF, 0xBF, 0x69),
        SeveridadRender::Informacion => Color::rgb(0x61, 0xAF, 0xEF),
        SeveridadRender::Sugerencia  => Color::rgb(0x98, 0x98, 0x98),
    }
}

/// Encapsula el pipeline de renderizado de texto (gutter + editor + barra de estado + hover).
pub struct RendererTexto {
    sistema_fuentes: FontSystem,
    cache_formas: SwashCache,
    atlas: TextAtlas,
    renderer: TextRenderer,
    buffer: Buffer,
    buffer_barra: Buffer,
    buffer_gutter: Buffer,
    buffer_hover: Buffer,
    metricas: Metrics,
    metricas_barra: Metrics,
    scroll_linea: i32,
    ancho_gutter: f32,
    /// true cuando hay hover activo que debe dibujarse
    hover_activo: bool,
    /// posición en píxeles donde anclar el popup (esquina superior-izquierda)
    hover_pos_px: (f32, f32),
    /// Posición del cursor en el frame anterior — para detectar si se movió
    cursor_anterior: Option<CursorRender>,
    /// Buffer de texto para la barra de tabs superior
    buffer_tabs: Buffer,
    metricas_tabs: Metrics,
    /// Límites X de cada tab (inicio, fin) para detección de clicks
    tabs_limites: Vec<(f32, f32)>,
}

impl RendererTexto {
    pub fn nuevo(
        dispositivo: &wgpu::Device,
        cola: &wgpu::Queue,
        formato: wgpu::TextureFormat,
        tamano_fuente: f32,
        multiplicador_linea: f32,
        familia_fuente: Option<&str>,
    ) -> Self {
        let mut sistema_fuentes = FontSystem::new();
        if let Some(familia) = familia_fuente {
            sistema_fuentes.db_mut().set_monospace_family(familia);
            tracing::info!("Fuente configurada: '{familia}'");
        }
        let cache_formas = SwashCache::new();
        let mut atlas = TextAtlas::new(dispositivo, cola, formato);
        let renderer = TextRenderer::new(
            &mut atlas,
            dispositivo,
            wgpu::MultisampleState::default(),
            None,
        );

        let metricas_tabs = Metrics::new(13.0, ALTURA_TABS);
        let mut buffer_tabs = Buffer::new(&mut sistema_fuentes, metricas_tabs);
        buffer_tabs.set_size(&mut sistema_fuentes, 1280.0, ALTURA_TABS);

        let metricas = Metrics::new(tamano_fuente, tamano_fuente * multiplicador_linea);
        let mut buffer = Buffer::new(&mut sistema_fuentes, metricas);
        buffer.set_size(&mut sistema_fuentes, 1280.0, 720.0);

        let metricas_barra = Metrics::new(13.0, ALTURA_BARRA);
        let mut buffer_barra = Buffer::new(&mut sistema_fuentes, metricas_barra);
        buffer_barra.set_size(&mut sistema_fuentes, 1280.0, ALTURA_BARRA);

        // El gutter usa las mismas métricas que el editor para alinearse visualmente
        let mut buffer_gutter = Buffer::new(&mut sistema_fuentes, metricas);
        buffer_gutter.set_size(&mut sistema_fuentes, 48.0, 720.0);

        // El popup de hover usa las mismas métricas que el editor
        let mut buffer_hover = Buffer::new(&mut sistema_fuentes, metricas);
        buffer_hover.set_size(&mut sistema_fuentes, POPUP_ANCHO, metricas.line_height * POPUP_LINEAS as f32 + 4.0);

        Self {
            sistema_fuentes,
            cache_formas,
            atlas,
            renderer,
            buffer,
            buffer_barra,
            buffer_gutter,
            buffer_hover,
            metricas,
            metricas_barra,
            scroll_linea: 0,
            ancho_gutter: 48.0,
            hover_activo: false,
            hover_pos_px: (0.0, 0.0),
            cursor_anterior: None,
            buffer_tabs,
            metricas_tabs,
            tabs_limites: Vec::new(),
        }
    }

    /// Devuelve la altura en píxeles de la barra de tabs.
    pub fn altura_tabs(&self) -> f32 { ALTURA_TABS }

    /// Devuelve el índice del tab sobre el que está la coordenada X, o None.
    pub fn tab_en_posicion(&self, x: f32) -> Option<usize> {
        self.tabs_limites.iter().position(|&(x0, x1)| x >= x0 && x < x1)
    }

    /// Ajusta el scroll manualmente (rueda del ratón).
    /// El cursor tracking no anulará este ajuste mientras el cursor no se mueva.
    pub fn ajustar_scroll(&mut self, delta: i32) {
        self.scroll_linea = (self.scroll_linea + delta).max(0);
    }

    /// Actualiza los buffers de texto con el contenido del frame.
    pub fn actualizar_contenido(&mut self, contenido: &ContenidoRender, ancho: f32, alto: f32) {
        let alto_editor = (alto - ALTURA_BARRA - ALTURA_TABS).max(1.0);

        // — Barra de tabs ──────────────────────────────────────────────────
        self.tabs_limites.clear();
        let char_w_tab = self.metricas_tabs.font_size * RATIO_CHAR;
        let mut tab_x = 0.0f32;
        let tab_frags: Vec<(String, Attrs<'static>)> = contenido.tabs.iter()
            .flat_map(|tab| {
                let color = if tab.activo { COLOR_TAB_ACTIVO } else { COLOR_TAB_INACTIVO };
                let base = Attrs::new().family(Family::Monospace);
                if tab.modificado {
                    let prefijo = format!("  ● ");
                    let sufijo = format!("{}  ", tab.nombre);
                    let ancho = (prefijo.chars().count() + sufijo.chars().count()) as f32 * char_w_tab;
                    self.tabs_limites.push((tab_x, tab_x + ancho));
                    tab_x += ancho;
                    vec![
                        (prefijo, base.color(COLOR_TAB_PUNTO)),
                        (sufijo, base.color(color)),
                    ]
                } else {
                    let texto_tab = format!("  {}  ", tab.nombre);
                    let ancho = texto_tab.chars().count() as f32 * char_w_tab;
                    self.tabs_limites.push((tab_x, tab_x + ancho));
                    tab_x += ancho;
                    vec![(texto_tab, base.color(color))]
                }
            })
            .collect();
        let frags_refs: Vec<(&str, Attrs)> =
            tab_frags.iter().map(|(s, a)| (s.as_str(), *a)).collect();
        self.buffer_tabs.set_metrics(&mut self.sistema_fuentes, self.metricas_tabs);
        self.buffer_tabs.set_size(&mut self.sistema_fuentes, ancho, ALTURA_TABS);
        self.buffer_tabs.set_rich_text(&mut self.sistema_fuentes, frags_refs, Shaping::Advanced);
        self.buffer_tabs.shape_until_scroll(&mut self.sistema_fuentes);

        // — Scroll tracking: solo activa cuando el cursor se mueve ──────────
        let cursor_movio = contenido.cursor != self.cursor_anterior;
        if cursor_movio {
            if let Some(cursor) = contenido.cursor {
                let cursor_line = cursor.linea as i32;
                let visible_lines = (alto_editor / self.metricas.line_height) as i32;
                let visible_lines = visible_lines.max(1);

                if cursor_line < self.scroll_linea + MARGEN_SCROLL {
                    self.scroll_linea = (cursor_line - MARGEN_SCROLL).max(0);
                } else if cursor_line >= self.scroll_linea + visible_lines - MARGEN_SCROLL {
                    self.scroll_linea = (cursor_line - visible_lines + 1 + MARGEN_SCROLL).max(0);
                }
            }
        }
        self.cursor_anterior = contenido.cursor;

        // — Gutter (números de línea) ──────────────────────────────────────
        let total_lineas = contenido.lineas.len().max(1);
        let n_digitos = digitos(total_lineas);
        let char_ancho = self.metricas.font_size * RATIO_CHAR;
        self.ancho_gutter = (n_digitos as f32 * char_ancho) + PADDING_GUTTER;
        let cursor_line = contenido.cursor.map(|c| c.linea as usize).unwrap_or(usize::MAX);

        let gutter_frags: Vec<(String, Attrs<'static>)> = (1..=total_lineas)
            .map(|n| {
                let s = format!("{:>width$}\n", n, width = n_digitos);
                let color = if n - 1 == cursor_line {
                    COLOR_GUTTER_ACTIVO
                } else {
                    COLOR_GUTTER
                };
                (s, Attrs::new().family(Family::Monospace).color(color))
            })
            .collect();

        let Self {
            sistema_fuentes,
            buffer,
            buffer_barra,
            buffer_gutter,
            buffer_hover,
            metricas,
            metricas_barra,
            scroll_linea,
            ancho_gutter,
            hover_activo,
            hover_pos_px,
            ..
        } = self;

        buffer_gutter.set_metrics(sistema_fuentes, *metricas);
        buffer_gutter.set_size(sistema_fuentes, *ancho_gutter, alto_editor);
        let gutter_refs: Vec<(&str, Attrs)> =
            gutter_frags.iter().map(|(s, a)| (s.as_str(), *a)).collect();
        buffer_gutter.set_rich_text(sistema_fuentes, gutter_refs, Shaping::Basic);
        // set_scroll DESPUÉS de set_rich_text para no ser anulado
        buffer_gutter.set_scroll(*scroll_linea);
        buffer_gutter.shape_until_scroll(sistema_fuentes);

        // — Editor principal ───────────────────────────────────────────────
        let texto_left = *ancho_gutter;
        let ancho_editor = (ancho - texto_left).max(1.0);

        buffer.set_metrics(sistema_fuentes, *metricas);
        buffer.set_size(sistema_fuentes, ancho_editor, alto_editor);

        let texto = contenido.texto_completo();
        let cursor_byte = contenido.cursor.map(|c| cursor_byte_offset(&contenido.lineas, c));

        let fragmentos = construir_spans_glyphon(
            &texto,
            &contenido.spans,
            contenido.seleccion_bytes,
            &contenido.matches_busqueda,
            contenido.match_activo,
            &contenido.diagnosticos,
            cursor_byte,
        );

        let refs: Vec<(&str, Attrs)> =
            fragmentos.iter().map(|(s, a)| (s.as_str(), *a)).collect();

        buffer.set_rich_text(sistema_fuentes, refs, Shaping::Advanced);
        // set_scroll DESPUÉS de set_rich_text para no ser anulado
        buffer.set_scroll(*scroll_linea);
        buffer.shape_until_scroll(sistema_fuentes);

        // — Barra de estado ────────────────────────────────────────────────
        buffer_barra.set_metrics(sistema_fuentes, *metricas_barra);
        buffer_barra.set_size(sistema_fuentes, ancho, ALTURA_BARRA);

        let barra_attrs = Attrs::new().family(Family::Monospace).color(COLOR_TEXTO_BARRA);
        buffer_barra.set_rich_text(
            sistema_fuentes,
            std::iter::once((contenido.barra_estado.as_str(), barra_attrs)),
            Shaping::Advanced,
        );
        buffer_barra.shape_until_scroll(sistema_fuentes);

        // — Popup de hover ─────────────────────────────────────────────────
        *hover_activo = contenido.hover_texto.is_some();
        if let Some(ref texto_hover) = contenido.hover_texto {
            // Truncar a POPUP_LINEAS líneas, máx 90 chars cada una
            let truncado: String = texto_hover
                .lines()
                .take(POPUP_LINEAS)
                .map(|l| if l.chars().count() > 90 { l.chars().take(90).collect() } else { l.to_string() })
                .collect::<Vec<_>>()
                .join("\n");

            let popup_alto = metricas.line_height * POPUP_LINEAS as f32 + 4.0;
            buffer_hover.set_metrics(sistema_fuentes, *metricas);
            buffer_hover.set_size(sistema_fuentes, POPUP_ANCHO, popup_alto);

            let hover_attrs = Attrs::new().family(Family::Monospace).color(COLOR_HOVER);
            buffer_hover.set_rich_text(
                sistema_fuentes,
                std::iter::once((truncado.as_str(), hover_attrs)),
                Shaping::Advanced,
            );
            buffer_hover.shape_until_scroll(sistema_fuentes);

            // Calcular posición en píxeles: encima del cursor, o debajo si está arriba
            if let Some(cursor) = contenido.cursor {
                let char_ancho_local = metricas.font_size * RATIO_CHAR;
                let linea_visible = cursor.linea as i32 - *scroll_linea;
                let cx = *ancho_gutter + (cursor.columna as f32 * char_ancho_local) + 4.0;
                let cy = linea_visible as f32 * metricas.line_height + 8.0;
                let py = if linea_visible <= 2 {
                    cy + metricas.line_height + 4.0
                } else {
                    cy - popup_alto - 4.0
                };
                *hover_pos_px = (cx.max(*ancho_gutter + 4.0), py.max(8.0));
            }
        }
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

        let tabs_top = ALTURA_TABS as i32;
        let alto_editor = (alto as i32) - ALTURA_BARRA as i32;
        let gutter_ancho_i = self.ancho_gutter as i32;
        let texto_left = self.ancho_gutter;

        let Self {
            renderer,
            sistema_fuentes,
            atlas,
            buffer,
            buffer_barra,
            buffer_gutter,
            buffer_hover,
            buffer_tabs,
            cache_formas,
            hover_activo,
            hover_pos_px,
            metricas,
            ..
        } = self;

        let mut areas: Vec<TextArea> = vec![
            // 1. Barra de tabs
            TextArea {
                buffer: buffer_tabs,
                left: 0.0,
                top: 0.0,
                scale: 1.0,
                bounds: TextBounds {
                    left: 0,
                    top: 0,
                    right: ancho as i32,
                    bottom: tabs_top,
                },
                default_color: COLOR_TAB_INACTIVO,
            },
            // 2. Gutter de números de línea
            TextArea {
                buffer: buffer_gutter,
                left: 4.0,
                top: ALTURA_TABS + 8.0,
                scale: 1.0,
                bounds: TextBounds {
                    left: 0,
                    top: tabs_top,
                    right: gutter_ancho_i,
                    bottom: alto_editor,
                },
                default_color: COLOR_GUTTER,
            },
            // 3. Editor de texto
            TextArea {
                buffer,
                left: texto_left + 4.0,
                top: ALTURA_TABS + 8.0,
                scale: 1.0,
                bounds: TextBounds {
                    left: gutter_ancho_i,
                    top: tabs_top,
                    right: ancho as i32,
                    bottom: alto_editor,
                },
                default_color: COLOR_TEXTO,
            },
            // 3. Barra de estado
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
        ];

        // 4. Popup de hover (sólo cuando hay hover activo)
        if *hover_activo {
            let (hx, hy) = *hover_pos_px;
            let hx = hx.min(ancho as f32 - POPUP_ANCHO).max(gutter_ancho_i as f32);
            let popup_alto = (metricas.line_height * POPUP_LINEAS as f32 + 4.0) as i32;
            areas.push(TextArea {
                buffer: buffer_hover,
                left: hx,
                top: hy,
                scale: 1.0,
                bounds: TextBounds {
                    left: hx as i32,
                    top: hy as i32,
                    right: (hx as i32 + POPUP_ANCHO as i32).min(ancho as i32),
                    bottom: (hy as i32 + popup_alto).min(alto_editor),
                },
                default_color: COLOR_HOVER,
            });
        }

        renderer
            .prepare(
                dispositivo,
                cola,
                sistema_fuentes,
                atlas,
                Resolution { width: ancho, height: alto },
                areas,
                cache_formas,
            )
            .map_err(|e| anyhow!("glyphon prepare falló: {e:?}"))
    }

    pub fn ancho_gutter(&self) -> f32 { self.ancho_gutter }
    pub fn scroll_linea(&self) -> i32 { self.scroll_linea }

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

/// Construye fragmentos `(texto, Attrs)` respetando la jerarquía de color.
fn construir_spans_glyphon(
    texto: &str,
    spans: &[SpanTexto],
    seleccion: Option<(usize, usize)>,
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

    // ── Fronteras ────────────────────────────────────────────────────────
    let mut fronteras: Vec<usize> = vec![0, total];

    for s in spans {
        fronteras.push(s.inicio_byte.min(total));
        fronteras.push(s.fin_byte.min(total));
    }
    for d in diagnosticos {
        fronteras.push(d.inicio_byte.min(total));
        fronteras.push(d.fin_byte.min(total));
    }
    if let Some((ss, se)) = seleccion {
        fronteras.push(ss.min(total));
        fronteras.push(se.min(total));
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

    // ── Rango cursor ─────────────────────────────────────────────────────
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

    let rango_match_activo: Option<(usize, usize)> = match_activo
        .and_then(|idx| matches.get(idx))
        .copied();

    // ── Asignar color a cada segmento ────────────────────────────────────
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

        // Prioridad 5: diagnóstico
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

        // Prioridad 2.5: selección (supera sintaxis, cediendo a matches y diagnósticos)
        if let Some((ss, se)) = seleccion {
            if mid >= ss && mid < se {
                resultado.push((
                    texto[seg_ini..seg_fin].to_string(),
                    Attrs::new().family(Family::Monospace).color(COLOR_SELECCION),
                ));
                continue;
            }
        }

        // Prioridad 3: match inactivo
        if matches.iter().any(|&(ms, me)| mid >= ms && mid < me) {
            resultado.push((
                texto[seg_ini..seg_fin].to_string(),
                Attrs::new().family(Family::Monospace).color(COLOR_MATCH),
            ));
            continue;
        }

        // Prioridad 2: span sintáctico
        let color_sintax = spans
            .iter()
            .rev()
            .find(|s| s.inicio_byte <= mid && s.fin_byte > mid)
            .map(|s| Color::rgb(s.color.r, s.color.g, s.color.b));

        resultado.push((
            texto[seg_ini..seg_fin].to_string(),
            Attrs::new()
                .family(Family::Monospace)
                .color(color_sintax.unwrap_or(COLOR_TEXTO)),
        ));
    }

    // Cursor past-end
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

/// Número de dígitos decimales de `n` (mínimo 1).
fn digitos(n: usize) -> usize {
    if n == 0 { return 1; }
    let mut d = 0;
    let mut v = n;
    while v > 0 {
        d += 1;
        v /= 10;
    }
    d
}

/// Byte offset del cursor en `lineas.join("\n")`.
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
