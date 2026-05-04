// Copyright 2026 Franz (frantsiles)
// Licensed under the Apache License, Version 2.0

//! # Resaltado sintáctico
//!
//! Parsea el texto del buffer con tree-sitter y produce spans semánticos.
//!
//! ## Lenguajes soportados
//!
//! ### Con gramática tree-sitter (resaltado completo)
//!
//! | Extensión                   | Lenguaje    | Gramática                    |
//! |---|---|---|
//! | `.rs`                       | Rust        | tree-sitter-rust 0.21        |
//! | `.js` `.mjs` `.cjs`         | JavaScript  | tree-sitter-javascript 0.21  |
//! | `.jsx`                      | JSX         | tree-sitter-javascript 0.21  |
//! | `.py` `.pyw`                | Python      | tree-sitter-python 0.21      |
//! | `.json` `.jsonc`            | JSON        | tree-sitter-json 0.21        |
//! | `.yaml` `.yml`              | YAML        | tree-sitter-yaml 0.6         |
//! | `.toml`                     | TOML        | *(sin gramática — ver nota)* |
//! | `.sh` `.bash` `.zsh` `.fish`| Shell       | tree-sitter-bash 0.21        |
//! | `.ts`                       | TypeScript  | tree-sitter-typescript 0.21  |
//! | `.tsx`                      | TSX         | tree-sitter-typescript 0.21  |
//! | `.html` `.htm`              | HTML        | tree-sitter-html 0.20        |
//! | `.css`                      | CSS         | tree-sitter-css 0.21         |
//! | `.cs`                       | C#          | tree-sitter-c-sharp 0.21     |
//! | `.java`                     | Java        | tree-sitter-java 0.21        |
//!
//! ### Identificados, sin gramática (texto plano coloreado)
//!
//! | Extensión            | Lenguaje |
//! |---|---|
//! | `.scss` `.sass`      | SCSS     |
//! | `.md` `.markdown`    | Markdown |
//! | `.wit`               | WIT      |
//! | `.svg`               | SVG      |
//! | `.xml`               | XML      |
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

/// Lenguaje de programación del documento.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Lenguaje {
    // ── Con gramática tree-sitter ──────────────────────────────────
    Rust,
    JavaScript,
    Jsx,
    TypeScript,
    Tsx,
    Python,
    Json,
    Yaml,
    Bash,
    Html,
    Css,
    CSharp,
    Java,
    // ── Identificados, sin gramática (texto plano con nombre) ──────
    // TOML: crates disponibles requieren tree-sitter exacto 0.20/0.21, conflicto
    // con links=tree-sitter 0.22. Queda como lenguaje identificado hasta 0.23+.
    Toml,
    Scss,
    Markdown,
    Wit,
    Svg,
    Xml,
    // ── Sin identificar ────────────────────────────────────────────
    Desconocido,
}

impl Lenguaje {
    /// Detecta el lenguaje por extensión de archivo.
    pub fn desde_extension(ext: &str) -> Self {
        match ext {
            "rs"                          => Self::Rust,
            "js" | "mjs" | "cjs"          => Self::JavaScript,
            "jsx"                          => Self::Jsx,
            "ts"                           => Self::TypeScript,
            "tsx"                          => Self::Tsx,
            "py" | "pyw"                   => Self::Python,
            "json" | "jsonc"               => Self::Json,
            "yaml" | "yml"                 => Self::Yaml,
            "toml"                         => Self::Toml,
            "sh" | "bash" | "zsh" | "fish" => Self::Bash,
            "html" | "htm"                 => Self::Html,
            "css"                          => Self::Css,
            "scss" | "sass"                => Self::Scss,
            "cs"                           => Self::CSharp,
            "java"                         => Self::Java,
            "md" | "markdown"              => Self::Markdown,
            "wit"                          => Self::Wit,
            "svg"                          => Self::Svg,
            "xml"                          => Self::Xml,
            _                              => Self::Desconocido,
        }
    }

    /// Convierte un nombre de lenguaje (como el que envía un plugin) a enum.
    pub fn desde_nombre(nombre: &str) -> Self {
        match nombre.to_lowercase().as_str() {
            "rust"                    => Self::Rust,
            "javascript" | "js"       => Self::JavaScript,
            "jsx"                     => Self::Jsx,
            "typescript" | "ts"       => Self::TypeScript,
            "tsx"                     => Self::Tsx,
            "python" | "py"           => Self::Python,
            "json"                    => Self::Json,
            "yaml"                    => Self::Yaml,
            "toml"                    => Self::Toml,
            "bash" | "sh" | "shell"   => Self::Bash,
            "html"                    => Self::Html,
            "css"                     => Self::Css,
            "scss" | "sass"           => Self::Scss,
            "csharp" | "cs" | "c#"   => Self::CSharp,
            "java"                    => Self::Java,
            "markdown" | "md"         => Self::Markdown,
            "wit"                     => Self::Wit,
            "svg"                     => Self::Svg,
            "xml"                     => Self::Xml,
            _                         => Self::Desconocido,
        }
    }

