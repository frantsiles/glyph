// Copyright 2026 Franz (frantsiles)
// Licensed under the Apache License, Version 2.0

//! Adaptador Lua → `Plugin`.
//!
//! Un script Lua válido debe retornar una tabla con al menos `nombre` y
//! `permisos`. Hooks opcionales: `inicializar()`, `al_cambiar(ruta)`,
//! `al_guardar(ruta)`, `al_abrir(ruta)`. El hook especial `tema()` retorna
//! una tabla de colores.
//!
//! ## Permisos y sandbox
//!
//! Al cargar el script, el host lee `M.permisos` y aplica un sandbox
//! que elimina las APIs peligrosas no declaradas:
//!
//! - Sin `leer_archivos` ni `escribir_archivos`: se elimina `io`, `dofile`, `loadfile`
//! - Sin `ejecutar_procesos`: `os` queda reducido a funciones de tiempo seguras
//! - Sin `leer_archivos`: se elimina `require` y `load`
//!
//! ## Ejemplo de script Lua
//!
//! ```lua
//! local M = {}
//! M.nombre   = "mi-plugin"
//! M.permisos = { ui = true }
//!
//! function M.tema()
//!     return { keyword = "#C678DD", string = "#98C379" }
//! end
//!
//! return M
//! ```

use std::collections::HashMap;

use mlua::{Function, Lua, RegistryKey, Table, Value};

use glyph_plugin_api::{AccionPlugin, ContextoPlugin, Permisos, Plugin};

pub(crate) struct PluginLua {
    lua: Lua,
    nombre: String,
    key_modulo: RegistryKey,
    permisos: Permisos,
}

impl PluginLua {
    /// Carga un script Lua, lee sus permisos declarados y aplica el sandbox.
    pub fn desde_str(nombre: &str, script: &str) -> anyhow::Result<Self> {
        let lua = Lua::new();

        let tabla: Table<'_> = lua
            .load(script)
            .eval()
            .map_err(|e| anyhow::anyhow!("Error cargando plugin '{nombre}': {e}"))?;

        // 1. Leer permisos antes de sandboxear
        let permisos = leer_permisos_lua(&tabla);
        tracing::info!(
            "[plugin '{nombre}'] permisos declarados: {}",
            permisos.resumen()
        );

        // 2. Aplicar sandbox: eliminar APIs no declaradas
        aplicar_sandbox(&lua, &permisos)?;

        let key_modulo = lua
            .create_registry_value(tabla)
            .map_err(|e| anyhow::anyhow!("Error registrando módulo Lua: {e}"))?;

        Ok(Self {
            lua,
            nombre: nombre.to_string(),
            key_modulo,
            permisos,
        })
    }

    /// Llama un hook de la forma `function hook_name(ruta)`.
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

    fn permisos(&self) -> Permisos {
        self.permisos.clone()
    }

    fn inicializar(&mut self) -> Vec<AccionPlugin> {
        let mut acciones = Vec::new();

        if let Some(tema) = self.cargar_tema() {
            tracing::info!(
                "[plugin '{}'] tema cargado ({} entradas)",
                self.nombre,
                tema.len()
            );
            acciones.push(AccionPlugin::EstablecerTema(tema));
        }

        let Ok(tabla) = self.lua.registry_value::<Table<'_>>(&self.key_modulo) else {
            return acciones;
        };
        let func_opt: Option<Function<'_>> = tabla.get("inicializar").ok();
        if let Some(func) = func_opt {
            if let Err(e) = func.call::<(), ()>(()) {
                tracing::warn!("[plugin '{}'] error en inicializar: {e}", self.nombre);
            }
        }

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
// Sandbox
// ------------------------------------------------------------------

/// Lee la tabla `M.permisos` del módulo Lua. Si no existe, retorna los permisos por defecto.
fn leer_permisos_lua(tabla: &Table) -> Permisos {
    let Ok(t) = tabla.get::<_, Table>("permisos") else {
        return Permisos::default();
    };
    Permisos {
        ui:                 t.get("ui").unwrap_or(true),
        leer_archivos:      t.get("leer_archivos").unwrap_or(false),
        escribir_archivos:  t.get("escribir_archivos").unwrap_or(false),
        ejecutar_procesos:  t.get("ejecutar_procesos").unwrap_or(false),
        red:                t.get("red").unwrap_or(false),
    }
}

/// Aplica restricciones al entorno Lua global basadas en los permisos declarados.
///
/// Limitación conocida: funciones capturadas en closures antes del sandbox
/// no se ven afectadas. Para un sandbox hermético se usará wasmtime en M4.
fn aplicar_sandbox(lua: &Lua, permisos: &Permisos) -> anyhow::Result<()> {
    let globals = lua.globals();

    // Sin acceso a archivos: eliminar io y funciones de carga de ficheros
    if !permisos.leer_archivos && !permisos.escribir_archivos {
        globals.set("io", Value::Nil)?;
        globals.set("dofile", Value::Nil)?;
        globals.set("loadfile", Value::Nil)?;
    }

    // Sin carga dinámica de código (require, load) salvo con leer_archivos
    if !permisos.leer_archivos {
        globals.set("require", Value::Nil)?;
        globals.set("load", Value::Nil)?;
    }

    // Sin ejecución de procesos: reducir os a funciones de tiempo seguras
    if !permisos.ejecutar_procesos {
        if let Ok(os_completo) = globals.get::<_, Table>("os") {
            let os_seguro = lua.create_table()?;
            for fn_segura in &["time", "date", "clock", "difftime"] {
                if let Ok(f) = os_completo.get::<_, Value>(*fn_segura) {
                    os_seguro.set(*fn_segura, f)?;
                }
            }
            globals.set("os", os_seguro)?;
        }
    }

    Ok(())
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
