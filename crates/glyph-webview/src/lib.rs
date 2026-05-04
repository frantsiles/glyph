// Copyright 2026 Franz (frantsiles)
// Licensed under the Apache License, Version 2.0

//! # glyph-webview
//!
//! Renderiza vistas HTML externas (preview Markdown, diagramas, etc.) abriéndolas
//! en el navegador por defecto del sistema. El archivo HTML se actualiza en disco
//! y el navegador lo recarga automáticamente vía `<meta http-equiv="refresh">`.

use std::collections::HashMap;
use std::path::PathBuf;

use pulldown_cmark::{CodeBlockKind, Event, Options, Parser, Tag};

// ------------------------------------------------------------------
// GestorVistas
// ------------------------------------------------------------------

/// Gestiona ventanas de vista previa HTML abiertas en el navegador del sistema.
///
/// Cada vista tiene un ID único. La primera apertura lanza el navegador;
/// las actualizaciones posteriores solo reescriben el archivo HTML en disco —
/// el navegador recarga gracias al `<meta http-equiv="refresh" content="2">`.
pub struct GestorVistas {
    vistas: HashMap<String, PathBuf>,
}

impl GestorVistas {
    pub fn nuevo() -> Self {
        Self { vistas: HashMap::new() }
    }

    /// Abre o actualiza una vista de preview.
    /// - Primera llamada: escribe HTML en `/tmp/glyph_preview_{id}.html` y abre el navegador.
    /// - Llamadas siguientes: solo reescribe el archivo (el navegador auto-recarga).
    pub fn abrir_o_actualizar(&mut self, id: &str, titulo: &str, contenido_md: &str) -> anyhow::Result<()> {
        let es_nueva = !self.vistas.contains_key(id);
        let ruta = self.vistas
            .entry(id.to_string())
            .or_insert_with(|| std::env::temp_dir().join(format!("glyph_preview_{id}.html")));

        std::fs::write(&*ruta, md_a_html(titulo, contenido_md).as_bytes())?;

        if es_nueva {
            if let Err(e) = open::that(&*ruta) {
                tracing::warn!("No se pudo abrir el navegador para preview: {e}");
            }
        }
        Ok(())
    }

    /// Cierra la vista y elimina el archivo temporal.
    pub fn cerrar(&mut self, id: &str) {
        if let Some(ruta) = self.vistas.remove(id) {
            let _ = std::fs::remove_file(ruta);
        }
    }

    pub fn esta_abierta(&self, id: &str) -> bool {
        self.vistas.contains_key(id)
    }
}

impl Default for GestorVistas {
    fn default() -> Self { Self::nuevo() }
}

// ------------------------------------------------------------------
// Markdown → HTML
// ------------------------------------------------------------------

/// Convierte Markdown a un documento HTML completo con tema Catppuccin Mocha
/// y soporte de diagramas Mermaid.
pub fn md_a_html(titulo: &str, texto: &str) -> String {
    let body = md_a_body(texto);
    [TEMPLATE_START, titulo, TEMPLATE_MID, &body, TEMPLATE_END].concat()
}

fn md_a_body(texto: &str) -> String {
    let mut en_mermaid = false;
    let mut eventos: Vec<Event> = Vec::new();

    for event in Parser::new_ext(texto, Options::all()) {
        // Detectar inicio de bloque mermaid sin mover el event
        let es_mermaid_start = matches!(
            &event,
            Event::Start(Tag::CodeBlock(CodeBlockKind::Fenced(lang))) if lang.as_ref() == "mermaid"
        );

        if es_mermaid_start {
            en_mermaid = true;
            eventos.push(Event::Html(r#"<div class="mermaid">"#.into()));
        } else if en_mermaid {
            match event {
                Event::End(Tag::CodeBlock(_)) => {
                    en_mermaid = false;
                    eventos.push(Event::Html("</div>".into()));
                }
                Event::Text(t) => {
                    // Emitir como Html crudo para que Mermaid.js reciba el texto sin escapar
                    eventos.push(Event::Html(t));
                }
                _ => {}
            }
        } else {
            eventos.push(event);
        }
    }

    let mut html = String::new();
    pulldown_cmark::html::push_html(&mut html, eventos.into_iter());
    html
}

// ------------------------------------------------------------------
// Plantilla HTML — Catppuccin Mocha + Mermaid CDN
// ------------------------------------------------------------------

const TEMPLATE_START: &str = r#"<!DOCTYPE html>
<html lang="es">
<head>
<meta charset="utf-8">
<meta http-equiv="refresh" content="2">
<title>"#;

const TEMPLATE_MID: &str = r#"</title>
<style>
*{box-sizing:border-box}
:root{color-scheme:dark}
body{background:#1e1e2e;color:#cdd6f4;font-family:"Segoe UI",system-ui,sans-serif;max-width:860px;margin:0 auto;padding:2rem 2rem 4rem;line-height:1.6}
a{color:#89b4fa;text-decoration:none}
a:hover{text-decoration:underline}
h1,h2,h3,h4,h5,h6{color:#b4befe;margin-top:1.5rem}
h1,h2{border-bottom:1px solid #313244;padding-bottom:.3rem}
pre{background:#181825;border-radius:8px;padding:1rem 1.2rem;overflow-x:auto}
code{font-family:"Cascadia Code","Fira Code",monospace;font-size:.88em}
:not(pre)>code{background:#313244;color:#cba6f7;padding:2px 6px;border-radius:4px}
blockquote{border-left:4px solid #89b4fa;margin:1rem 0;padding:.5rem 1rem;background:#181825;border-radius:0 8px 8px 0;color:#bac2de}
table{border-collapse:collapse;width:100%;margin:1rem 0}
th,td{border:1px solid #313244;padding:8px 12px}
th{background:#181825;color:#b4befe;font-weight:600}
tr:nth-child(even){background:#1e1e2e}
tr:nth-child(odd){background:#181825}
hr{border:none;border-top:1px solid #313244;margin:2rem 0}
img{max-width:100%;border-radius:8px}
.mermaid{background:#181825;border-radius:8px;padding:1rem;margin:1rem 0;text-align:center}
.mermaid svg{max-width:100%}
</style>
</head>
<body>
"#;

const TEMPLATE_END: &str = r#"
<script src="https://cdn.jsdelivr.net/npm/mermaid@10/dist/mermaid.min.js"></script>
<script>mermaid.initialize({startOnLoad:true,theme:"dark"});</script>
</body>
</html>"#;
