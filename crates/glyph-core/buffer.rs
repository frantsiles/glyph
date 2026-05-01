// Copyright 2026 Franz (frantsiles)
// Licensed under the Apache License, Version 2.0

//! # Buffer
//!
//! Estructura de datos principal del editor basada en [`ropey::Rope`].
//!
//! ## ¿Por qué Rope?
//!
//! Un String simple tiene inserción/eliminación O(n) — lento en archivos grandes.
//! Rope divide el texto en un árbol balanceado, logrando O(log n) para ediciones,
//! lo que mantiene el editor fluido incluso en archivos de millones de líneas.
//!
//! ## Uso básico
//!
//! ```rust
//! use glyph_core::Buffer;
//!
//! let mut buffer = Buffer::nuevo("fn main() {\n    println!(\"Hola\");\n}");
//! buffer.insertar(12, "    // comentario\n");
//! assert_eq!(buffer.lineas(), 4);
//! ```

use anyhow::Result;
use ropey::Rope;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use uuid::Uuid;

/// Representa un buffer de texto en memoria.
///
/// Cada archivo abierto en el editor corresponde a un Buffer.
/// Puede existir un buffer sin archivo asociado (ej: buffer nuevo sin guardar).
#[derive(Debug)]
pub struct Buffer {
    /// Identificador único del buffer — no cambia durante la sesión
    pub id: Uuid,

    /// Contenido del texto usando estructura Rope para edición eficiente
    rope: Rope,

    /// Ruta del archivo en disco, si existe
    pub ruta: Option<PathBuf>,

    /// Si el buffer tiene cambios sin guardar
    pub modificado: bool,

    /// Codificación detectada al abrir el archivo
    pub codificacion: Codificacion,

    /// Tipo de fin de línea detectado
    pub fin_de_linea: FinDeLinea,
}

/// Codificación del archivo
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum Codificacion {
    #[default]
    Utf8,
    Utf8Bom,
    Utf16Le,
    Utf16Be,
}

/// Tipo de fin de línea
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum FinDeLinea {
    #[default]
    Lf,        // Unix/Linux/macOS (\n)
    CrLf,      // Windows (\r\n)
    Cr,        // Antiguo macOS (\r)
}

impl Buffer {
    /// Crea un buffer nuevo con contenido inicial
    pub fn nuevo(contenido: &str) -> Self {
        Self {
            id: Uuid::new_v4(),
            rope: Rope::from_str(contenido),
            ruta: None,
            modificado: false,
            codificacion: Codificacion::default(),
            fin_de_linea: FinDeLinea::default(),
        }
    }

    /// Crea un buffer vacío (para archivos nuevos)
    pub fn vacio() -> Self {
        Self::nuevo("")
    }

    /// Crea un buffer asociado a una ruta de archivo
    pub fn con_ruta(contenido: &str, ruta: PathBuf) -> Self {
        Self {
            ruta: Some(ruta),
            ..Self::nuevo(contenido)
        }
    }

    // ------------------------------------------------------------------
    // Consultas — operaciones de lectura, no modifican el buffer
    // ------------------------------------------------------------------

    /// Número total de líneas en el documento
    pub fn lineas(&self) -> usize {
        self.rope.len_lines()
    }

    /// Número total de caracteres (unicode scalar values)
    pub fn caracteres(&self) -> usize {
        self.rope.len_chars()
    }

    /// Número total de bytes
    pub fn bytes(&self) -> usize {
        self.rope.len_bytes()
    }

    /// Obtiene el contenido de una línea específica (0-indexado)
    pub fn linea(&self, indice: usize) -> Option<String> {
        if indice >= self.lineas() {
            return None;
        }
        Some(self.rope.line(indice).to_string())
    }

    /// Convierte una posición (línea, columna) a un índice de carácter absoluto
    ///
    /// # Argumentos
    /// * `linea` — Número de línea (0-indexado)
    /// * `columna` — Número de columna en caracteres (0-indexado)
    pub fn posicion_a_indice(&self, linea: usize, columna: usize) -> Result<usize> {
        let inicio_linea = self.rope.line_to_char(linea);
        Ok(inicio_linea + columna)
    }

