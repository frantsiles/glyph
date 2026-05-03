// Copyright 2026 Franz (frantsiles)
// Licensed under the Apache License, Version 2.0

//! # plugin-sidebar
//!
//! Plugin oficial de explorador de archivos para Glyph.
//!
//! El explorador se implementa en Lua (`init.lua`) y se embebe en el binario.

/// Nombre del plugin
pub const NOMBRE: &str = "sidebar";

/// Script Lua del explorador, embebido en tiempo de compilación.
pub const SCRIPT: &str = include_str!("../init.lua");
