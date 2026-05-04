// Copyright 2026 Franz (frantsiles)
// Licensed under the Apache License, Version 2.0

//! # plugin-markdown
//!
//! Plugin oficial de preview Markdown para Glyph.
//!
//! Muestra una barra de estado en la parte inferior cuando se abre un archivo
//! `.md` o `.markdown`. El usuario puede hacer click (o Ctrl+Shift+M) para
//! alternar el preview en el navegador del sistema.

/// Nombre del plugin
pub const NOMBRE: &str = "markdown";

/// Script Lua del plugin, embebido en tiempo de compilación.
pub const SCRIPT: &str = include_str!("../init.lua");
