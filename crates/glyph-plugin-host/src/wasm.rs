// Copyright 2026 Franz (frantsiles)
// Licensed under the Apache License, Version 2.0

//! Host WASM para plugins compilados a `wasm32-unknown-unknown`.
//!
//! ## ABI (ver `glyph-plugin-api/wit/plugin.wit` para la spec completa)
//!
//! El módulo WASM debe exportar:
//! - `glyph_alloc(size: i32) -> i32` — reservar memoria para strings entrantes
//! - `glyph_metadata() -> i32` — puntero a JSON de metadatos
//! - `glyph_inicializar() -> i32`
//! - `glyph_al_abrir(ruta_ptr: i32, ruta_len: i32) -> i32`
//! - `glyph_al_cambiar(ruta_ptr: i32, ruta_len: i32, version: i32) -> i32`
//! - `glyph_al_guardar(ruta_ptr: i32, ruta_len: i32) -> i32`
//!
//! Todos los valores de retorno son punteros a `[u32-LE len][UTF-8 JSON]`.
//! El host importa `env::glyph_log(ptr: i32, len: i32)`.

use std::collections::HashMap;
use std::path::Path;

use anyhow::{anyhow, Result};
use wasmtime::{Caller, Engine, Instance, Linker, Memory, Module, Store};

use glyph_plugin_api::{AccionPlugin, ContextoPlugin, Permisos, Plugin};

// ------------------------------------------------------------------
// Tipos de deserialización JSON (ABI privado)
// ------------------------------------------------------------------

#[derive(serde::Deserialize)]
struct MetadataJson {
    nombre:       String,
    #[serde(default)]
    #[allow(dead_code)]
    descripcion:  String,
    #[serde(default)]
    permisos:     PermisosJson,
}

#[derive(serde::Deserialize, Default)]
struct PermisosJson {
    #[serde(default = "verdadero")]
    ui:                bool,
    #[serde(default)]
    leer_archivos:     bool,
    #[serde(default)]
    escribir_archivos: bool,
    #[serde(default)]
    ejecutar_procesos: bool,
    #[serde(default)]
    red:               bool,
}

fn verdadero() -> bool { true }

/// AccionPlugin en formato JSON externo de serde.
/// Ejemplo: `{"EstablecerTema": {"keyword":"#F92672",...}}`
#[derive(serde::Deserialize)]
enum AccionJson {
    EstablecerTema(HashMap<String, String>),
    LogMensaje(String),
}

fn accion_json_a_plugin(a: AccionJson) -> Option<AccionPlugin> {
    match a {
        AccionJson::EstablecerTema(colores) => {
            let mapa: HashMap<String, [u8; 3]> = colores
                .into_iter()
                .filter_map(|(k, v)| hex_a_rgb(&v).map(|rgb| (k, rgb)))
                .collect();
            Some(AccionPlugin::EstablecerTema(mapa))
        }
        AccionJson::LogMensaje(msg) => Some(AccionPlugin::LogMensaje(msg)),
    }
}

fn hex_a_rgb(hex: &str) -> Option<[u8; 3]> {
    let hex = hex.trim_start_matches('#');
    if hex.len() != 6 { return None; }
    Some([
        u8::from_str_radix(&hex[0..2], 16).ok()?,
        u8::from_str_radix(&hex[2..4], 16).ok()?,
        u8::from_str_radix(&hex[4..6], 16).ok()?,
    ])
}

// ------------------------------------------------------------------
// PluginWasm
// ------------------------------------------------------------------

pub(crate) struct PluginWasm {
    store:    Store<()>,
    instance: Instance,
    memory:   Memory,
    nombre:   String,
    permisos: Permisos,
}

impl PluginWasm {
    /// Carga un plugin WASM desde un archivo `.wasm`.
    pub fn desde_archivo(ruta: &Path) -> Result<Self> {
        let bytes = std::fs::read(ruta)
            .map_err(|e| anyhow!("No se pudo leer '{}':\n  {e}", ruta.display()))?;
        let nombre = ruta
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("plugin-wasm")
            .to_string();
        Self::desde_bytes(&nombre, &bytes)
    }

