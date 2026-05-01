// Copyright 2026 Franz (frantsiles)
// Licensed under the Apache License, Version 2.0

//! # glyph-plugin-api
//!
//! Contratos públicos del SDK de plugins de Glyph.
//!
//! Cualquier plugin (Lua, WASM, Rust nativo) se expresa a través de estos
//! tipos. La capa de aplicación solo conoce `Plugin` + `AccionPlugin`;
//! no necesita saber si el plugin está escrito en Lua o en Rust.

use std::collections::HashMap;

// ------------------------------------------------------------------
// Contexto — lo que el plugin recibe en cada hook
// ------------------------------------------------------------------

/// Instantánea del estado del editor que se pasa al plugin en cada hook.
#[derive(Debug, Clone)]
pub struct ContextoPlugin {
    /// Ruta del archivo abierto (si la hay)
    pub ruta: Option<String>,
    /// Versión del documento (se incrementa en cada cambio)
    pub version_doc: u32,
}

// ------------------------------------------------------------------
// AccionPlugin — lo que el plugin puede devolver para afectar al editor
// ------------------------------------------------------------------

/// Acciones que un plugin puede solicitar al editor.
#[derive(Debug)]
pub enum AccionPlugin {
    /// Establece un tema de colores.
    /// Claves: nombres de tipo semántico (ver `TipoResaltado` en glyph-core).
    /// Valores: color RGB en formato `[r, g, b]`.
    EstablecerTema(HashMap<String, [u8; 3]>),

    /// Emite un mensaje informativo en el log del editor.
    LogMensaje(String),
}

// ------------------------------------------------------------------
// Trait Plugin
// ------------------------------------------------------------------

/// Interfaz que todo plugin debe implementar.
///
/// Los métodos tienen implementaciones por defecto que no hacen nada,
/// así un plugin solo implementa los hooks que le interesan.
pub trait Plugin: Send + 'static {
    fn nombre(&self) -> &str;

    fn descripcion(&self) -> &str {
        ""
    }

    /// Se llama una vez al cargar el plugin. Ideal para registrar temas,
    /// comandos y otras configuraciones estáticas.
    fn inicializar(&mut self) -> Vec<AccionPlugin> {
        vec![]
    }

    /// Se llama tras cada modificación de texto en el documento activo.
    fn al_cambiar(&mut self, _ctx: &ContextoPlugin) -> Vec<AccionPlugin> {
        vec![]
    }

    /// Se llama después de guardar el documento activo.
    fn al_guardar(&mut self, _ctx: &ContextoPlugin) -> Vec<AccionPlugin> {
        vec![]
    }

    /// Se llama al abrir un archivo.
    fn al_abrir(&mut self, _ctx: &ContextoPlugin) -> Vec<AccionPlugin> {
        vec![]
    }
}
