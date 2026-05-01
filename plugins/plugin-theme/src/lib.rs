// Copyright 2026 Franz (frantsiles)
// Licensed under the Apache License, Version 2.0

//! # plugin-theme
//!
//! Plugin oficial de temas para Glyph.
//!
//! El tema se define en Lua (`init.lua`) y se embebe en el binario en
//! tiempo de compilación. El host lo carga con:
//!
//! ```ignore
//! use glyph_plugin_host::HostPlugins;
//!
//! let mut host = HostPlugins::nuevo();
//! host.cargar_lua(plugin_theme::NOMBRE, plugin_theme::TEMA_SCRIPT).unwrap();
//! host.inicializar();
//! ```

/// Nombre del plugin (usado para identificación en logs)
pub const NOMBRE: &str = "One Dark";

/// Script Lua del tema, embebido en el binario en tiempo de compilación.
/// El usuario puede sobrescribir el tema cargando su propio script Lua.
pub const TEMA_SCRIPT: &str = include_str!("../init.lua");
