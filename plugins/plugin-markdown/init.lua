-- plugin-markdown
-- Preview de Markdown con soporte Mermaid — Glyph

local M = {}

M.nombre = "markdown"
M.permisos = { ui = true }

local seccion_registrada = false
local preview_activa = false

local function actualizar_barra()
  local icono = preview_activa and "●" or "○"
  local accion = preview_activa and "activo · click para cerrar" or "click para abrir · Ctrl+Shift+M"
  glyph.actualizar_seccion("md-barra", {
    {
      texto = "  " .. icono .. " Markdown Preview  ·  " .. accion,
      color = preview_activa and "#a6e3a1" or "#89b4fa",
    }
  })
end

function M.al_abrir(ruta)
  if not ruta or ruta == "" then return end
  local ext = ruta:match("%.(%w+)$") or ""
  if (ext == "md" or ext == "markdown") and not seccion_registrada then
    seccion_registrada = true
    glyph.registrar_seccion({
      id = "md-barra",
      lado = "abajo",
      tamano = 28,
      color_fondo = "#181825",
    })
    actualizar_barra()
  end
end

function M.click_seccion(id, linea)
  if id == "md-barra" then
    preview_activa = not preview_activa
    actualizar_barra()
    glyph.toggle_preview_md()
  end
end

function M.tecla_seccion(id, tecla, modificadores)
  if id == "md-barra" and tecla == "Enter" then
    preview_activa = not preview_activa
    actualizar_barra()
    glyph.toggle_preview_md()
  end
end

return M
