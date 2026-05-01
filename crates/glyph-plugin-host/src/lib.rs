// Copyright 2026 Franz (frantsiles)
// Licensed under the Apache License, Version 2.0

//! # glyph-plugin-host
//!
//! Orquestador del sistema de plugins de Glyph.
//!
//! ## Flujo
//!
//! ```text
//! HostPlugins::nuevo()
//!   └─ cargar_lua(nombre, script)  →  registra un PluginLua
//! HostPlugins::inicializar()
//!   └─ llama Plugin::inicializar() en cada plugin
//!       └─ AccionPlugin::EstablecerTema → actualiza tema_activo
//! HostPlugins::color("keyword")    →  [r, g, b]
//! ```

mod lua;

use std::collections::HashMap;

use glyph_plugin_api::{AccionPlugin, ContextoPlugin, Plugin};

use crate::lua::PluginLua;

// ------------------------------------------------------------------
// Tema por defecto (One Dark) — activo hasta que un plugin lo reemplace
// ------------------------------------------------------------------

fn tema_por_defecto() -> HashMap<String, [u8; 3]> {
    [
        ("keyword",     [0xC6, 0x78, 0xDD]),
        ("string",      [0x98, 0xC3, 0x79]),
        ("comment",     [0x5C, 0x63, 0x70]),
        ("function",    [0x61, 0xAF, 0xEF]),
        ("type",        [0xE5, 0xC0, 0x7B]),
        ("number",      [0xD1, 0x9A, 0x66]),
        ("operator",    [0x56, 0xB6, 0xC2]),
        ("variable",    [0xE0, 0x6C, 0x75]),
        ("constant",    [0xD1, 0x9A, 0x66]),
        ("punctuation", [0xAB, 0xB2, 0xBF]),
        ("attribute",   [0xE5, 0xC0, 0x7B]),
        ("default",     [0xAB, 0xB2, 0xBF]),
    ]
    .into_iter()
    .map(|(k, v)| (k.to_string(), v))
    .collect()
}

// ------------------------------------------------------------------
// HostPlugins
// ------------------------------------------------------------------

/// Gestiona el ciclo de vida de todos los plugins cargados.
pub struct HostPlugins {
    plugins: Vec<Box<dyn Plugin>>,
    tema_activo: HashMap<String, [u8; 3]>,
}

impl HostPlugins {
    pub fn nuevo() -> Self {
        Self {
            plugins: Vec::new(),
            tema_activo: tema_por_defecto(),
        }
    }

    /// Carga un plugin Lua desde su código fuente.
    pub fn cargar_lua(&mut self, nombre: &str, script: &str) -> anyhow::Result<()> {
        let plugin = PluginLua::desde_str(nombre, script)?;
        self.plugins.push(Box::new(plugin));
        tracing::info!("Plugin cargado: '{nombre}'");
        Ok(())
    }

    /// Inicializa todos los plugins registrados y aplica sus acciones.
    /// Debe llamarse una vez después de cargar todos los plugins.
    pub fn inicializar(&mut self) {
        let acciones: Vec<AccionPlugin> = self
            .plugins
            .iter_mut()
            .flat_map(|p| p.inicializar())
            .collect();

        for accion in acciones {
            self.aplicar(accion);
        }
    }

    /// Dispara el hook `al_abrir` en todos los plugins.
    pub fn al_abrir(&mut self, ruta: Option<&str>) {
        let ctx = ContextoPlugin {
            ruta: ruta.map(|s| s.to_string()),
            version_doc: 0,
        };
        let acciones: Vec<AccionPlugin> = self
            .plugins
            .iter_mut()
            .flat_map(|p| p.al_abrir(&ctx))
            .collect();
        for a in acciones {
            self.aplicar(a);
        }
    }

    /// Dispara el hook `al_cambiar` en todos los plugins.
    pub fn al_cambiar(&mut self, ruta: Option<&str>, version: u32) {
        let ctx = ContextoPlugin {
            ruta: ruta.map(|s| s.to_string()),
            version_doc: version,
        };
        let acciones: Vec<AccionPlugin> = self
            .plugins
            .iter_mut()
            .flat_map(|p| p.al_cambiar(&ctx))
            .collect();
        for a in acciones {
            self.aplicar(a);
        }
    }

    /// Dispara el hook `al_guardar` en todos los plugins.
    pub fn al_guardar(&mut self, ruta: Option<&str>) {
        let ctx = ContextoPlugin {
            ruta: ruta.map(|s| s.to_string()),
            version_doc: 0,
        };
        let acciones: Vec<AccionPlugin> = self
            .plugins
            .iter_mut()
            .flat_map(|p| p.al_guardar(&ctx))
            .collect();
        for a in acciones {
            self.aplicar(a);
        }
    }

    /// Devuelve el color RGB del tema activo para una clave semántica.
    ///
    /// Claves válidas: `"keyword"`, `"string"`, `"comment"`, `"function"`,
    /// `"type"`, `"number"`, `"operator"`, `"variable"`, `"constant"`,
    /// `"punctuation"`, `"attribute"`, `"default"`.
    pub fn color(&self, clave: &str) -> [u8; 3] {
        self.tema_activo
            .get(clave)
            .copied()
            .unwrap_or([0xAB, 0xB2, 0xBF])
    }

    // ── Privados ─────────────────────────────────────────────────────

    fn aplicar(&mut self, accion: AccionPlugin) {
        match accion {
            AccionPlugin::EstablecerTema(tema) => {
                tracing::info!("Tema activo actualizado ({} colores)", tema.len());
                self.tema_activo = tema;
            }
            AccionPlugin::LogMensaje(msg) => {
                tracing::info!("[plugin] {msg}");
            }
        }
    }
}

impl Default for HostPlugins {
    fn default() -> Self {
        Self::nuevo()
    }
}
