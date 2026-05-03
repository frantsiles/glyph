-- plugin-theme/init.lua
-- Tema Wallbash para el editor Glyph
-- Paleta derivada del tema wallbash (fondo #1B1A29, acento #F0AAAC).

local M = {}

M.nombre      = "Wallbash"
M.descripcion = "Tema oscuro azul-violeta basado en wallbash"
M.version     = "1.0.0"

-- Permisos declarados: solo UI (cambiar colores del tema)
M.permisos = { ui = true }

function M.tema()
    return {
        keyword     = "#F0AAAC",   -- rosa-coral      — fn, let, struct, impl…
        string      = "#CCDDFF",   -- azul claro       — "texto", 'c'
        comment     = "#7A8CB4",   -- azul medio        — // …  /* … */  (~4:1 sobre #1E1E2E)
        ["function"]= "#AFAAF0",   -- lavanda           — nombre de función
        type        = "#9AD0E6",   -- cian suave        — tipos, structs, enums
        number      = "#AADCF0",   -- cian claro        — literales numéricos
        operator    = "#AAC1F0",   -- azul medio        — +, -, =, ::, ->
        variable    = "#FFFFFF",   -- blanco            — bindings, parámetros
        constant    = "#AADCF0",   -- cian claro        — constantes, statics
        punctuation = "#7A92C2",   -- azul-gris         — {}()[];,.
        attribute   = "#AADCF0",   -- cian claro        — #[derive…]
        default     = "#FFFFFF",   -- blanco            — texto no clasificado
    }
end

return M
