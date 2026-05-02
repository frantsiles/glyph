// Copyright 2026 Franz (frantsiles)
// Licensed under the Apache License, Version 2.0

//! # Resaltado sintáctico
//!
//! Parsea el texto del buffer con tree-sitter y produce spans semánticos.
//!
//! ## Lenguajes soportados
//!
//! | Extensión      | Lenguaje   | Gramática                    |
//! |---|---|---|
//! | `.rs`          | Rust       | tree-sitter-rust 0.21        |
//! | `.js` `.mjs`   | JavaScript | tree-sitter-javascript 0.21  |
//! | `.py` `.pyw`   | Python     | tree-sitter-python 0.21      |
//!
//! ## Arquitectura
//!
//! ```text
//! buffer (texto raw)
//!   └─ Resaltador::resaltar(texto, lenguaje)
//!       └─ HighlightConfiguration (por lenguaje, compilada una vez)
//!           └─ Vec<SpanSintactico>  (inicio_byte, fin_byte, TipoResaltado)
//! ```

use std::collections::HashMap;
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Lenguaje {
    Rust,
    JavaScript,
    Python,
    Desconocido,
}

impl Lenguaje {
    /// Detecta el lenguaje por extensión de archivo
    pub fn desde_extension(ext: &str) -> Self {
        match ext {
            "rs"              => Self::Rust,
            "js" | "mjs" | "cjs" => Self::JavaScript,
            "py" | "pyw"      => Self::Python,
            _                 => Self::Desconocido,
        }
    }
}

// ------------------------------------------------------------------
// Configuración interna de tree-sitter
// ------------------------------------------------------------------

/// Nombres de highlight que tree-sitter-highlight asignará a índices.
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
    TipoResaltado::Predeterminado,
];

// ------------------------------------------------------------------
// Resaltador
// ------------------------------------------------------------------

/// Resaltador de sintaxis basado en tree-sitter.
///
/// Crear una instancia por sesión — `HighlightConfiguration` es costosa
/// de inicializar (compila las queries) y debe reutilizarse entre frames.
pub struct Resaltador {
    configs: HashMap<Lenguaje, HighlightConfiguration>,
}

impl Resaltador {
    /// Inicializa el resaltador con todas las gramáticas incluidas.
    pub fn nuevo() -> Self {
        let mut configs = HashMap::new();

        // Rust
        match HighlightConfiguration::new(
            tree_sitter_rust::language(),
            "rust",
            tree_sitter_rust::HIGHLIGHTS_QUERY,
            tree_sitter_rust::INJECTIONS_QUERY,
            "",
        ) {
            Ok(mut c) => { c.configure(HIGHLIGHT_NAMES); configs.insert(Lenguaje::Rust, c); }
            Err(e) => tracing::warn!("No se pudo cargar gramática Rust: {e}"),
        }

        // JavaScript — exporta HIGHLIGHT_QUERY (sin 'S') e INJECTIONS_QUERY
        match HighlightConfiguration::new(
            tree_sitter_javascript::language(),
            "javascript",
            tree_sitter_javascript::HIGHLIGHT_QUERY,
            tree_sitter_javascript::INJECTIONS_QUERY,
            tree_sitter_javascript::LOCALS_QUERY,
        ) {
            Ok(mut c) => { c.configure(HIGHLIGHT_NAMES); configs.insert(Lenguaje::JavaScript, c); }
            Err(e) => tracing::warn!("No se pudo cargar gramática JavaScript: {e}"),
        }

        // Python — solo HIGHLIGHTS_QUERY, sin injections ni locals
        match HighlightConfiguration::new(
            tree_sitter_python::language(),
            "python",
            tree_sitter_python::HIGHLIGHTS_QUERY,
            "",
            "",
        ) {
            Ok(mut c) => { c.configure(HIGHLIGHT_NAMES); configs.insert(Lenguaje::Python, c); }
            Err(e) => tracing::warn!("No se pudo cargar gramática Python: {e}"),
        }

        Self { configs }
    }

    /// Resalta el texto y devuelve spans semánticos.
    ///
    /// Si el lenguaje es `Desconocido` o no hay gramática disponible,
    /// devuelve un único span predeterminado que cubre todo el texto.
    pub fn resaltar(&self, texto: &str, lenguaje: Lenguaje) -> Vec<SpanSintactico> {
        if let Some(config) = self.configs.get(&lenguaje) {
            self.resaltar_con_config(texto, config)
        } else {
            vec![SpanSintactico {
                inicio_byte: 0,
                fin_byte: texto.len(),
                tipo: TipoResaltado::Predeterminado,
            }]
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
                    spans.push(SpanSintactico { inicio_byte: start, fin_byte: end, tipo });
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
