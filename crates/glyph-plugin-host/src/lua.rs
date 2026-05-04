// Copyright 2026 Franz (frantsiles)
// Licensed under the Apache License, Version 2.0

//! Adaptador Lua → `Plugin`.
//!
//! ## Global `glyph`
//!
//! El host inyecta una tabla `glyph` en el estado Lua con:
//!
//! ```lua
//! glyph.registrar_seccion({ id, lado, tamano, color_fondo })
//! glyph.abrir_archivo(ruta)            -- requiere permiso leer_archivos
//! glyph.leer_directorio(ruta)          -- requiere permiso leer_archivos
//!   → { {nombre, ruta, es_dir}, ... }  (dirs primero, luego archivos)
//! ```
//!
//! Las acciones generadas por estas funciones se acumulan en `_glyph_pending`
//! y se drenan después de cada llamada a un hook.
//!
//! ## Hooks del script Lua
//!
//! ```lua
//! M.nombre   = "mi-plugin"
//! M.permisos = { ui = true, leer_archivos = true }
//!
//! function M.tema() return { keyword = "#C678DD" } end
//! function M.inicializar()  ... end
//! function M.al_abrir(ruta) ... end
//! function M.al_cambiar(ruta) ... end
//! function M.al_guardar(ruta) ... end
//! function M.renderizar_seccion(id) ... end  -- retorna tabla de líneas
//! function M.click_seccion(id, linea) ... end
//! ```

use std::collections::HashMap;

use mlua::{Function, Lua, RegistryKey, Table, Value};

use glyph_plugin_api::{
    AccionPlugin, ContextoPlugin, LineaSeccion, NivelNotificacion, Permisos, Plugin, SeccionConfig,
};

pub(crate) struct PluginLua {
    lua: Lua,
    nombre: String,
    key_modulo: RegistryKey,
    permisos: Permisos,
}

impl PluginLua {
    pub fn desde_str(nombre: &str, script: &str) -> anyhow::Result<Self> {
        let lua = Lua::new();

        // 1. Inyectar tabla _glyph_pending y global glyph antes de cargar el script
        inyectar_global_glyph(&lua)?;

        // 2. Cargar el script
        let tabla: Table<'_> = lua
            .load(script)
            .eval()
            .map_err(|e| anyhow::anyhow!("Error cargando plugin '{nombre}': {e}"))?;

        // 3. Leer permisos del módulo
        let permisos = leer_permisos_lua(&tabla);
        tracing::info!("[plugin '{nombre}'] permisos: {}", permisos.resumen());

        // 4. Sandbox según permisos declarados
        aplicar_sandbox(&lua, &permisos)?;

        let key_modulo = lua
            .create_registry_value(tabla)
            .map_err(|e| anyhow::anyhow!("Error registrando módulo: {e}"))?;

        Ok(Self { lua, nombre: nombre.to_string(), key_modulo, permisos })
    }

