-- Copyright 2026 Franz (frantsiles)
-- Licensed under the Apache License, Version 2.0
--
-- plugin-sidebar: explorador de archivos para Glyph
--
-- Muestra el árbol del directorio activo con carpetas expandibles.
-- Click en un archivo → lo abre en un tab nuevo.
-- Ctrl+B → toggle sidebar.

local M = {}

M.nombre      = "sidebar"
M.descripcion = "Explorador de archivos"
M.permisos    = { ui = true, leer_archivos = true }

-- ── Estado interno ────────────────────────────────────────────────

-- Árbol: lista de nodos en orden de renderizado
-- Cada nodo: { ruta, nombre, es_dir, nivel, expandido }
local arbol = {}
-- Mapa línea (1-based) → ruta de archivo (nil para carpetas)
local lineas_rutas = {}
-- Directorio raíz actual
local directorio_raiz = nil
-- Archivo activo (para resaltarlo)
local archivo_activo = nil
-- Línea enfocada para navegación por teclado (1-based)
local linea_enfocada = nil

-- ── Helpers ───────────────────────────────────────────────────────

local function extraer_directorio(ruta)
    if not ruta or ruta == "" then return nil end
    local pos = ruta:match("^(.*)/[^/]*$")
    return pos
end

local function leer_nivel(dir, nivel)
    if not dir then return {} end
    local entradas = glyph.leer_directorio(dir)
    local nodos = {}
    for _, e in ipairs(entradas) do
        table.insert(nodos, {
            ruta      = e.ruta,
            nombre    = e.nombre,
            es_dir    = e.es_dir,
            nivel     = nivel,
            expandido = false,
        })
    end
    return nodos
end

local function extraer_padre(dir)
    if not dir or dir == "" then return nil end
    if dir == "/" then return nil end
    local padre = dir:match("^(.*)/[^/]+$")
    if not padre or padre == "" then
        return "/"
    end
    return padre
end

local function construir_arbol(dir)
    arbol = {}
    if not dir then return end

    local padre = extraer_padre(dir)
    if padre then
        table.insert(arbol, {
            ruta      = padre,
            nombre    = "..",
            es_dir    = true,
            nivel     = 0,
            expandido = false,
            subir     = true,
        })
    end

    -- Nodo raíz
    local nombre_raiz = dir:match("[^/]+$") or dir
    table.insert(arbol, {
        ruta      = dir,
        nombre    = nombre_raiz,
        es_dir    = true,
        nivel     = 0,
        expandido = true,
    })
    -- Hijos del primer nivel
    local hijos = leer_nivel(dir, 1)
    for _, h in ipairs(hijos) do
        table.insert(arbol, h)
    end
end

-- Busca el índice del nodo padre de arbol[idx] retrocediendo por nivel.
local function buscar_padre(idx)
    local nivel = arbol[idx].nivel
    if nivel == 0 then return nil end
    for i = idx - 1, 1, -1 do
        if arbol[i].nivel < nivel then
            return i
        end
    end
    return nil
end

-- Genera las líneas de la sección a partir del árbol actual.
-- El foco se comprueba primero para que el fondo resaltado aparezca siempre;
-- dentro del bloque de foco se aplica el color específico del tipo de nodo.
local function generar_lineas()
    lineas_rutas = {}
    local lineas = {}
    for i, nodo in ipairs(arbol) do
        local sangria = string.rep("  ", nodo.nivel)
        local prefijo
        if nodo.subir then
            prefijo = "▴ "
        elseif nodo.es_dir then
            prefijo = nodo.expandido and "▾ " or "▸ "
        else
            prefijo = "  "
        end
        local texto = sangria .. prefijo .. nodo.nombre

        local color, fondo, negrita

        if i == linea_enfocada then
            -- El foco siempre muestra fondo. El color del texto depende del tipo.
            fondo   = "#3E4451"
            negrita = true
            if nodo.ruta == archivo_activo then
                color = "#E5C07B"   -- dorado: archivo activo bajo el foco
            elseif nodo.subir then
                color = "#C678DD"   -- púrpura: ".." bajo el foco
            elseif nodo.es_dir then
                color = "#61AFEF"   -- azul: carpeta bajo el foco
            else
                color = "#FFFFFF"   -- blanco: archivo bajo el foco
            end
        elseif nodo.ruta == archivo_activo then
            color   = "#E5C07B"
            negrita = true
        elseif nodo.subir then
            color   = "#C678DD"
        elseif nodo.es_dir then
            color   = "#61AFEF"
        else
            color   = "#ABB2BF"
        end

        table.insert(lineas, { texto = texto, color = color, negrita = negrita, fondo = fondo })

        -- Solo los archivos (no dirs) tienen ruta para abrir
        if not nodo.es_dir then
            lineas_rutas[i] = nodo.ruta
        end
    end
    return lineas
end

-- Activa el nodo en idx: abre archivos, navega hacia el padre o alterna carpetas.
-- con_notif: true para acciones de ratón (donde la notificación ayuda),
--            false para teclado (la respuesta visual del árbol es suficiente).
local function activar_nodo(idx, con_notif)
    local nodo = arbol[idx]
    if not nodo then return end

    if nodo.subir then
        local padre = nodo.ruta
        if padre and padre ~= directorio_raiz then
            directorio_raiz = padre
            construir_arbol(padre)
            linea_enfocada = 1
            if con_notif then
                glyph.mostrar_notificacion("Navegado a: " .. padre, "info")
            end
        end
    elseif nodo.es_dir then
        nodo.expandido = not nodo.expandido
        if nodo.expandido then
            local hijos = leer_nivel(nodo.ruta, nodo.nivel + 1)
            for i, h in ipairs(hijos) do
                table.insert(arbol, idx + i, h)
            end
        else
            local i = idx + 1
            while i <= #arbol and arbol[i].nivel > nodo.nivel do
                table.remove(arbol, i)
            end
        end
    else
        glyph.abrir_archivo(nodo.ruta)
        if con_notif then
            glyph.mostrar_notificacion("Abierto: " .. nodo.nombre, "info")
        end
    end