    /// Convierte un índice de carácter absoluto a (línea, columna)
    pub fn indice_a_posicion(&self, indice: usize) -> Result<(usize, usize)> {
        let linea = self.rope.char_to_line(indice);
        let inicio_linea = self.rope.line_to_char(linea);
        let columna = indice - inicio_linea;
        Ok((linea, columna))
    }

    /// Extrae un rango de texto como String
    ///
    /// # Argumentos
    /// * `inicio` — Índice de carácter de inicio (inclusivo)
    /// * `fin` — Índice de carácter de fin (exclusivo)
    pub fn rango_texto(&self, inicio: usize, fin: usize) -> String {
        self.rope.slice(inicio..fin).to_string()
    }

    /// Retorna todo el contenido del buffer como String
    pub fn contenido_completo(&self) -> String {
        self.rope.to_string()
    }

    // ------------------------------------------------------------------
    // Mutaciones — operaciones que modifican el buffer
    // Todas marcan el buffer como modificado automáticamente
    // ------------------------------------------------------------------

    /// Inserta texto en una posición de carácter absoluta
    ///
    /// # Argumentos
    /// * `indice` — Posición donde insertar (0-indexado)
    /// * `texto` — Texto a insertar
    pub fn insertar(&mut self, indice: usize, texto: &str) {
        self.rope.insert(indice, texto);
        self.modificado = true;
    }

    /// Inserta texto en una posición (línea, columna)
    pub fn insertar_en(&mut self, linea: usize, columna: usize, texto: &str) -> Result<()> {
        let indice = self.posicion_a_indice(linea, columna)?;
        self.insertar(indice, texto);
        Ok(())
    }

    /// Elimina un rango de caracteres
    ///
    /// # Argumentos
    /// * `inicio` — Índice de inicio (inclusivo)
    /// * `fin` — Índice de fin (exclusivo)
    pub fn eliminar(&mut self, inicio: usize, fin: usize) {
        self.rope.remove(inicio..fin);
        self.modificado = true;
    }

    /// Reemplaza un rango de texto con nuevo contenido
    pub fn reemplazar(&mut self, inicio: usize, fin: usize, texto: &str) {
        self.eliminar(inicio, fin);
        self.insertar(inicio, texto);
    }

    /// Reemplaza todo el contenido del buffer
    pub fn reemplazar_todo(&mut self, nuevo_contenido: &str) {
        self.rope = Rope::from_str(nuevo_contenido);
        self.modificado = true;
    }

    /// Marca el buffer como guardado (limpia el flag de modificado)
    pub fn marcar_guardado(&mut self) {
        self.modificado = false;
    }
}

// ------------------------------------------------------------------
// Tests unitarios
// ------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_buffer_nuevo() {
        let buffer = Buffer::nuevo("hola mundo");
        assert_eq!(buffer.caracteres(), 10);
        assert_eq!(buffer.lineas(), 1);
        assert!(!buffer.modificado);
    }

    #[test]
    fn test_insertar() {
        let mut buffer = Buffer::nuevo("hola");
        buffer.insertar(4, " mundo");
        assert_eq!(buffer.contenido_completo(), "hola mundo");
        assert!(buffer.modificado);
    }

    #[test]
    fn test_eliminar() {
        let mut buffer = Buffer::nuevo("hola mundo");
        buffer.eliminar(4, 10);
        assert_eq!(buffer.contenido_completo(), "hola");
    }

    #[test]
    fn test_posicion_a_indice() {
        let buffer = Buffer::nuevo("linea1\nlinea2\nlinea3");
        let indice = buffer.posicion_a_indice(1, 3).unwrap();
        // línea 0 = "linea1\n" (7 chars), línea 1 col 3 = índice 10
        assert_eq!(indice, 10);
    }

    #[test]
    fn test_multiples_lineas() {
        let buffer = Buffer::nuevo("a\nb\nc");
        assert_eq!(buffer.lineas(), 3);
        assert_eq!(buffer.linea(1).unwrap().trim(), "b");
    }
}