// Copyright 2026 Franz (frantsiles)
// Licensed under the Apache License, Version 2.0

//! Configuración global de la aplicación Glyph.
//!
//! Se carga desde `~/.config/glyph/config.toml` al arrancar.
//! Si el archivo no existe o contiene errores, se aplican los valores por defecto.
//!
//! ## Ejemplo de config.toml
//!
//! ```toml
//! [editor]
//! familia_fuente = "JetBrains Mono"
//! tamano_fuente  = 16.0
//! interlineado   = 1.4
//!
//! [ventana]
//! ancho = 1440
//! alto  = 900
//!
//! [lenguaje.rs]
//! tamano_tab = 4
//!
//! [lenguaje.lua]
//! tamano_tab    = 2
//! tamano_fuente = 14.0
//!
//! [lenguaje.py]
//! tamano_tab = 4
//! ```

use std::collections::HashMap;

use serde::Deserialize;

// ------------------------------------------------------------------
// Raíz
// ------------------------------------------------------------------

#[derive(Debug, Deserialize, Default)]
pub struct ConfigApp {
    #[serde(default)]
    pub editor: EditorConfig,

    #[serde(default)]
    pub ventana: VentanaConfig,

    /// Claves = extensión de archivo sin punto ("rs", "lua", "py"…)
    #[serde(default)]
    pub lenguaje: HashMap<String, LenguajeConfig>,
}

// ------------------------------------------------------------------
// Secciones
// ------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct EditorConfig {
    /// Familia tipográfica para el editor. Si es None usa la monospace del sistema.
    pub familia_fuente: Option<String>,

    /// Tamaño de fuente en puntos (toda la UI salvo overrides por lenguaje)
    pub tamano_fuente: f32,

    /// Altura de línea como múltiplo del tamaño de fuente
    pub interlineado: f32,
}

impl Default for EditorConfig {
    fn default() -> Self {
        Self {
            familia_fuente: None,
            tamano_fuente: 16.0,
            interlineado: 1.4,
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct VentanaConfig {
    pub ancho: u32,
    pub alto: u32,
}

impl Default for VentanaConfig {
    fn default() -> Self {
        Self { ancho: 1280, alto: 720 }
    }
}

#[derive(Debug, Deserialize, Default, Clone)]
pub struct LenguajeConfig {
    /// Espacios por nivel de indentación (Tab inserta esta cantidad de espacios)
    pub tamano_tab: Option<usize>,

    /// Sobreescribe el tamaño de fuente global para este lenguaje
    pub tamano_fuente: Option<f32>,

    /// Sobreescribe la familia tipográfica global para este lenguaje
    pub familia_fuente: Option<String>,
}

// ------------------------------------------------------------------
// Carga
// ------------------------------------------------------------------

impl ConfigApp {
    pub fn cargar() -> Self {
        let ruta = dirs::config_dir().map(|p| p.join("glyph/config.toml"));

        if let Some(ruta) = ruta.filter(|p| p.exists()) {
            match std::fs::read_to_string(&ruta) {
                Ok(contenido) => match toml::from_str::<ConfigApp>(&contenido) {
                    Ok(cfg) => {
                        tracing::info!("Config cargada desde {}", ruta.display());
                        return cfg;
                    }
                    Err(e) => tracing::warn!("Config inválida en {}: {e}", ruta.display()),
                },
                Err(e) => tracing::warn!("No se pudo leer la config: {e}"),
            }
        }

        ConfigApp::default()
    }

    /// Devuelve la configuración efectiva para una extensión de archivo dada.
    pub fn para_extension(&self, ext: &str) -> LenguajeConfig {
        self.lenguaje.get(ext).cloned().unwrap_or_default()
    }
}
