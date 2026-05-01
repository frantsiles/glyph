// Copyright 2026 Franz (frantsiles)
// Licensed under the Apache License, Version 2.0

//! Adaptador Lua → `Plugin`.
//!
//! Un script Lua válido debe retornar una tabla con al menos `nombre`.
//! Hooks opcionales: `inicializar()`, `al_cambiar(ruta)`, `al_guardar(ruta)`,
//! `al_abrir(ruta)`. El hook especial `tema()` retorna una tabla de colores.
//!
//! ## Ejemplo de script Lua
//!
//! ```lua
//! local M = {}
//! M.nombre = "mi-plugin"
//!
//! function M.tema()
//!     return { keyword = "#C678DD", string = "#98C379" }
//! end
//!
//! function M.al_guardar(ruta)
//!     -- hacer algo al guardar
//! end
//!
//! return M
//! ```

use std::collections::HashMap;

use mlua::{Function, Lua, RegistryKey, Table};

use glyph_plugin_api::{AccionPlugin, ContextoPlugin, Plugin};

pub(crate) struct PluginLua {
    lua: Lua,
    nombre: String,
    key_modulo: RegistryKey,
}

impl PluginLua {
    /// Carga un script Lua desde un string y lo inicializa.
    pub fn desde_str(nombre: &str, script: &str) -> anyhow::Result<Self> {
        let lua = Lua::new();
        let tabla: Table<'_> = lua
            .load(script)
            .eval()
            .map_err(|e| anyhow::anyhow!("Error cargando plugin '{nombre}': {e}"))?;

        let key_modulo = lua
            .create_registry_value(tabla)
            .map_err(|e| anyhow::anyhow!("Error registrando módulo Lua: {e}"))?;

        Ok(Self {
            lua,
            nombre: nombre.to_string(),
            key_modulo,
        })
    }

    /// Intenta llamar un hook de la forma `function hook_name(ruta)`.
    fn llamar_hook(&self, nombre_fn: &str, ctx: &ContextoPlugin) -> Vec<AccionPlugin> {
        let Ok(tabla) = self.lua.registry_value::<Table<'_>>(&self.key_modulo) else {
            return vec![];
        };
        let func: Function<'_> = match tabla.get(nombre_fn) {
            Ok(f) => f,
            Err(_) => return vec![],
        };
        let ruta = ctx.ruta.clone().unwrap_or_default();
        if let Err(e) = func.call::<(std::string::String,), ()>((ruta,)) {
            tracing::warn!("[plugin '{}'] error en {nombre_fn}: {e}", self.nombre);
        }
        vec![]
    }

    /// Llama a `M.tema()` y convierte la tabla Lua en un mapa de colores RGB.
    fn cargar_tema(&self) -> Option<HashMap<std::string::String, [u8; 3]>> {
        let tabla: Table<'_> = self.lua.registry_value(&self.key_modulo).ok()?;
        let func: Function<'_> = tabla.get("tema").ok()?;
        let colores: Table<'_> = func.call(()).ok()?;

        let mut mapa: HashMap<std::string::String, [u8; 3]> = HashMap::new();
        for par in colores.pairs::<std::string::String, std::string::String>() {
            if let Ok((clave, valor)) = par {
                if let Some(rgb) = hex_a_rgb(&valor) {
                    mapa.insert(clave, rgb);
                }
            }
        }
        if mapa.is_empty() { None } else { Some(mapa) }
    }
}

impl Plugin for PluginLua {
    fn nombre(&self) -> &str {
        &self.nombre
    }

    fn inicializar(&mut self) -> Vec<AccionPlugin> {
        let mut acciones = Vec::new();

        // Intentar cargar tema si el plugin lo provee
        if let Some(tema) = self.cargar_tema() {
            tracing::info!("[plugin '{}'] tema cargado ({} entradas)", self.nombre, tema.len());
            acciones.push(AccionPlugin::EstablecerTema(tema));
        }

        // Llamar hook `inicializar()` si existe
        let ctx = ContextoPlugin { ruta: None, version_doc: 0 };
        let Ok(tabla) = self.lua.registry_value::<Table<'_>>(&self.key_modulo) else {
            return acciones;
        };
        let func_opt: Option<Function<'_>> = tabla.get("inicializar").ok();
        if let Some(func) = func_opt {
            if let Err(e) = func.call::<(), ()>(()) {
                tracing::warn!("[plugin '{}'] error en inicializar: {e}", self.nombre);
            }
        }
        drop(ctx);

        acciones
    }

    fn al_cambiar(&mut self, ctx: &ContextoPlugin) -> Vec<AccionPlugin> {
        self.llamar_hook("al_cambiar", ctx)
    }

    fn al_guardar(&mut self, ctx: &ContextoPlugin) -> Vec<AccionPlugin> {
        self.llamar_hook("al_guardar", ctx)
    }

    fn al_abrir(&mut self, ctx: &ContextoPlugin) -> Vec<AccionPlugin> {
        self.llamar_hook("al_abrir", ctx)
    }
}

// ------------------------------------------------------------------
// Helpers
// ------------------------------------------------------------------

/// Convierte `"#RRGGBB"` a `[r, g, b]`. Retorna `None` si el formato es inválido.
fn hex_a_rgb(hex: &str) -> Option<[u8; 3]> {
    let hex = hex.trim_start_matches('#');
    if hex.len() != 6 {
        return None;
    }
    Some([
        u8::from_str_radix(&hex[0..2], 16).ok()?,
        u8::from_str_radix(&hex[2..4], 16).ok()?,
        u8::from_str_radix(&hex[4..6], 16).ok()?,
    ])
}
