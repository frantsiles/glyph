// Copyright 2026 Franz (frantsiles)
// Licensed under the Apache License, Version 2.0

//! # Historia (Undo/Redo)
//!
//! Sistema de historial de cambios para undo/redo ilimitado.
//! Basado en el patrón Command — cada operación es reversible.

/// Una operación atómica que puede deshacerse
#[derive(Debug, Clone)]
pub enum Operacion {
    /// Inserción de texto: posición y texto insertado
    Insertar { indice: usize, texto: String },
    /// Eliminación de texto: posición y texto eliminado (para poder revertir)
    Eliminar { indice: usize, texto: String },
}

/// Historial de operaciones con soporte de undo/redo
#[derive(Debug, Default)]
pub struct Historia {
    /// Operaciones pasadas — se deshacen en orden inverso
    pasado: Vec<Vec<Operacion>>,

    /// Operaciones futuras — disponibles después de un undo
    futuro: Vec<Vec<Operacion>>,

    /// Operaciones del grupo actual (se agrupan en un solo undo)
    grupo_actual: Vec<Operacion>,
}

impl Historia {
    pub fn nueva() -> Self {
        Self::default()
    }

    /// Registra una operación en el grupo actual
    pub fn registrar(&mut self, operacion: Operacion) {
        // Cualquier nueva acción limpia el futuro (no más redo después de editar)
        self.futuro.clear();
        self.grupo_actual.push(operacion);
    }

    /// Confirma el grupo actual como un punto de undo único
    /// Llamar después de completar una acción lógica del usuario
    pub fn confirmar_grupo(&mut self) {
        if !self.grupo_actual.is_empty() {
            let grupo = std::mem::take(&mut self.grupo_actual);
            self.pasado.push(grupo);
        }
    }

    /// Retorna las operaciones a deshacer, si hay alguna
    pub fn deshacer(&mut self) -> Option<Vec<Operacion>> {
        self.confirmar_grupo(); // asegurarse de confirmar lo pendiente
        let grupo = self.pasado.pop()?;
        self.futuro.push(grupo.clone());
        Some(grupo)
    }

    /// Retorna las operaciones a rehacer, si hay alguna
    pub fn rehacer(&mut self) -> Option<Vec<Operacion>> {
        let grupo = self.futuro.pop()?;
        self.pasado.push(grupo.clone());
        Some(grupo)
    }

    /// Retorna true si hay operaciones para deshacer
    pub fn puede_deshacer(&self) -> bool {
        !self.pasado.is_empty() || !self.grupo_actual.is_empty()
    }

    /// Retorna true si hay operaciones para rehacer
    pub fn puede_rehacer(&self) -> bool {
        !self.futuro.is_empty()
    }

    /// Limpia todo el historial
    pub fn limpiar(&mut self) {
        self.pasado.clear();
        self.futuro.clear();
        self.grupo_actual.clear();
    }
}