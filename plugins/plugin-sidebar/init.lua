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

-- ── Helpers ───────────────────────────────────────────────────────

local function extraer_directorio(ruta)
    if not ruta or ruta == "" then return nil end
    -- Encontrar la última barra
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

-- Genera las líneas de la sección a partir del árbol actual.
-- También actualiza lineas_rutas.
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

        -- Color: archivo activo = dorado, subir = magenta, dirs = azul claro, archivos = blanco
        local color
        if nodo.ruta == archivo_activo then
            color = "#E5C07B"  -- dorado
        elseif nodo.subir then
            color = "#C678DD"  -- púrpura claro
        elseif nodo.es_dir then
            color = "#61AFEF"  -- azul
        else
            color = "#ABB2BF"  -- gris claro
        end

        local negrita = (nodo.ruta == archivo_activo)
        table.insert(lineas, { texto = texto, color = color, negrita = negrita })

        -- Solo los archivos (no dirs) tienen ruta para abrir
        if not nodo.es_dir then
            lineas_rutas[i] = nodo.ruta
        end
    end
    return lineas
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
    end
    local lineas = generar_lineas()
    glyph.actualizar_seccion("sidebar", lineas)
end

function M.al_cambiar(ruta)
    if ruta == "" or ruta == archivo_activo then return end
    archivo_activo = ruta
    local lineas = generar_lineas()
    glyph.actualizar_seccion("sidebar", lineas)
end

function M.click_seccion(id, linea_idx)
    -- linea_idx es 0-based desde el renderer; pasamos a 1-based para Lua
    local idx = linea_idx + 1
    local nodo = arbol[idx]
    if not nodo then return end

    if nodo.subir then
        -- Navegar un nivel arriba en la jerarquía
        local padre = nodo.ruta
        if padre and padre ~= directorio_raiz then
            directorio_raiz = padre
            construir_arbol(padre)
        end
    elseif nodo.es_dir then
        -- Toggle expandir/colapsar
        nodo.expandido = not nodo.expandido
        if nodo.expandido then
            -- Insertar hijos después de este nodo
            local hijos = leer_nivel(nodo.ruta, nodo.nivel + 1)
            for i, h in ipairs(hijos) do
                table.insert(arbol, idx + i, h)
            end
        else
            -- Eliminar todos los descendientes
            local i = idx + 1
            while i <= #arbol and arbol[i].nivel > nodo.nivel do
                table.remove(arbol, i)
            end
        end
    else
        -- Abrir el archivo
        glyph.abrir_archivo(nodo.ruta)
    end

    -- Actualizar sidebar
    local lineas = generar_lineas()
    glyph.actualizar_seccion("sidebar", lineas)
end

return M
