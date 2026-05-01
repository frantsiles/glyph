// Copyright 2026 Franz (frantsiles)
// Licensed under the Apache License, Version 2.0

//! # ContenidoRender
//!
//! Contrato de datos entre la capa de aplicación y el renderer.
//!
//! ## Por qué existe este tipo
//!
//! El renderer no conoce `glyph-core` — no sabe de `Document`, `Buffer` ni `Cursor`.
//! `ContenidoRender` es el DTO (Data Transfer Object) que la app construye a partir del
//! Document del core y pasa al renderer. Así podemos cambiar la representación interna
//! del core sin tocar el renderer, y viceversa.

/// Color RGB para un fragmento de texto (independiente de cualquier tema visual)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ColorRender {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl ColorRender {
    pub const fn rgb(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b }
    }
}

/// Fragmento de texto coloreado (posiciones en bytes, compatible con str slicing UTF-8)
#[derive(Debug, Clone)]
pub struct SpanTexto {
    pub inicio_byte: usize,
    pub fin_byte: usize,
    pub color: ColorRender,
}

/// Posición del cursor para renderizar (independiente de `glyph_core::Posicion`)
#[derive(Debug, Clone, Copy, Default)]
pub struct CursorRender {
    /// Línea del cursor (0-indexado)
    pub linea: u32,
    /// Columna del cursor (0-indexado)
    pub columna: u32,
}

/// Todo lo que el renderer necesita para dibujar un frame
#[derive(Debug, Clone)]
pub struct ContenidoRender {
    /// Líneas de texto a mostrar, sin separadores de línea
    pub lineas: Vec<String>,

    /// Posición del cursor principal, si debe mostrarse
    pub cursor: Option<CursorRender>,

    /// Tamaño de fuente en puntos (sobreescribe el de ConfigRenderer si presente)
    pub tamano_fuente: f32,

    /// Spans de sintaxis coloreados — vacío si no hay resaltado activo
    pub spans: Vec<SpanTexto>,
}

impl ContenidoRender {
    /// Contenido vacío: una línea en blanco, cursor en el origen
    pub fn vacio() -> Self {
        Self {
            lineas: vec![String::new()],
            cursor: Some(CursorRender::default()),
            tamano_fuente: 16.0,
            spans: Vec::new(),
        }
    }

    /// Construye el texto completo para pasar a cosmic-text/glyphon
    pub fn texto_completo(&self) -> String {
        self.lineas.join("\n")
    }
}
