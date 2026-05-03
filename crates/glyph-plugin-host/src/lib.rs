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
//!   └─ cargar_lua(nombre, script)
//!       ├─ Lee M.permisos del script
//!       ├─ Aplica sandbox Lua (elimina APIs no declaradas)
//!       └─ Registra plugin + permisos en el host
//! HostPlugins::inicializar()
//!   └─ llama Plugin::inicializar() en cada plugin
//!       └─ AccionPlugin::EstablecerTema → comprueba permiso 'ui' → actualiza tema
//! HostPlugins::color("keyword")    →  [r, g, b]
//! ```

mod lua;
mod wasm;

use std::collections::HashMap;
use std::path::Path;

use glyph_plugin_api::{AccionPlugin, ContextoPlugin, Permisos, Plugin};

use crate::lua::PluginLua;
use crate::wasm::PluginWasm;

// ------------------------------------------------------------------
// Tema por defecto (One Dark) — activo hasta que un plugin lo reemplace
// ------------------------------------------------------------------

fn tema_por_defecto() -> HashMap<String, [u8; 3]> {
    [
        ("keyword",     [0xF0, 0xAA, 0xAC]),  // rosa-coral    — wallbash
        ("string",      [0xCC, 0xDD, 0xFF]),  // azul claro
        ("comment",     [0x7A, 0x8C, 0xB4]),  // azul medio (~4:1 sobre #1E1E2E)
        ("function",    [0xAF, 0xAA, 0xF0]),  // lavanda
        ("type",        [0x9A, 0xD0, 0xE6]),  // cian suave
        ("number",      [0xAA, 0xDC, 0xF0]),  // cian claro
        ("operator",    [0xAA, 0xC1, 0xF0]),  // azul medio
        ("variable",    [0xFF, 0xFF, 0xFF]),  // blanco
        ("constant",    [0xAA, 0xDC, 0xF0]),  // cian claro
        ("punctuation", [0x7A, 0x92, 0xC2]),  // azul-gris
        ("attribute",   [0xAA, 0xDC, 0xF0]),  // cian claro
        ("default",     [0xFF, 0xFF, 0xFF]),  // blanco
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
    /// Permisos por nombre de plugin — se leen al cargar, antes del sandbox.
    permisos: HashMap<String, Permisos>,
}

impl HostPlugins {
    pub fn nuevo() -> Self {
        Self {
            plugins: Vec::new(),
            tema_activo: tema_por_defecto(),
            permisos: HashMap::new(),
        }
    }

    /// Carga un plugin Lua, aplica sandbox según sus permisos declarados y lo registra.
    pub fn cargar_lua(&mut self, nombre: &str, script: &str) -> anyhow::Result<()> {
        let plugin = PluginLua::desde_str(nombre, script)?;
        let perms = plugin.permisos();
        self.permisos.insert(nombre.to_string(), perms);
        self.plugins.push(Box::new(plugin));
        tracing::info!("Plugin cargado: '{nombre}'");
        Ok(())
    }

    /// Carga un plugin WASM desde un archivo `.wasm` y lo registra.
    pub fn cargar_wasm(&mut self, ruta: &Path) -> anyhow::Result<()> {
        let plugin = PluginWasm::desde_archivo(ruta)?;
        let nombre = plugin.nombre().to_string();
        let perms = plugin.permisos();
        self.permisos.insert(nombre.clone(), perms);
        self.plugins.push(Box::new(plugin));
        tracing::info!("Plugin WASM cargado: '{nombre}'");
        Ok(())
    }

    /// Carga un plugin WASM desde bytes en memoria y lo registra.
    pub fn cargar_wasm_bytes(&mut self, nombre: &str, bytes: &[u8]) -> anyhow::Result<()> {
        let plugin = PluginWasm::desde_bytes(nombre, bytes)?;
        let perms = plugin.permisos();
        self.permisos.insert(nombre.to_string(), perms);
        self.plugins.push(Box::new(plugin));
        tracing::info!("Plugin WASM cargado: '{nombre}'");
        Ok(())
    }

    /// Inicializa todos los plugins registrados y aplica sus acciones.
    pub fn inicializar(&mut self) {
        let acciones = self.recoger_acciones(|p| p.inicializar());
        for (nombre, accion) in acciones {
            self.aplicar(accion, &nombre);
        }
    }

    /// Dispara el hook `al_abrir` en todos los plugins.
    pub fn al_abrir(&mut self, ruta: Option<&str>) {
        let ctx = ContextoPlugin {
            ruta: ruta.map(|s| s.to_string()),
            version_doc: 0,
        };
        let acciones = self.recoger_acciones(|p| p.al_abrir(&ctx));
        for (nombre, accion) in acciones {
            self.aplicar(accion, &nombre);
        }
    }

    /// Dispara el hook `al_cambiar` en todos los plugins.
    pub fn al_cambiar(&mut self, ruta: Option<&str>, version: u32) {
        let ctx = ContextoPlugin {
            ruta: ruta.map(|s| s.to_string()),
            version_doc: version,
        };
        let acciones = self.recoger_acciones(|p| p.al_cambiar(&ctx));
        for (nombre, accion) in acciones {
            self.aplicar(accion, &nombre);
        }
    }

    /// Dispara el hook `al_guardar` en todos los plugins.
    pub fn al_guardar(&mut self, ruta: Option<&str>) {
        let ctx = ContextoPlugin {
            ruta: ruta.map(|s| s.to_string()),
            version_doc: 0,
        };
        let acciones = self.recoger_acciones(|p| p.al_guardar(&ctx));
        for (nombre, accion) in acciones {
            self.aplicar(accion, &nombre);
        }
    }

    /// Devuelve el color RGB del tema activo para una clave semántica.
    pub fn color(&self, clave: &str) -> [u8; 3] {
        self.tema_activo
            .get(clave)
            .copied()
            .unwrap_or([0xAB, 0xB2, 0xBF])
    }

    // ── Privados ─────────────────────────────────────────────────────

    /// Recoge `(nombre_plugin, accion)` de todos los plugins para un hook dado.
    fn recoger_acciones<F>(&mut self, mut hook: F) -> Vec<(String, AccionPlugin)>
    where
        F: FnMut(&mut Box<dyn Plugin>) -> Vec<AccionPlugin>,
    {
        self.plugins
            .iter_mut()
            .flat_map(|p| {
                let nombre = p.nombre().to_string();
                hook(p).into_iter().map(move |a| (nombre.clone(), a))
            })
            .collect()
    }

    /// Aplica una acción, verificando que el plugin tenga los permisos necesarios.
    fn aplicar(&mut self, accion: AccionPlugin, nombre_plugin: &str) {
        match accion {
            AccionPlugin::EstablecerTema(tema) => {
                let tiene_ui = self
                    .permisos
                    .get(nombre_plugin)
                    .map(|p| p.ui)
                    .unwrap_or(false);

                if !tiene_ui {
                    tracing::warn!(
                        "[plugin '{nombre_plugin}'] rechazado: EstablecerTema requiere permiso 'ui'"
                    );
                    return;
                }

                tracing::info!(
                    "[plugin '{nombre_plugin}'] tema aplicado ({} colores)",
                    tema.len()
                );
                self.tema_activo = tema;
            }
            AccionPlugin::LogMensaje(msg) => {
                tracing::info!("[plugin '{nombre_plugin}'] {msg}");
            }
        }
    }
}

impl Default for HostPlugins {
    fn default() -> Self {
        Self::nuevo()
    }
}
