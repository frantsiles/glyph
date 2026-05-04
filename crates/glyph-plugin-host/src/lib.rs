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
//!   └─ cargar_lua(nombre, script) / cargar_wasm(ruta)
//!
//! HostPlugins::inicializar()
//!   └─ Plugin::inicializar() en cada plugin
//!       ├─ AccionPlugin::EstablecerTema → actualiza tema
//!       ├─ AccionPlugin::RegistrarSeccion → registra sección
//!       └─ AccionPlugin::ActualizarContenidoSeccion → carga contenido inicial
//!
//! HostPlugins::secciones_para_render()  →  Vec<SeccionContenidoRender>
//! HostPlugins::evento_seccion(id, linea) → Vec<AccionPlugin>
//! ```

mod lua;
mod wasm;

use std::collections::HashMap;
use std::path::Path;

use glyph_plugin_api::{AccionPlugin, ContextoPlugin, DireccionNavegacion, LineaSeccion, NivelNotificacion, Permisos, Plugin, SeccionConfig};

use crate::lua::PluginLua;
use crate::wasm::PluginWasm;

// ------------------------------------------------------------------
// Tema por defecto (One Dark)
// ------------------------------------------------------------------

fn tema_por_defecto() -> HashMap<String, [u8; 3]> {
    [
        ("keyword",     [0xF0, 0xAA, 0xAC]),
        ("string",      [0xCC, 0xDD, 0xFF]),
        ("comment",     [0x7A, 0x8C, 0xB4]),
        ("function",    [0xAF, 0xAA, 0xF0]),
        ("type",        [0x9A, 0xD0, 0xE6]),
        ("number",      [0xAA, 0xDC, 0xF0]),
        ("operator",    [0xAA, 0xC1, 0xF0]),
        ("variable",    [0xFF, 0xFF, 0xFF]),
        ("constant",    [0xAA, 0xDC, 0xF0]),
        ("punctuation", [0x7A, 0x92, 0xC2]),
        ("attribute",   [0xAA, 0xDC, 0xF0]),
        ("default",     [0xFF, 0xFF, 0xFF]),
    ]
    .into_iter()
    .map(|(k, v)| (k.to_string(), v))
    .collect()
}

// ------------------------------------------------------------------
// Datos internos de una sección registrada
// ------------------------------------------------------------------

struct SeccionRegistrada {
    config: SeccionConfig,
    plugin_nombre: String,
    contenido: Vec<LineaSeccion>,
}

// ------------------------------------------------------------------
// HostPlugins
// ------------------------------------------------------------------

pub struct HostPlugins {
    plugins: Vec<Box<dyn Plugin>>,
    tema_activo: HashMap<String, [u8; 3]>,
    permisos: HashMap<String, Permisos>,
    /// Secciones registradas por plugins (id → datos)
    secciones: HashMap<String, SeccionRegistrada>,
    /// Rutas de archivos pendientes de abrir en la app
    archivos_pendientes: Vec<String>,
    /// Mapeos de extensión a nombre de lenguaje registrados por plugins
    /// Clave: extensión sin punto ("foo"). Valor: nombre de lenguaje ("yaml").
    mapeos_extension: HashMap<String, String>,
}

impl HostPlugins {
    pub fn nuevo() -> Self {
        Self {
            plugins: Vec::new(),
            tema_activo: tema_por_defecto(),
            permisos: HashMap::new(),
            secciones: HashMap::new(),
            archivos_pendientes: Vec::new(),
            mapeos_extension: HashMap::new(),
        }
    }

    /// Devuelve el nombre del lenguaje registrado por algún plugin para esta extensión,
    /// o `None` si ningún plugin ha declarado un mapeo para ella.
    pub fn lenguaje_para_extension(&self, ext: &str) -> Option<&str> {
        self.mapeos_extension.get(ext).map(|s| s.as_str())
    }

    /// Carga un plugin Lua y lo registra.
    pub fn cargar_lua(&mut self, nombre: &str, script: &str) -> anyhow::Result<()> {
        let plugin = PluginLua::desde_str(nombre, script)?;
        let perms = plugin.permisos();
        self.permisos.insert(nombre.to_string(), perms);
        self.plugins.push(Box::new(plugin));
        tracing::info!("Plugin cargado: '{nombre}'");
        Ok(())
    }

    /// Carga un plugin WASM desde archivo y lo registra.
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

    /// Inicializa todos los plugins.
    pub fn inicializar(&mut self) {
        let acciones = self.recoger_acciones(|p| p.inicializar(), None);
        for (nombre, accion) in acciones {
            let _ = self.aplicar_accion(accion, &nombre);
        }
    }

    /// Dispara el hook `al_abrir`.
    pub fn al_abrir(&mut self, ruta: Option<&str>) -> Vec<AccionPlugin> {
        let ctx = ContextoPlugin {
            ruta: ruta.map(|s| s.to_string()),
            version_doc: 0,
            origen_plugin: None,
        };
        let acciones = self.recoger_acciones(|p| p.al_abrir(&ctx), None);
        self.aplicar_acciones(acciones)
    }

