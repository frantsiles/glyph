// Copyright 2026 Franz (frantsiles)
// Licensed under the Apache License, Version 2.0

//! # EventoEditor
//!
//! Contrato de eventos entre el renderer (capa de entrada) y la app (capa de lógica).
//!
//! El renderer detecta input del teclado y lo traduce a `EventoEditor`.
//! La app recibe esos eventos, actualiza el `Document` del core y devuelve
//! un `ContenidoRender` actualizado. El renderer no sabe nada de `glyph-core`.

/// Dirección de movimiento del cursor
#[derive(Debug, Clone, Copy)]
pub enum DireccionCursor {
    Izquierda,
    Derecha,
    Arriba,
    Abajo,
    InicioLinea,  // Home
    FinLinea,     // End
    PaginaArriba, // Page Up — sube N líneas
    PaginaAbajo,  // Page Down — baja N líneas
    InicioDoc,    // Ctrl+Home — inicio del documento
    FinDoc,       // Ctrl+End — fin del documento
}

/// Eventos de edición emitidos por el renderer hacia la capa de aplicación
#[derive(Debug, Clone)]
pub enum EventoEditor {
    /// Insertar texto en la posición del cursor (incluye "\n" para Enter)
    InsertarTexto(String),

    /// Borrar el carácter antes del cursor (Backspace)
    BorrarAtras,

    /// Borrar el carácter después del cursor (Delete)
    BorrarAdelante,

    /// Mover el cursor en una dirección
    MoverCursor(DireccionCursor),

    /// Deshacer la última operación (Ctrl+Z)
    Deshacer,

    /// Rehacer la operación deshecha (Ctrl+Y)
    Rehacer,

    /// Guardar el archivo actual (Ctrl+S)
    Guardar,

    /// Activar modo búsqueda (Ctrl+F)
    IniciarBusqueda,

    /// El usuario actualizó el texto de búsqueda
    ActualizarBusqueda(String),

    /// Saltar al siguiente resultado (Enter en modo búsqueda)
    SiguienteMatch,

    /// Saltar al resultado anterior (Shift+Enter en modo búsqueda)
    MatchAnterior,

    /// Salir del modo búsqueda/reemplazo (Escape)
    TerminarBusqueda,

    /// Activar modo búsqueda+reemplazo (Ctrl+H)
    IniciarReemplazo,

    /// El usuario actualizó el texto de reemplazo
    ActualizarReemplazo(String),

    /// Reemplazar el match activo y avanzar al siguiente (Enter en modo reemplazo)
    ReemplazarMatch,

    /// Reemplazar todas las ocurrencias (Ctrl+H en modo reemplazo)
    ReemplazarTodo,

    /// Click del ratón — mover cursor a (línea, columna) en coordenadas de carácter
    MoverCursorA { linea: u32, columna: u32 },

    /// Pedir información de hover LSP en la posición del cursor (Ctrl+K)
    PedirHover,

    /// Seleccionar todo el documento (Ctrl+A)
    SeleccionarTodo,

    /// Mover cursor extendiendo la selección activa (Shift+Flecha / Shift+Home / Shift+End)
    ExtenderSeleccion(DireccionCursor),

    /// Copiar texto seleccionado al portapapeles (Ctrl+C)
    Copiar,

    /// Cortar texto seleccionado al portapapeles (Ctrl+X)
    Cortar,

    /// Pegar texto desde el portapapeles (Ctrl+V)
    Pegar,

    /// Abrir un nuevo tab vacío (Ctrl+T)
    NuevoTab,

    /// Una sección cambió el foco (por click o navegación)
    /// El String es el ID de la sección que ahora tiene foco
    CambioFoco(String),

    /// Cerrar el tab activo (Ctrl+W)
    CerrarTab,

    /// Activar el tab siguiente (Ctrl+Tab)
    SiguienteTab,

    /// Activar el tab anterior (Ctrl+Shift+Tab)
    AnteriorTab,

    /// Activar un tab concreto por índice (click en la barra de tabs)
    ActivarTab(usize),

    // ── M5: Secciones de plugin ───────────────────────────────────────

    /// El usuario hizo click en una sección de plugin.
    /// `linea` es el índice 0-based de la línea clickeada.
    ///
    /// **Nota para implementadores de secciones:**
    /// - La app debe renderizar el item en la línea con estilo "seleccionado" (diferente color/fondo)
    /// - Si el usuario hace Enter sobre el item seleccionado, emitirá otro EventoSeccion con la misma línea
    /// - La app puede usar este evento para abrir/activar el item (ej: abrir carpeta, archivo, etc.)
    EventoSeccion { id_seccion: String, linea: u32 },

    /// Navegación en una sección (teclado).
    /// Emitido cuando se presionan flechas u otras teclas de navegación en una sección que no es editor.
    ///
    /// **Ejemplo: Navegación en sidebar de carpetas**
    /// - Flecha Arriba → cambiar selección a item anterior
    /// - Flecha Abajo → cambiar selección a item siguiente
    /// - Flecha Derecha → expandir carpeta si está colapsada, o entrar en subcarpeta
    /// - Flecha Izquierda → colapsar carpeta si está expandida, o salir a carpeta padre
    /// - Home/End → ir al primer/último item
    /// - Page Up/Down → scroll de múltiples items
    ///
    /// **La app debe:**
    /// 1. Rastrear el `current_selected_line` en la sección
    /// 2. Manejar cada DireccionCursor para actualizar la selección
    /// 3. Renderizar el item seleccionado con fondo/color diferente
    /// 4. Renderizar items con hover cuando se pase el ratón (opcional, pero recomendado)
    NavegacionSeccion { id_seccion: String, direccion: DireccionCursor },

    /// Mostrar/ocultar la sidebar (Ctrl+B)
    ToggleSidebar,

    /// Abrir o cerrar la vista previa Markdown en el navegador (Ctrl+Shift+M)
    TogglePreviewMarkdown,
}
