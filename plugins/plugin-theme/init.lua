-- plugin-theme/init.lua
-- Tema One Dark para el editor Glyph
-- Reemplaza los colores predeterminados del host con esta paleta.

local M = {}

M.nombre      = "One Dark"
M.descripcion = "Tema oscuro basado en One Dark de Atom"
M.version     = "1.0.0"

function M.tema()
    return {
        -- Sintaxis
        keyword     = "#C678DD",   -- morado     — fn, let, struct, impl…
        string      = "#98C379",   -- verde      — "texto", 'c'
        comment     = "#5C6370",   -- gris       — // …  /* … */
        ["function"]= "#61AFEF",   -- azul       — nombre de función
        type        = "#E5C07B",   -- amarillo   — tipos, structs, enums
        number      = "#D19A66",   -- naranja    — literales numéricos
        operator    = "#56B6C2",   -- cian       — +, -, =, ::, ->
        variable    = "#E06C75",   -- rojo       — bindings, parámetros
        constant    = "#D19A66",   -- naranja    — constantes, statics
        punctuation = "#ABB2BF",   -- gris claro — {}()[];,.
        attribute   = "#E5C07B",   -- amarillo   — #[derive…]
        default     = "#ABB2BF",   -- gris claro — texto no clasificado
    }
end

return M