    /// Dispara el hook `al_cambiar`.
    pub fn al_cambiar(&mut self, ruta: Option<&str>, version: u32, origen_plugin: Option<&str>) -> Vec<AccionPlugin> {
        let ctx = ContextoPlugin {
            ruta: ruta.map(|s| s.to_string()),
            version_doc: version,
            origen_plugin: origen_plugin.map(|s| s.to_string()),
        };
        let acciones = self.recoger_acciones(|p| p.al_cambiar(&ctx), origen_plugin);
        self.aplicar_acciones(acciones)
    }

    /// Dispara el hook `al_guardar`.
    pub fn al_guardar(&mut self, ruta: Option<&str>, origen_plugin: Option<&str>) -> Vec<AccionPlugin> {
        let ctx = ContextoPlugin {
            ruta: ruta.map(|s| s.to_string()),
            version_doc: 0,
            origen_plugin: origen_plugin.map(|s| s.to_string()),
        };
        let acciones = self.recoger_acciones(|p| p.al_guardar(&ctx), origen_plugin);
        self.aplicar_acciones(acciones)
    }

    /// Enruta un evento de navegación de teclado a una sección del plugin.
    /// Mapea DireccionNavegacion a nombres de teclas (Arrow-Up, ArrowDown, etc.)
    /// y devuelve las acciones resultantes.
    pub fn navegacion_seccion(&mut self, id: &str, direccion: DireccionNavegacion) -> Vec<AccionPlugin> {
        let tecla = match direccion {
            DireccionNavegacion::Arriba => "ArrowUp",
            DireccionNavegacion::Abajo => "ArrowDown",
            DireccionNavegacion::Izquierda => "ArrowLeft",
            DireccionNavegacion::Derecha => "ArrowRight",
            DireccionNavegacion::InicioLinea => "Home",
            DireccionNavegacion::FinLinea => "End",
            DireccionNavegacion::PaginaArriba => "PageUp",
            DireccionNavegacion::PaginaAbajo => "PageDown",
            DireccionNavegacion::InicioDoc => "Home",
            DireccionNavegacion::FinDoc => "End",
        };

        let nombre_plugin = match self.secciones.get(id) {
            Some(s) => s.plugin_nombre.clone(),
            None => return vec![],
        };

        let acciones = {
            let plugin = self.plugins.iter_mut()
                .find(|p| p.nombre() == nombre_plugin);
            match plugin {
                Some(p) => p.tecla_seccion(id, tecla, ""),
                None => vec![],
            }
        };

        // Aplicar las acciones (actualizar estado interno del host)
        let mut acciones_externas = Vec::new();
        for accion in acciones {
            if let Some(externa) = self.aplicar_accion(accion, &nombre_plugin) {
                acciones_externas.push(externa);
            }
        }
        acciones_externas
    }

    /// Enruta un evento de sección al plugin propietario.
    /// Si `linea == u32::MAX` es el centinela de teclado Enter → llama `tecla_seccion("Enter")`.
    /// Para clicks reales llama `click_seccion(linea)`.
    pub fn evento_seccion(&mut self, id: &str, linea: u32) -> Vec<AccionPlugin> {
        let nombre_plugin = match self.secciones.get(id) {
            Some(s) => s.plugin_nombre.clone(),
            None => return vec![],
        };

        let acciones = {
            let plugin = self.plugins.iter_mut()
                .find(|p| p.nombre() == nombre_plugin);
            match plugin {
                Some(p) if linea == u32::MAX => p.tecla_seccion(id, "Enter", ""),
                Some(p) => p.click_seccion(id, linea),
                None => vec![],
            }
        };

        // Aplicar las acciones (actualizar estado interno del host)
        let mut acciones_externas = Vec::new();
        for accion in acciones {
            if let Some(externa) = self.aplicar_accion(accion, &nombre_plugin) {
                acciones_externas.push(externa);
            }
        }
        acciones_externas
    }

    /// Devuelve y limpia la cola de archivos pendientes de abrir.
    pub fn drenar_archivos_pendientes(&mut self) -> Vec<String> {
        std::mem::take(&mut self.archivos_pendientes)
    }

    /// Color del tema activo para una clave semántica.
    pub fn color(&self, clave: &str) -> [u8; 3] {
        self.tema_activo.get(clave).copied().unwrap_or([0xAB, 0xB2, 0xBF])
    }

    /// Genera la lista de secciones con su contenido, lista para `ContenidoRender`.
    pub fn secciones_para_render(&self) -> Vec<SeccionParaRender> {
        self.secciones.values().map(|s| SeccionParaRender {
            id: s.config.id.clone(),
            lado: s.config.lado.clone(),
            tamano: s.config.tamano,
            color_fondo: s.config.color_fondo,
            lineas: s.contenido.iter().map(|l| LineaParaRender {
                texto: l.texto.clone(),
                color: l.color,
                negrita: l.negrita,
                fondo: l.fondo,
            }).collect(),
        }).collect()
    }

    // ── Privados ─────────────────────────────────────────────────────