    /// Nombre legible para mostrar en UI (barra de estado, paleta, etc.)
    pub fn nombre_display(&self) -> &'static str {
        match self {
            Self::Rust        => "Rust",
            Self::JavaScript  => "JavaScript",
            Self::Jsx         => "JSX",
            Self::TypeScript  => "TypeScript",
            Self::Tsx         => "TSX",
            Self::Python      => "Python",
            Self::Json        => "JSON",
            Self::Yaml        => "YAML",
            Self::Toml        => "TOML",
            Self::Bash        => "Shell",
            Self::Html        => "HTML",
            Self::Css         => "CSS",
            Self::Scss        => "SCSS",
            Self::CSharp      => "C#",
            Self::Java        => "Java",
            Self::Markdown    => "Markdown",
            Self::Wit         => "WIT",
            Self::Svg         => "SVG",
            Self::Xml         => "XML",
            Self::Desconocido => "Texto plano",
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

        // ── Rust ──────────────────────────────────────────────────────
        cargar_config(
            &mut configs, Lenguaje::Rust,
            tree_sitter_rust::language(), "rust",
            tree_sitter_rust::HIGHLIGHTS_QUERY,
            tree_sitter_rust::INJECTIONS_QUERY, "",
        );

        // ── JavaScript ────────────────────────────────────────────────
        // HIGHLIGHT_QUERY (sin 'S') — convención de este crate
        cargar_config(
            &mut configs, Lenguaje::JavaScript,
            tree_sitter_javascript::language(), "javascript",
            tree_sitter_javascript::HIGHLIGHT_QUERY,
            tree_sitter_javascript::INJECTIONS_QUERY,
            tree_sitter_javascript::LOCALS_QUERY,
        );

        // ── JSX — misma gramática que JavaScript ──────────────────────
        cargar_config(
            &mut configs, Lenguaje::Jsx,
            tree_sitter_javascript::language(), "jsx",
            tree_sitter_javascript::HIGHLIGHT_QUERY,
            tree_sitter_javascript::INJECTIONS_QUERY,
            tree_sitter_javascript::LOCALS_QUERY,
        );

        // ── Python ────────────────────────────────────────────────────
        cargar_config(
            &mut configs, Lenguaje::Python,
            tree_sitter_python::language(), "python",
            tree_sitter_python::HIGHLIGHTS_QUERY, "", "",
        );

        // ── JSON ──────────────────────────────────────────────────────
        cargar_config(
            &mut configs, Lenguaje::Json,
            tree_sitter_json::language(), "json",
            tree_sitter_json::HIGHLIGHTS_QUERY, "", "",
        );

        // ── YAML ──────────────────────────────────────────────────────
        cargar_config(
            &mut configs, Lenguaje::Yaml,
            tree_sitter_yaml::language(), "yaml",
            tree_sitter_yaml::HIGHLIGHTS_QUERY, "", "",
        );

        // ── TOML — sin gramática (todos los crates TOML requieren tree-sitter exacto
        //    0.20 o 0.21, incompatible con links=tree-sitter 0.22 del workspace) ────

        // ── Bash / Shell — HIGHLIGHT_QUERY (sin 'S') ──────────────────
        cargar_config(
            &mut configs, Lenguaje::Bash,
            tree_sitter_bash::language(), "bash",
            tree_sitter_bash::HIGHLIGHT_QUERY, "", "",
        );

        // ── TypeScript ────────────────────────────────────────────────
        cargar_config(
            &mut configs, Lenguaje::TypeScript,
            tree_sitter_typescript::language_typescript(), "typescript",
            tree_sitter_typescript::HIGHLIGHTS_QUERY,
            "",
            tree_sitter_typescript::LOCALS_QUERY,
        );

        // ── TSX ───────────────────────────────────────────────────────
        cargar_config(
            &mut configs, Lenguaje::Tsx,
            tree_sitter_typescript::language_tsx(), "tsx",
            tree_sitter_typescript::HIGHLIGHTS_QUERY,
            "",
            tree_sitter_typescript::LOCALS_QUERY,
        );

        // ── HTML ──────────────────────────────────────────────────────
        cargar_config(
            &mut configs, Lenguaje::Html,
            tree_sitter_html::language(), "html",
            tree_sitter_html::HIGHLIGHTS_QUERY,
            tree_sitter_html::INJECTIONS_QUERY, "",
        );

        // ── CSS ───────────────────────────────────────────────────────
        cargar_config(
            &mut configs, Lenguaje::Css,
            tree_sitter_css::language(), "css",
            tree_sitter_css::HIGHLIGHTS_QUERY, "", "",
        );

        // ── C# ────────────────────────────────────────────────────────
        cargar_config(
            &mut configs, Lenguaje::CSharp,
            tree_sitter_c_sharp::language(), "c_sharp",
            tree_sitter_c_sharp::HIGHLIGHTS_QUERY, "", "",
        );

        // ── Java ──────────────────────────────────────────────────────
        cargar_config(
            &mut configs, Lenguaje::Java,
            tree_sitter_java::language(), "java",
            tree_sitter_java::HIGHLIGHTS_QUERY, "", "",
        );

        Self { configs }
    }

    /// Resalta el texto y devuelve spans semánticos.
    ///
    /// Si el lenguaje es `Desconocido` o no tiene gramática registrada,
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

// ------------------------------------------------------------------
// Helper privado
// ------------------------------------------------------------------

fn cargar_config(
    configs: &mut HashMap<Lenguaje, HighlightConfiguration>,
    lenguaje: Lenguaje,
    language: tree_sitter::Language,
    nombre: &str,
    highlights: &str,
    injections: &str,
    locals: &str,
) {
    match HighlightConfiguration::new(language, nombre, highlights, injections, locals) {
        Ok(mut c) => {
            c.configure(HIGHLIGHT_NAMES);
            configs.insert(lenguaje, c);
        }
        Err(e) => tracing::warn!("No se pudo cargar gramática {nombre}: {e}"),
    }
}
