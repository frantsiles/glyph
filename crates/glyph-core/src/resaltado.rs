// Copyright 2026 Franz (frantsiles)
// Licensed under the Apache License, Version 2.0

//! # Resaltado sintáctico
//!
//! Parsea el texto del buffer con tree-sitter y produce spans semánticos.
//! El core solo asigna *tipos* (PalabraClave, Cadena, etc.) — la capa de
//! aplicación es la responsable de mapear esos tipos a colores concretos
//! (el tema vivirá en un plugin Lua en Milestone 3).
//!
//! ## Arquitectura
//!
//! ```text
//! buffer (texto raw)
//!   └─ Resaltador::resaltar()
//!       └─ tree-sitter-highlight → HighlightEvent stream
//!           └─ Vec<SpanSintactico>  (inicio_byte, fin_byte, TipoResaltado)
//! ```

use tree_sitter_highlight::{HighlightConfiguration, HighlightEvent, Highlighter};

// ------------------------------------------------------------------
// Tipos semánticos
// ------------------------------------------------------------------

/// Tipo semántico de un fragmento de código.
/// Independiente del tema visual — eso lo decide la capa de aplicación.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TipoResaltado {
    PalabraClave,
    CadenaTexto,
    Comentario,
    Funcion,
    Tipo,
    Numero,
    Operador,
    Variable,
    Constante,
    Puntuacion,
    Atributo,
    Predeterminado,
}

// ------------------------------------------------------------------
// Span de salida
// ------------------------------------------------------------------

/// Fragmento de texto con su tipo semántico.
/// Las posiciones son en bytes (compatibles con str slicing en UTF-8).
#[derive(Debug, Clone)]
pub struct SpanSintactico {
    pub inicio_byte: usize,
    pub fin_byte: usize,
    pub tipo: TipoResaltado,
}

// ------------------------------------------------------------------
// Detección de lenguaje
// ------------------------------------------------------------------

/// Lenguaje de programación detectado del documento
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lenguaje {
    Rust,
    Desconocido,
}

impl Lenguaje {
    /// Detecta el lenguaje por extensión de archivo
    pub fn desde_extension(ext: &str) -> Self {
        match ext {
            "rs" => Self::Rust,
            _ => Self::Desconocido,
        }
    }
}

// ------------------------------------------------------------------
// Configuración interna de tree-sitter
// ------------------------------------------------------------------

/// Nombres de highlight que tree-sitter-highlight asignará a índices.
/// El orden determina la prioridad cuando hay ambigüedad.
const HIGHLIGHT_NAMES: &[&str] = &[
    "keyword",     // 0
    "string",      // 1
    "comment",     // 2
    "function",    // 3
    "type",        // 4
    "number",      // 5
    "operator",    // 6
    "variable",    // 7
    "constant",    // 8
    "punctuation", // 9
    "attribute",   // 10
    "label",       // 11
];

/// Mapeo índice → TipoResaltado. Debe coincidir con HIGHLIGHT_NAMES.
const TIPOS: &[TipoResaltado] = &[
    TipoResaltado::PalabraClave,
    TipoResaltado::CadenaTexto,
    TipoResaltado::Comentario,
    TipoResaltado::Funcion,
    TipoResaltado::Tipo,
    TipoResaltado::Numero,
    TipoResaltado::Operador,
    TipoResaltado::Variable,
    TipoResaltado::Constante,
    TipoResaltado::Puntuacion,
    TipoResaltado::Atributo,
    TipoResaltado::Predeterminado, // label → predeterminado por ahora
];

// ------------------------------------------------------------------
// Resaltador
// ------------------------------------------------------------------

/// Resaltador de sintaxis basado en tree-sitter.
///
/// Crear una instancia por sesión — `HighlightConfiguration` es costosa
/// de inicializar (compila las queries) y debe reutilizarse entre frames.
pub struct Resaltador {
    config_rust: HighlightConfiguration,
}

impl Resaltador {
    /// Inicializa el resaltador con las gramáticas incluidas.
    pub fn nuevo() -> Self {
        let mut config_rust = HighlightConfiguration::new(
            tree_sitter_rust::language(),
            "rust",
            tree_sitter_rust::HIGHLIGHTS_QUERY,
            tree_sitter_rust::INJECTIONS_QUERY,
            "", // tree-sitter-rust 0.21 no exporta LOCALS_QUERY
        )
        .expect("Fallo compilando queries de tree-sitter-rust");

        config_rust.configure(HIGHLIGHT_NAMES);

        Self { config_rust }
    }

    /// Resalta el texto y devuelve spans semánticos.
    ///
    /// Los spans son contiguos y cubren todo el texto de inicio a fin.
    /// Si el lenguaje es `Desconocido`, devuelve un único span predeterminado.
    pub fn resaltar(&self, texto: &str, lenguaje: Lenguaje) -> Vec<SpanSintactico> {
        match lenguaje {
            Lenguaje::Rust => self.resaltar_con_config(texto, &self.config_rust),
            Lenguaje::Desconocido => vec![SpanSintactico {
                inicio_byte: 0,
                fin_byte: texto.len(),
                tipo: TipoResaltado::Predeterminado,
            }],
        }
    }

    fn resaltar_con_config(&self, texto: &str, config: &HighlightConfiguration) -> Vec<SpanSintactico> {
        let mut highlighter = Highlighter::new();

        let eventos = match highlighter.highlight(config, texto.as_bytes(), None, |_| None) {
            Ok(e) => e,
            Err(e) => {
                tracing::warn!("Error al parsear con tree-sitter: {e}");
                return vec![SpanSintactico {
                    inicio_byte: 0,
                    fin_byte: texto.len(),
                    tipo: TipoResaltado::Predeterminado,
                }];
            }
        };

        let mut spans: Vec<SpanSintactico> = Vec::new();
        // Pila de tipos activos — el tope es el highlight más específico
        let mut pila: Vec<TipoResaltado> = Vec::new();

        for evento in eventos {
            match evento {
                Ok(HighlightEvent::HighlightStart(h)) => {
                    let tipo = TIPOS.get(h.0).copied().unwrap_or(TipoResaltado::Predeterminado);
                    pila.push(tipo);
                }
                Ok(HighlightEvent::HighlightEnd) => {
                    pila.pop();
                }
                Ok(HighlightEvent::Source { start, end }) => {
                    let tipo = pila.last().copied().unwrap_or(TipoResaltado::Predeterminado);
                    spans.push(SpanSintactico {
                        inicio_byte: start,
                        fin_byte: end,
                        tipo,
                    });
                }
                Err(e) => {
                    tracing::warn!("Error en stream de eventos tree-sitter: {e}");
                    break;
                }
            }
        }

        spans
    }
}

impl Default for Resaltador {
    fn default() -> Self {
        Self::nuevo()
    }
}