    fn recoger_acciones<F>(&mut self, mut hook: F, origen_plugin: Option<&str>) -> Vec<(String, AccionPlugin)>
    where
        F: FnMut(&mut Box<dyn Plugin>) -> Vec<AccionPlugin>,
    {
        let mut acciones = Vec::new();
        for plugin in self.plugins.iter_mut() {
            let nombre = plugin.nombre().to_string();
            if origen_plugin.map_or(false, |or| or == nombre) {
                continue;
            }
            for accion in hook(plugin) {
                acciones.push((nombre.clone(), accion));
            }
        }
        acciones
    }

    fn aplicar_acciones(&mut self, acciones: Vec<(String, AccionPlugin)>) -> Vec<AccionPlugin> {
        acciones.into_iter()
            .filter_map(|(nombre, accion)| self.aplicar_accion(accion, &nombre))
            .collect()
    }

    fn aplicar_accion(&mut self, accion: AccionPlugin, nombre_plugin: &str) -> Option<AccionPlugin> {
        let tiene_ui = self.permisos.get(nombre_plugin).map(|p| p.ui).unwrap_or(false);
        let tiene_leer = self.permisos.get(nombre_plugin).map(|p| p.leer_archivos).unwrap_or(false);

        match accion {
            AccionPlugin::EstablecerTema(tema) => {
                if !tiene_ui {
                    tracing::warn!("['{nombre_plugin}'] rechazado: EstablecerTema requiere permiso 'ui'");
                    return None;
                }
                tracing::info!("['{nombre_plugin}'] tema aplicado ({} colores)", tema.len());
                self.tema_activo = tema;
                None
            }

            AccionPlugin::LogMensaje(msg) => {
                tracing::info!("['{nombre_plugin}'] {msg}");
                None
            }

            AccionPlugin::RegistrarSeccion(config) => {
                if !tiene_ui {
                    tracing::warn!("['{nombre_plugin}'] rechazado: RegistrarSeccion requiere 'ui'");
                    return None;
                }
                tracing::info!("['{nombre_plugin}'] sección registrada: '{}'", config.id);
                self.secciones.insert(config.id.clone(), SeccionRegistrada {
                    config,
                    plugin_nombre: nombre_plugin.to_string(),
                    contenido: Vec::new(),
                });
                None
            }

            AccionPlugin::ActualizarContenidoSeccion { id, lineas } => {
                if let Some(sec) = self.secciones.get_mut(&id) {
                    sec.contenido = lineas;
                } else {
                    tracing::warn!("['{nombre_plugin}'] ActualizarContenidoSeccion: sección '{id}' no registrada");
                }
                None
            }

            AccionPlugin::QuitarSeccion(id) => {
                self.secciones.remove(&id);
                tracing::info!("['{nombre_plugin}'] sección '{id}' eliminada");
                None
            }

            AccionPlugin::AbrirArchivo(ruta) => {
                if !tiene_leer {
                    tracing::warn!("['{nombre_plugin}'] rechazado: AbrirArchivo requiere 'leer_archivos'");
                    return None;
                }
                Some(AccionPlugin::AbrirArchivo(ruta))
            }

            AccionPlugin::ReemplazarContenidoBuffer { contenido, origen_plugin } => {
                Some(AccionPlugin::ReemplazarContenidoBuffer { contenido, origen_plugin })
            }

            AccionPlugin::DecorarLineas(lineas) => {
                Some(AccionPlugin::DecorarLineas(lineas))
            }

            AccionPlugin::MostrarNotificacion { mensaje, nivel } => {
                let nivel_str = match nivel {
                    NivelNotificacion::Info  => "INFO",
                    NivelNotificacion::Aviso => "AVISO",
                    NivelNotificacion::Error => "ERROR",
                };
                tracing::info!("[notificacion {nivel_str}] {mensaje}");
                None
            }

            AccionPlugin::RegistrarMapeoExtension { extension, lenguaje } => {
                tracing::info!("['{nombre_plugin}'] extensión .{extension} → {lenguaje}");
                self.mapeos_extension.insert(extension, lenguaje);
                None
            }

            AccionPlugin::EstablecerLenguajeBuffer(lenguaje) => {
                // El host no controla el buffer — pasa la acción a la app.
                Some(AccionPlugin::EstablecerLenguajeBuffer(lenguaje))
            }

            AccionPlugin::ToggleVistaPreviaMd => {
                // El host no controla la ventana del navegador — pasa la acción a la app.
                Some(AccionPlugin::ToggleVistaPreviaMd)
            }
        }
    }
}

impl Default for HostPlugins {
    fn default() -> Self { Self::nuevo() }
}

// ------------------------------------------------------------------
// Tipos para transferir datos de sección al exterior (sin deps de renderer)
// ------------------------------------------------------------------

pub struct LineaParaRender {
    pub texto: String,
    pub color: Option<[u8; 3]>,
    pub negrita: bool,
    pub fondo: Option<[u8; 3]>,
}

pub struct SeccionParaRender {
    pub id: String,
    pub lado: String,
    pub tamano: f32,
    pub color_fondo: Option<[u8; 3]>,
    pub lineas: Vec<LineaParaRender>,
}
