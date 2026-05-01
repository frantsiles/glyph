// Copyright 2026 Franz (frantsiles)
// Licensed under the Apache License, Version 2.0

//! Configuración inicial del renderer — todos los parámetros en un solo lugar.

/// Parámetros de arranque de la ventana y el renderer
#[derive(Debug, Clone)]
pub struct ConfigRenderer {
    /// Título de la ventana del OS
    pub titulo: String,

    /// Ancho inicial en píxeles físicos
    pub ancho: u32,

    /// Alto inicial en píxeles físicos
    pub alto: u32,

    /// Tamaño de fuente en puntos
    pub tamano_fuente: f32,

    /// Altura de línea como múltiplo del tamaño de fuente
    pub multiplicador_linea: f32,
}

impl Default for ConfigRenderer {
    fn default() -> Self {
        Self {
            titulo: "Glyph".to_string(),
            ancho: 1280,
            alto: 720,
            tamano_fuente: 16.0,
            multiplicador_linea: 1.4,
        }
    }
}

impl ConfigRenderer {
    /// Altura de línea calculada en píxeles
    pub fn altura_linea(&self) -> f32 {
        self.tamano_fuente * self.multiplicador_linea
    }
}