    /// Llama `M.hook_name(arg)` y retorna los AccionPlugin generados.
    fn llamar_hook_str(&self, nombre_fn: &str, arg: &str) -> Vec<AccionPlugin> {
        let Ok(tabla) = self.lua.registry_value::<Table<'_>>(&self.key_modulo) else {
            return vec![];
        };
        let func: Function<'_> = match tabla.get(nombre_fn) {
            Ok(f) => f,
            Err(_) => return vec![],
        };
        // Limpiar pending antes del hook
        if let Ok(globals) = self.lua.globals().get::<_, Table>("_glyph_pending") {
            let _ = globals; // solo acceder para verificar existencia
        }
        reset_pending(&self.lua);

        if let Err(e) = func.call::<_, ()>((arg.to_string(),)) {
            tracing::warn!("[plugin '{}'] error en {nombre_fn}: {e}", self.nombre);
        }
        drenar_pending(&self.lua, &self.nombre)
    }

    fn cargar_tema(&self) -> Option<HashMap<String, [u8; 3]>> {
        let tabla: Table<'_> = self.lua.registry_value(&self.key_modulo).ok()?;
        let func: Function<'_> = tabla.get("tema").ok()?;
        let colores: Table<'_> = func.call(()).ok()?;
        let mut mapa: HashMap<String, [u8; 3]> = HashMap::new();
        for par in colores.pairs::<String, String>() {
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
    fn nombre(&self) -> &str { &self.nombre }

    fn permisos(&self) -> Permisos { self.permisos.clone() }

    fn inicializar(&mut self) -> Vec<AccionPlugin> {
        let mut acciones = Vec::new();

        if let Some(tema) = self.cargar_tema() {
            tracing::info!("[plugin '{}'] tema cargado ({} entradas)", self.nombre, tema.len());
            acciones.push(AccionPlugin::EstablecerTema(tema));
        }

        let Ok(tabla) = self.lua.registry_value::<Table<'_>>(&self.key_modulo) else {
            return acciones;
        };

        reset_pending(&self.lua);

        let func_opt: Option<Function<'_>> = tabla.get("inicializar").ok();
        if let Some(func) = func_opt {
            if let Err(e) = func.call::<(), ()>(()) {
                tracing::warn!("[plugin '{}'] error en inicializar: {e}", self.nombre);
            }
        }

        acciones.extend(drenar_pending(&self.lua, &self.nombre));
        acciones
    }

    fn al_cambiar(&mut self, ctx: &ContextoPlugin) -> Vec<AccionPlugin> {
        self.llamar_hook_str("al_cambiar", ctx.ruta.as_deref().unwrap_or(""))
    }

    fn al_guardar(&mut self, ctx: &ContextoPlugin) -> Vec<AccionPlugin> {
        self.llamar_hook_str("al_guardar", ctx.ruta.as_deref().unwrap_or(""))
    }

    fn al_abrir(&mut self, ctx: &ContextoPlugin) -> Vec<AccionPlugin> {
        self.llamar_hook_str("al_abrir", ctx.ruta.as_deref().unwrap_or(""))
    }

    fn click_seccion(&mut self, id_seccion: &str, linea: u32) -> Vec<AccionPlugin> {
        let Ok(tabla) = self.lua.registry_value::<Table<'_>>(&self.key_modulo) else {
            return vec![];
        };
        let func: Function<'_> = match tabla.get("click_seccion") {
            Ok(f) => f,
            Err(_) => return vec![],
        };
        reset_pending(&self.lua);
        if let Err(e) = func.call::<_, ()>((id_seccion.to_string(), linea)) {
            tracing::warn!("[plugin '{}'] error en click_seccion: {e}", self.nombre);
        }
        drenar_pending(&self.lua, &self.nombre)
    }

    fn tecla_seccion(&mut self, id_seccion: &str, tecla: &str, modifiers: &str) -> Vec<AccionPlugin> {
        let Ok(tabla) = self.lua.registry_value::<Table<'_>>(&self.key_modulo) else {
            return vec![];
        };
        let func: Function<'_> = match tabla.get("tecla_seccion") {
            Ok(f) => f,
            Err(_) => return vec![],
        };
        reset_pending(&self.lua);
        if let Err(e) = func.call::<_, ()>((id_seccion.to_string(), tecla.to_string(), modifiers.to_string())) {
            tracing::warn!("[plugin '{}'] error en tecla_seccion: {e}", self.nombre);
        }
        drenar_pending(&self.lua, &self.nombre)
    }
}

// ------------------------------------------------------------------
// Inyección del global `glyph`
// ------------------------------------------------------------------