    /// Carga un plugin WASM desde bytes en memoria.
    pub fn desde_bytes(nombre: &str, bytes: &[u8]) -> Result<Self> {
        let engine = Engine::default();
        let module = Module::from_binary(&engine, bytes)
            .map_err(|e| anyhow!("Módulo WASM inválido '{nombre}': {e}"))?;

        let mut store = Store::new(&engine, ());
        let mut linker: Linker<()> = Linker::new(&engine);

        // — Import: env::glyph_log ───────────────────────────────────
        linker.func_wrap(
            "env",
            "glyph_log",
            |mut caller: Caller<'_, ()>, ptr: i32, len: i32| {
                let Some(ext) = caller.get_export("memory") else { return; };
                let Some(mem) = ext.into_memory() else { return; };
                let data = mem.data(&caller);
                let start = ptr as usize;
                let end = start.saturating_add(len as usize).min(data.len());
                if let Ok(msg) = std::str::from_utf8(&data[start..end]) {
                    tracing::info!("[wasm plugin] {msg}");
                }
            },
        )?;

        let instance = linker
            .instantiate(&mut store, &module)
            .map_err(|e| anyhow!("Error instanciando '{nombre}': {e}"))?;

        let memory = instance
            .get_memory(&mut store, "memory")
            .ok_or_else(|| anyhow!("Plugin '{nombre}' no exporta 'memory'"))?;

        // — Leer metadatos ────────────────────────────────────────────
        let func = instance
            .get_typed_func::<(), i32>(&mut store, "glyph_metadata")
            .map_err(|e| anyhow!("Plugin '{nombre}' no exporta glyph_metadata: {e}"))?;

        let ptr = func.call(&mut store, ())?;
        let json = leer_string_wasm(&memory, &store, ptr)?;

        let meta: MetadataJson = serde_json::from_str(&json)
            .map_err(|e| anyhow!("Metadatos inválidos en '{nombre}': {e}\nJSON: {json}"))?;

        let permisos = Permisos {
            ui:                meta.permisos.ui,
            leer_archivos:     meta.permisos.leer_archivos,
            escribir_archivos: meta.permisos.escribir_archivos,
            ejecutar_procesos: meta.permisos.ejecutar_procesos,
            red:               meta.permisos.red,
        };

        let nombre_final = if meta.nombre.is_empty() {
            nombre.to_string()
        } else {
            meta.nombre.clone()
        };

        tracing::info!(
            "[wasm '{nombre_final}'] cargado — permisos: {}",
            permisos.resumen()
        );

        Ok(Self {
            store,
            instance,
            memory,
            nombre: nombre_final,
            permisos,
        })
    }

    // ── Helpers privados ─────────────────────────────────────────────

    /// Lee un string con prefijo de longitud desde la memoria WASM.
    fn leer_string(&self, ptr: i32) -> Result<String> {
        leer_string_wasm(&self.memory, &self.store, ptr)
    }

    /// Deserializa un JSON de acciones desde un puntero WASM.
    fn ptr_a_acciones(&self, ptr: i32, ctx: &str) -> Vec<AccionPlugin> {
        let json = match self.leer_string(ptr) {
            Ok(s) => s,
            Err(e) => {
                tracing::warn!("[wasm '{}'] error leyendo resultado de {ctx}: {e}", self.nombre);
                return vec![];
            }
        };
        if json.is_empty() || json == "[]" || json == "null" {
            return vec![];
        }
        match serde_json::from_str::<Vec<AccionJson>>(&json) {
            Ok(acciones) => acciones.into_iter().filter_map(accion_json_a_plugin).collect(),
            Err(e) => {
                tracing::warn!("[wasm '{}'] JSON inválido en {ctx}: {e}", self.nombre);
                vec![]
            }
        }
    }

    /// Escribe un string en la memoria WASM (vía glyph_alloc) y retorna el puntero.
    fn escribir_string(&mut self, s: &str) -> Result<i32> {
        let bytes = s.as_bytes();
        if bytes.is_empty() {
            return Ok(0);
        }
        let alloc = self.instance.get_typed_func::<i32, i32>(&mut self.store, "glyph_alloc")
            .map_err(|_| anyhow!("glyph_alloc no disponible"))?;
        let ptr = alloc.call(&mut self.store, bytes.len() as i32)?;
        let data = self.memory.data_mut(&mut self.store);
        let start = ptr as usize;
        if start + bytes.len() > data.len() {
            anyhow::bail!("glyph_alloc retornó ptr fuera de bounds");
        }
        data[start..start + bytes.len()].copy_from_slice(bytes);
        Ok(ptr)
    }

    /// Llama un hook con firma `(ruta_ptr: i32, ruta_len: i32) -> i32`.
    fn llamar_hook_ruta(&mut self, nombre_fn: &str, ctx: &ContextoPlugin) -> Vec<AccionPlugin> {
        let ruta = ctx.ruta.as_deref().unwrap_or("");
        let ruta_ptr = match self.escribir_string(ruta) {
            Ok(p) => p,
            Err(e) => {
                tracing::warn!("[wasm '{}'] no se pudo escribir ruta: {e}", self.nombre);
                return vec![];
            }
        };
        let ruta_len = ruta.len() as i32;

        let hook = match self
            .instance
            .get_typed_func::<(i32, i32), i32>(&mut self.store, nombre_fn)
        {
            Ok(f) => f,
            Err(_) => return vec![],  // hook no implementado = OK
        };

        let ptr = match hook.call(&mut self.store, (ruta_ptr, ruta_len)) {
            Ok(p) => p,
            Err(e) => {
                tracing::warn!("[wasm '{}'] error en {nombre_fn}: {e}", self.nombre);
                return vec![];
            }
        };

        self.ptr_a_acciones(ptr, nombre_fn)
    }
}

