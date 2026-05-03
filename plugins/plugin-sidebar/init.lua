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

        -- Color: archivo activo = dorado, foco = azul, subir = magenta, dirs = azul claro, archivos = blanco
        local color
        local fondo = nil
        if nodo.ruta == archivo_activo then
            color = "#E5C07B"  -- dorado
        elseif i == linea_enfocada then
            color = "#FFFFFF"  -- blanco para foco
            fondo = "#3E4451"  -- fondo gris oscuro para foco
        elseif nodo.subir then
            color = "#C678DD"  -- púrpura claro
        elseif nodo.es_dir then
            color = "#61AFEF"  -- azul
        else
            color = "#ABB2BF"  -- gris claro
        end

        local negrita = (nodo.ruta == archivo_activo) or (i == linea_enfocada)
        table.insert(lineas, { texto = texto, color = color, negrita = negrita, fondo = fondo })

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
        linea_enfocada = nil  -- reset foco
    end
    local lineas = generar_lineas()
    glyph.actualizar_seccion("sidebar", lineas)
end

function M.al_cambiar(ruta)
    if ruta == "" or ruta == archivo_activo then return end
    archivo_activo = ruta
    -- Encontrar la línea del archivo activo para enfocar
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
    local nodo = arbol[idx]
    if not nodo then return end

    -- Establecer foco en la línea clickeada
    linea_enfocada = idx

    if nodo.subir then
        -- Navegar un nivel arriba en la jerarquía
        local padre = nodo.ruta
        if padre and padre ~= directorio_raiz then
            directorio_raiz = padre
            construir_arbol(padre)
            linea_enfocada = 1  -- foco en el primer elemento del nuevo directorio
            glyph.mostrar_notificacion("Navegado a directorio: " .. padre, "info")
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
            glyph.mostrar_notificacion("Directorio expandido: " .. nodo.nombre, "info")
        else
            -- Eliminar todos los descendientes
            local i = idx + 1
            while i <= #arbol and arbol[i].nivel > nodo.nivel do
                table.remove(arbol, i)
            end
            glyph.mostrar_notificacion("Directorio colapsado: " .. nodo.nombre, "info")
        end
    else
        -- Abrir el archivo
        glyph.abrir_archivo(nodo.ruta)
    end

    -- Actualizar sidebar
    local lineas = generar_lineas()
    glyph.actualizar_seccion("sidebar", lineas)
end

-- ── Navegación por teclado ────────────────────────────────────────

function M.tecla_seccion(id, tecla, modificadores)
    if #arbol == 0 then return false end

    -- Inicializar foco si no existe
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
        if nodo and nodo.es_dir and nodo.expandido then
            -- Colapsar directorio
            nodo.expandido = false
            local idx = linea_enfocada + 1
            while idx <= #arbol and arbol[idx].nivel > nodo.nivel do
                table.remove(arbol, idx)
            end
            glyph.mostrar_notificacion("Directorio colapsado: " .. nodo.nombre, "info")
            consumido = true
        end
    elseif tecla == "ArrowRight" then
        local nodo = arbol[linea_enfocada]
        if nodo and nodo.es_dir and not nodo.expandido then
            -- Expandir directorio
            nodo.expandido = true
            local hijos = leer_nivel(nodo.ruta, nodo.nivel + 1)
            for i, h in ipairs(hijos) do
                table.insert(arbol, linea_enfocada + i, h)
            end
            glyph.mostrar_notificacion("Directorio expandido: " .. nodo.nombre, "info")
            consumido = true
        end
    elseif tecla == "Enter" or tecla == " " then
        -- Activar elemento (igual que click)
        local nodo = arbol[linea_enfocada]
        if nodo then
            if nodo.subir then
                local padre = nodo.ruta
                if padre and padre ~= directorio_raiz then
                    directorio_raiz = padre
                    construir_arbol(padre)
                    linea_enfocada = 1
                    glyph.mostrar_notificacion("Navegado a directorio: " .. padre, "info")
                end
            elseif nodo.es_dir then
                nodo.expandido = not nodo.expandido
                if nodo.expandido then
                    local hijos = leer_nivel(nodo.ruta, nodo.nivel + 1)
                    for i, h in ipairs(hijos) do
                        table.insert(arbol, linea_enfocada + i, h)
                    end
                    glyph.mostrar_notificacion("Directorio expandido: " .. nodo.nombre, "info")
                else
                    local idx = linea_enfocada + 1
                    while idx <= #arbol and arbol[idx].nivel > nodo.nivel do
                        table.remove(arbol, idx)
                    end
                    glyph.mostrar_notificacion("Directorio colapsado: " .. nodo.nombre, "info")
                end
            else
                glyph.abrir_archivo(nodo.ruta)
                glyph.mostrar_notificacion("Archivo abierto: " .. nodo.nombre, "info")
            end
            consumido = true
        end
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