end

-- ── Hooks ─────────────────────────────────────────────────────────

function M.inicializar()
    glyph.registrar_seccion({
        id          = "sidebar",
        lado        = "izquierda",
        tamano      = 240,
        color_fondo = "#181825",
    })
    -- Sin directorio activo, la sidebar muestra un mensaje de bienvenida
    local lineas = {
        { texto = "EXPLORADOR", color = "#7AA2F7", negrita = true },
        { texto = "" },
        { texto = "  Abre un archivo" },
        { texto = "  para ver el árbol." },
    }
    glyph.actualizar_seccion("sidebar", lineas)
end

function M.al_abrir(ruta)
    if ruta == "" then return end
    archivo_activo = ruta
    local dir = extraer_directorio(ruta)
    if dir and dir ~= directorio_raiz then
        directorio_raiz = dir
        construir_arbol(dir)
        linea_enfocada = nil  -- reset foco al cambiar de directorio
    end
    local lineas = generar_lineas()
    glyph.actualizar_seccion("sidebar", lineas)
end

function M.al_cambiar(ruta)
    if ruta == "" or ruta == archivo_activo then return end
    archivo_activo = ruta
    -- Sincronizar el foco con el archivo que acaba de activarse
    for i, nodo in ipairs(arbol) do
        if nodo.ruta == ruta then
            linea_enfocada = i
            break
        end
    end
    local lineas = generar_lineas()
    glyph.actualizar_seccion("sidebar", lineas)
end

function M.click_seccion(id, linea_idx)
    -- linea_idx es 0-based desde el renderer; pasamos a 1-based para Lua
    local idx = linea_idx + 1
    if not arbol[idx] then return end

    linea_enfocada = idx
    activar_nodo(idx, true)

    local lineas = generar_lineas()
    glyph.actualizar_seccion("sidebar", lineas)
end

-- ── Navegación por teclado ────────────────────────────────────────
--
-- ArrowUp / ArrowDown  — mover foco una línea
-- ArrowLeft            — colapsar carpeta expandida; si no, saltar al padre
-- ArrowRight           — expandir carpeta colapsada; si ya expandida, entrar al primer hijo
-- Enter / Space        — activar nodo (abrir archivo, navegar al padre, alternar carpeta)
-- Home / End           — saltar al primer / último nodo
-- PageUp / PageDown    — saltar 10 líneas arriba / abajo

function M.tecla_seccion(id, tecla, modificadores)
    if #arbol == 0 then return false end

    -- Inicializar foco si no existe o está fuera de rango
    if not linea_enfocada or linea_enfocada > #arbol then
        linea_enfocada = 1
    end

    local consumido = false

    if tecla == "ArrowUp" then
        linea_enfocada = math.max(1, linea_enfocada - 1)
        consumido = true

    elseif tecla == "ArrowDown" then
        linea_enfocada = math.min(#arbol, linea_enfocada + 1)
        consumido = true

    elseif tecla == "ArrowLeft" then
        local nodo = arbol[linea_enfocada]
        if nodo and nodo.es_dir and nodo.expandido and not nodo.subir then
            -- Colapsar carpeta expandida
            nodo.expandido = false
            local i = linea_enfocada + 1
            while i <= #arbol and arbol[i].nivel > nodo.nivel do
                table.remove(arbol, i)
            end
        else
            -- Saltar al nodo padre en el árbol
            local padre_idx = buscar_padre(linea_enfocada)
            if padre_idx then
                linea_enfocada = padre_idx
            end
        end
        consumido = true

    elseif tecla == "ArrowRight" then
        local nodo = arbol[linea_enfocada]
        if nodo and nodo.es_dir and not nodo.subir then
            if not nodo.expandido then
                -- Expandir carpeta colapsada
                nodo.expandido = true
                local hijos = leer_nivel(nodo.ruta, nodo.nivel + 1)
                for i, h in ipairs(hijos) do
                    table.insert(arbol, linea_enfocada + i, h)
                end
            else
                -- Carpeta ya expandida: mover el foco al primer hijo
                local siguiente = linea_enfocada + 1
                if siguiente <= #arbol and arbol[siguiente].nivel > nodo.nivel then
                    linea_enfocada = siguiente
                end
            end
            consumido = true
        end

    elseif tecla == "Enter" or tecla == " " then
        activar_nodo(linea_enfocada, false)
        consumido = true

    elseif tecla == "Home" then
        linea_enfocada = 1
        consumido = true

    elseif tecla == "End" then
        linea_enfocada = #arbol
        consumido = true

    elseif tecla == "PageUp" then
        linea_enfocada = math.max(1, linea_enfocada - 10)
        consumido = true

    elseif tecla == "PageDown" then
        linea_enfocada = math.min(#arbol, linea_enfocada + 10)
        consumido = true
    end

    if consumido then
        local lineas = generar_lineas()
        glyph.actualizar_seccion("sidebar", lineas)
    end

    return consumido
end

return M
