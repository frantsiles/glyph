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
// Permisos — declaración de capacidades que el plugin necesita
// ------------------------------------------------------------------

/// Capacidades que un plugin debe declarar para que el host las conceda.
///
/// El host aplica estas restricciones al cargar el plugin. Un plugin
/// que no declara un permiso no puede usarlo aunque su código lo intente.
#[derive(Debug, Clone)]
pub struct Permisos {
    /// Puede modificar la UI (temas, decoraciones). Habilitado por defecto.
    pub ui: bool,
    /// Puede leer archivos del disco.
    pub leer_archivos: bool,
    /// Puede escribir archivos en el disco.
    pub escribir_archivos: bool,
    /// Puede lanzar subprocesos (`os.execute` en Lua, etc.).
    pub ejecutar_procesos: bool,
    /// Puede realizar peticiones de red (reservado para Milestone 4).
    pub red: bool,
}

impl Default for Permisos {
    /// Permisos mínimos: solo UI. Suficiente para un plugin de temas.
    fn default() -> Self {
        Self {
            ui: true,
            leer_archivos: false,
            escribir_archivos: false,
            ejecutar_procesos: false,
            red: false,
        }
    }
}

impl Permisos {
    /// Sin ningún permiso (útil como punto de partida para construir permisos mínimos).
    pub fn ninguno() -> Self {
        Self {
            ui: false,
            ..Default::default()
        }
    }

    /// Todos los permisos habilitados (solo para plugins internos de confianza).
    pub fn todos() -> Self {
        Self {
            ui: true,
            leer_archivos: true,
            escribir_archivos: true,
            ejecutar_procesos: true,
            red: true,
        }
    }

    /// Resumen legible de los permisos concedidos, para logging.
    pub fn resumen(&self) -> String {
        let mut caps: Vec<&str> = Vec::new();
        if self.ui { caps.push("ui"); }
        if self.leer_archivos { caps.push("leer_archivos"); }
        if self.escribir_archivos { caps.push("escribir_archivos"); }
        if self.ejecutar_procesos { caps.push("ejecutar_procesos"); }
        if self.red { caps.push("red"); }
        if caps.is_empty() { "ninguno".to_string() } else { caps.join(", ") }
    }
}

// ------------------------------------------------------------------
// Dirección de navegación — para eventos de teclado en secciones
// ------------------------------------------------------------------

/// Dirección de navegación en una sección de plugin.
#[derive(Debug, Clone, Copy)]
pub enum DireccionNavegacion {
    Arriba,
    Abajo,
    Izquierda,
    Derecha,
    InicioLinea,  // Home
    FinLinea,     // End
    PaginaArriba, // Page Up
    PaginaAbajo,  // Page Down
    InicioDoc,    // Ctrl+Home
    FinDoc,       // Ctrl+End
}

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
    /// Plugin que provocó la edición, usado para evitar loops de notificación.
    pub origen_plugin: Option<String>,
}

// ------------------------------------------------------------------
// Tipos para el sistema de secciones (M5)
// ------------------------------------------------------------------

/// Una línea de texto con estilo para renderizar en una sección de plugin.
#[derive(Debug, Clone)]
pub struct LineaSeccion {
    pub texto: String,
    /// Color RGB del texto — None usa el color por defecto de la sección.
    pub color: Option<[u8; 3]>,
    pub negrita: bool,
    /// Color RGB de fondo de la línea — None usa el fondo de la sección.
    pub fondo: Option<[u8; 3]>,
    /// Datos opacos devueltos al plugin cuando el usuario hace click en esta línea.
    pub payload: Option<Vec<u8>>,
}

impl LineaSeccion {
    pub fn simple(texto: impl Into<String>) -> Self {
        Self { texto: texto.into(), color: None, negrita: false, fondo: None, payload: None }
    }

    pub fn con_color(texto: impl Into<String>, color: [u8; 3]) -> Self {
        Self { texto: texto.into(), color: Some(color), negrita: false, fondo: None, payload: None }
    }
}

/// Configuración de una sección declarada por un plugin.
#[derive(Debug, Clone)]
pub struct SeccionConfig {
    pub id: String,
    /// "izquierda" | "derecha" | "arriba" | "abajo"
    pub lado: String,
    /// Tamaño en píxeles (fijo para M5)
    pub tamano: f32,
    /// Color de fondo RGB — None usa el color de fondo global del editor.
    pub color_fondo: Option<[u8; 3]>,
}

#[derive(Debug, Clone)]
pub enum NivelNotificacion {
    Info,
    Aviso,
    Error,
}

// ------------------------------------------------------------------
// Tipos para decoraciones en el gutter (M6)
// ------------------------------------------------------------------

/// Estado de una línea para la decoración del gutter.
#[derive(Debug, Clone)]
pub enum EstadoGutter {
    Añadido,
    Modificado,
    Borrado,
}

/// Decoración de una línea en el editor, típicamente usada por `plugin-git`.
#[derive(Debug, Clone)]
pub struct DecoracionLinea {
    /// Línea 0-based a decorar.
    pub linea: u32,
    pub estado: EstadoGutter,
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

    // ── M5: Sistema de secciones ──────────────────────────────────────

    /// Registra una nueva sección visual. Se llama típicamente desde `inicializar`.
    RegistrarSeccion(SeccionConfig),

    /// Actualiza el contenido visible de una sección. Reemplaza las líneas previas.
    ActualizarContenidoSeccion { id: String, lineas: Vec<LineaSeccion> },

    /// Elimina una sección registrada.
    QuitarSeccion(String),

    /// Solicita al editor que abra un archivo en un nuevo tab.
    AbrirArchivo(String),

    /// Reemplaza todo el contenido del buffer activo.
    ReemplazarContenidoBuffer { contenido: String, origen_plugin: String },

    /// Solicita decoraciones de línea en el gutter.
    DecorarLineas(Vec<DecoracionLinea>),

    /// Muestra una notificación efímera al usuario.
    MostrarNotificacion { mensaje: String, nivel: NivelNotificacion },
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

    /// Permisos que el plugin declara necesitar.
    /// El host los lee al cargar el plugin y aplica el sandbox correspondiente.
    fn permisos(&self) -> Permisos {
        Permisos::default()
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

    /// Se llama cuando el usuario hace click en una sección registrada por este plugin.
    /// `linea` es el índice 0-based de la línea clickeada en `Vec<LineaSeccion>`.
    fn click_seccion(&mut self, _id_seccion: &str, _linea: u32) -> Vec<AccionPlugin> {
        vec![]
    }

    /// Se llama cuando el usuario presiona una tecla en una sección registrada por este plugin.
    /// `tecla` es el nombre de la tecla (ej. "ArrowUp", "Enter").
    /// `modifiers` es una cadena con modificadores (ej. "Ctrl", "Shift", o "").
    fn tecla_seccion(&mut self, _id_seccion: &str, _tecla: &str, _modifiers: &str) -> Vec<AccionPlugin> {
        vec![]
    }
}