fn inyectar_global_glyph(lua: &Lua) -> anyhow::Result<()> {
    let globals = lua.globals();

    // _glyph_pending: tabla donde se acumulan acciones pendientes
    let pending = lua.create_table()?;
    globals.set("_glyph_pending", pending)?;

    let glyph = lua.create_table()?;

    // glyph.registrar_seccion(config)
    let reg_fn = lua.create_function(|lua_ctx, config: Table| {
        let pending: Table = lua_ctx.globals().get("_glyph_pending")?;
        let accion = lua_ctx.create_table()?;
        accion.set("tipo", "registrar_seccion")?;
        accion.set("id", config.get::<_, String>("id").unwrap_or_default())?;
        accion.set("lado", config.get::<_, String>("lado").unwrap_or_else(|_| "izquierda".into()))?;
        accion.set("tamano", config.get::<_, f32>("tamano").unwrap_or(240.0))?;
        if let Ok(color) = config.get::<_, String>("color_fondo") {
            accion.set("color_fondo", color)?;
        }
        pending.push(accion)?;
        Ok(())
    })?;
    glyph.set("registrar_seccion", reg_fn)?;

    // glyph.abrir_archivo(ruta)
    let abrir_fn = lua.create_function(|lua_ctx, ruta: String| {
        let pending: Table = lua_ctx.globals().get("_glyph_pending")?;
        let accion = lua_ctx.create_table()?;
        accion.set("tipo", "abrir_archivo")?;
        accion.set("ruta", ruta)?;
        pending.push(accion)?;
        Ok(())
    })?;
    glyph.set("abrir_archivo", abrir_fn)?;

    // glyph.actualizar_seccion(id, lineas)
    let actualizar_fn = lua.create_function(|lua_ctx, (id, lineas): (String, Table)| {
        let pending: Table = lua_ctx.globals().get("_glyph_pending")?;
        let accion = lua_ctx.create_table()?;
        accion.set("tipo", "actualizar_seccion")?;
        accion.set("id", id)?;
        accion.set("lineas", lineas)?;
        pending.push(accion)?;
        Ok(())
    })?;
    glyph.set("actualizar_seccion", actualizar_fn)?;

    // glyph.leer_directorio(ruta) → tabla de entradas {nombre, ruta, es_dir}
    let leer_dir_fn = lua.create_function(|lua_ctx, ruta: String| {
        let resultado = lua_ctx.create_table()?;
        let path = std::path::Path::new(&ruta);
        if !path.is_dir() {
            return Ok(resultado);
        }
        let mut entradas: Vec<(bool, String, String)> = Vec::new();
        if let Ok(iter) = std::fs::read_dir(path) {
            for entry in iter.flatten() {
                let es_dir = entry.path().is_dir();
                let nombre = entry.file_name().to_string_lossy().into_owned();
                if nombre.starts_with('.') { continue; }
                let ruta_completa = entry.path().to_string_lossy().into_owned();
                entradas.push((es_dir, nombre, ruta_completa));
            }
        }
        // Dirs primero, luego archivos; ambos grupos ordenados alfabéticamente
        entradas.sort_by(|a, b| {
            match (a.0, b.0) {
                (true, false) => std::cmp::Ordering::Less,
                (false, true) => std::cmp::Ordering::Greater,
                _ => a.1.to_lowercase().cmp(&b.1.to_lowercase()),
            }
        });
        for (i, (es_dir, nombre, ruta_e)) in entradas.into_iter().enumerate() {
            let entrada = lua_ctx.create_table()?;
            entrada.set("nombre", nombre)?;
            entrada.set("ruta", ruta_e)?;
            entrada.set("es_dir", es_dir)?;
            resultado.set(i + 1, entrada)?;
        }
        Ok(resultado)
    })?;
    glyph.set("leer_directorio", leer_dir_fn)?;

    // glyph.toggle_preview_md()
    let toggle_md_fn = lua.create_function(|lua_ctx, ()| {
        let pending: Table = lua_ctx.globals().get("_glyph_pending")?;
        let accion = lua_ctx.create_table()?;
        accion.set("tipo", "toggle_preview_md")?;
        pending.push(accion)?;
        Ok(())
    })?;
    glyph.set("toggle_preview_md", toggle_md_fn)?;

    // glyph.mostrar_notificacion(mensaje, nivel)
    let notificar_fn = lua.create_function(|lua_ctx, (mensaje, nivel): (String, String)| {
        let pending: Table = lua_ctx.globals().get("_glyph_pending")?;
        let accion = lua_ctx.create_table()?;
        accion.set("tipo", "mostrar_notificacion")?;
        accion.set("mensaje", mensaje)?;
        accion.set("nivel", nivel)?;
        pending.push(accion)?;
        Ok(())
    })?;
    glyph.set("mostrar_notificacion", notificar_fn)?;

    globals.set("glyph", glyph)?;
    Ok(())
}

// ------------------------------------------------------------------
// Drenaje de `_glyph_pending` → Vec<AccionPlugin>
// ------------------------------------------------------------------

fn reset_pending(lua: &Lua) {
    if let Ok(globals) = lua.globals().get::<_, Table>("_glyph_pending") {
        let _ = globals; // tabla ya existe, limpiarla
    }
    let _ = lua.globals().set("_glyph_pending", lua.create_table().unwrap_or_else(|_| {
        lua.globals().get::<_, Table>("_glyph_pending").unwrap()
    }));
}