// ------------------------------------------------------------------
// Impl Plugin
// ------------------------------------------------------------------

impl Plugin for PluginWasm {
    fn nombre(&self) -> &str { &self.nombre }

    fn permisos(&self) -> Permisos { self.permisos.clone() }

    fn inicializar(&mut self) -> Vec<AccionPlugin> {
        let func = match self
            .instance
            .get_typed_func::<(), i32>(&mut self.store, "glyph_inicializar")
        {
            Ok(f) => f,
            Err(_) => return vec![],
        };
        let ptr = match func.call(&mut self.store, ()) {
            Ok(p) => p,
            Err(e) => {
                tracing::warn!("[wasm '{}'] glyph_inicializar falló: {e}", self.nombre);
                return vec![];
            }
        };
        self.ptr_a_acciones(ptr, "glyph_inicializar")
    }

    fn al_abrir(&mut self, ctx: &ContextoPlugin) -> Vec<AccionPlugin> {
        self.llamar_hook_ruta("glyph_al_abrir", ctx)
    }

    fn al_guardar(&mut self, ctx: &ContextoPlugin) -> Vec<AccionPlugin> {
        self.llamar_hook_ruta("glyph_al_guardar", ctx)
    }

    fn al_cambiar(&mut self, ctx: &ContextoPlugin) -> Vec<AccionPlugin> {
        // al_cambiar tiene una firma diferente: (ruta_ptr, ruta_len, version)
        let ruta = ctx.ruta.as_deref().unwrap_or("");
        let ruta_ptr = match self.escribir_string(ruta) {
            Ok(p) => p,
            Err(e) => {
                tracing::warn!("[wasm '{}'] no se pudo escribir ruta: {e}", self.nombre);
                return vec![];
            }
        };
        let ruta_len = ruta.len() as i32;
        let version = ctx.version_doc as i32;

        let hook = match self
            .instance
            .get_typed_func::<(i32, i32, i32), i32>(&mut self.store, "glyph_al_cambiar")
        {
            Ok(f) => f,
            Err(_) => return vec![],
        };

        let ptr = match hook.call(&mut self.store, (ruta_ptr, ruta_len, version)) {
            Ok(p) => p,
            Err(e) => {
                tracing::warn!("[wasm '{}'] error en glyph_al_cambiar: {e}", self.nombre);
                return vec![];
            }
        };

        self.ptr_a_acciones(ptr, "glyph_al_cambiar")
    }
}

// ------------------------------------------------------------------
// Helper de lectura de memoria WASM
// ------------------------------------------------------------------

/// Lee un string con prefijo u32-LE de longitud desde la memoria WASM.
fn leer_string_wasm(mem: &Memory, store: &impl wasmtime::AsContext, ptr: i32) -> Result<String> {
    let data = mem.data(store);
    let p = ptr as usize;

    if ptr == 0 || p + 4 > data.len() {
        return Ok(String::new());
    }

    let len = u32::from_le_bytes(data[p..p + 4].try_into()?) as usize;
    if len == 0 { return Ok(String::new()); }

    let end = p + 4 + len;
    if end > data.len() {
        anyhow::bail!("string WASM fuera de límites: ptr={ptr} len={len} mem={}", data.len());
    }

    Ok(String::from_utf8(data[p + 4..end].to_vec())
        .map_err(|e| anyhow!("UTF-8 inválido en string WASM: {e}"))?)
}