fn drenar_pending(lua: &Lua, nombre_plugin: &str) -> Vec<AccionPlugin> {
    let globals = lua.globals();
    let pending: Table = match globals.get("_glyph_pending") {
        Ok(t) => t,
        Err(_) => return vec![],
    };
    let mut acciones = Vec::new();
    let len = pending.raw_len();
    for i in 1..=len {
        let item: Table = match pending.get(i) {
            Ok(t) => t,
            Err(_) => continue,
        };
        let tipo: String = match item.get("tipo") {
            Ok(t) => t,
            Err(_) => continue,
        };
        match tipo.as_str() {
            "registrar_seccion" => {
                let id: String = item.get("id").unwrap_or_default();
                let lado: String = item.get("lado").unwrap_or_else(|_| "izquierda".into());
                let tamano: f32 = item.get("tamano").unwrap_or(240.0);
                let color_fondo: Option<[u8; 3]> = item.get::<_, String>("color_fondo").ok()
                    .and_then(|s| hex_a_rgb(&s));
                acciones.push(AccionPlugin::RegistrarSeccion(SeccionConfig {
                    id, lado, tamano, color_fondo,
                }));
            }
            "abrir_archivo" => {
                let ruta: String = item.get("ruta").unwrap_or_default();
                if !ruta.is_empty() {
                    acciones.push(AccionPlugin::AbrirArchivo(ruta));
                }
            }
            "actualizar_seccion" => {
                let id: String = item.get("id").unwrap_or_default();
                let lineas_tabla: Table = match item.get("lineas") {
                    Ok(t) => t,
                    Err(_) => continue,
                };
                let lineas = tabla_a_lineas(lineas_tabla, nombre_plugin);
                acciones.push(AccionPlugin::ActualizarContenidoSeccion { id, lineas });
            }
            "mostrar_notificacion" => {
                let mensaje: String = item.get("mensaje").unwrap_or_default();
                let nivel_str: String = item.get("nivel").unwrap_or_else(|_| "info".into());
                let nivel = match nivel_str.to_lowercase().as_str() {
                    "aviso" | "warning" => NivelNotificacion::Aviso,
                    "error" => NivelNotificacion::Error,
                    _ => NivelNotificacion::Info,
                };
                acciones.push(AccionPlugin::MostrarNotificacion { mensaje, nivel });
            }
            "toggle_preview_md" => {
                acciones.push(AccionPlugin::ToggleVistaPreviaMd);
            }
            _ => {
                tracing::warn!("[plugin '{nombre_plugin}'] acción Lua desconocida: '{tipo}'");
            }
        }
    }
    acciones
}

fn tabla_a_lineas(tabla: Table, nombre_plugin: &str) -> Vec<LineaSeccion> {
    let mut lineas = Vec::new();
    let len = tabla.raw_len();
    for i in 1..=len {
        match tabla.get::<_, Value>(i) {
            Ok(Value::Table(t)) => {
                let texto: String = t.get("texto").unwrap_or_default();
                let color: Option<[u8; 3]> = t.get::<_, String>("color").ok()
                    .and_then(|s| hex_a_rgb(&s));
                let negrita: bool = t.get("negrita").unwrap_or(false);
                let fondo: Option<[u8; 3]> = t.get::<_, String>("fondo").ok()
                    .and_then(|s| hex_a_rgb(&s));
                let payload: Option<Vec<u8>> = t.get::<_, String>("payload").ok()
                    .map(|s| s.into_bytes());
                lineas.push(LineaSeccion { texto, color, negrita, fondo, payload });
            }
            Ok(Value::String(s)) => {
                lineas.push(LineaSeccion::simple(s.to_str().unwrap_or("")));
            }
            _ => {
                tracing::warn!("[plugin '{nombre_plugin}'] línea de sección inválida en índice {i}");
            }
        }
    }
    lineas
}

// ------------------------------------------------------------------
// Sandbox
// ------------------------------------------------------------------

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

fn aplicar_sandbox(lua: &Lua, permisos: &Permisos) -> anyhow::Result<()> {
    let globals = lua.globals();

    if !permisos.leer_archivos && !permisos.escribir_archivos {
        globals.set("io", Value::Nil)?;
        globals.set("dofile", Value::Nil)?;
        globals.set("loadfile", Value::Nil)?;
        // Ocultar glyph.leer_directorio si no tiene permiso
        if let Ok(glyph) = globals.get::<_, Table>("glyph") {
            glyph.set("leer_directorio", Value::Nil)?;
        }
    }

    if !permisos.leer_archivos {
        globals.set("require", Value::Nil)?;
        globals.set("load", Value::Nil)?;
    }

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

fn hex_a_rgb(hex: &str) -> Option<[u8; 3]> {
    let hex = hex.trim_start_matches('#');
    if hex.len() != 6 { return None; }
    Some([
        u8::from_str_radix(&hex[0..2], 16).ok()?,
        u8::from_str_radix(&hex[2..4], 16).ok()?,
        u8::from_str_radix(&hex[4..6], 16).ok()?,
    ])
}
