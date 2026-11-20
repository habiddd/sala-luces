# Luces editables en el panel derecho — plan

**Goal:** En el panel derecho, cualquier luz (de techo o de piso) se puede modificar entera, y ningún deslizador se queda bloqueado.

## Causas encontradas (app.rs)
1. `editor_luz` y `editor_luz_piso` muestran las especificaciones con `mostrar_specs`, que es solo lectura: tipo, potencia, lux a 1 m, apertura mín–máx, temperatura mín–máx y CRI. Marca y modelo también son solo texto.
2. En `controles_ajustes`, Apertura y Temperatura se desactivan (`add_enabled(!fija, …)`) cuando el rango de las specs es un solo valor (p. ej. 90–90°, 5600–5600 K). Además `a.limitar(s)` recorta el valor a ese rango.
3. `tocado()` solo cuenta un cambio si el deslizador se arrastró o se pulsó. Si escribes un número en la casilla del deslizador, el cambio se descarta y el valor vuelve atrás. Afecta a Dimmer, Apertura, Temperatura e Inclinación.

## Cambios — solo `src/app.rs`
1. **Specs editables.** Nueva `fn editar_specs(ui, id: &str, s: &mut SpecsLuz, piso: bool) -> bool`: un `egui::Grid` con etiqueta + control por fila. Las filas son "Tipo" (ComboBox con `TipoLuz::TODOS` filtrado por `es_piso() == piso`; al elegir uno, `*s = t.specs()`), "Potencia (W)", "Lux a 1 m", "Apertura mín (°)", "Apertura máx (°)", "Temperatura mín (K)", "Temperatura máx (K)" y "CRI". Usa `campo_specs(..., true)` con los mismos rangos que `celdas_specs`, y después el mismo arreglo de que el máximo nunca baje del mínimo. Si `celdas_specs` y `editar_specs` pueden compartir la lista de campos sin enredarse, hazlo; si no, déjalo duplicado.
   - En `editor_luz` y `editor_luz_piso`: "Marca" y "Modelo" pasan a `ui.text_edit_singleline` sobre el campo de la luz. El Código sigue solo de lectura, porque es el identificador que usan la selección y las tomas. Después, `editar_specs` en lugar de `mostrar_specs`. Si cambian las specs: `ajustes.limitar(&specs)` y `cambio = true`. Borra `mostrar_specs` si queda sin uso.
2. **Deslizadores libres.** En `controles_ajustes`, Apertura usa siempre el rango `1.0..=179.0` y Temperatura `1000.0..=20000.0`; ninguno se desactiva. `controles_ajustes` pasa a recibir `s: &mut SpecsLuz`: si el usuario mueve el valor fuera del rango de las specs, ese rango se amplía para incluirlo (`s.apertura_min = s.apertura_min.min(a.apertura)`, `s.apertura_max = s.apertura_max.max(a.apertura)`, y lo mismo con cct). Así `limitar` no lo devuelve atrás. Los que llaman (`editor_luz`, `editor_luz_piso`) guardan de vuelta las specs y los ajustes.
   - Función pura para probarlo: `fn ampliar_rango(min: f32, max: f32, v: f32) -> (f32, f32)`.
3. **Escribir en los deslizadores.** `tocado` también debe aceptar una edición con teclado en la casilla numérica del deslizador. **Antes, lee cómo `egui::Slider` de 0.37 reporta eso** (`~/.cargo/registry/src/*/egui-0.37*/src/widgets/slider.rs`: la casilla es un DragValue unido a la respuesta; mira qué banderas quedan: `lost_focus`, `has_focus`, `changed`). Añade la condición mínima correcta (probablemente `r.lost_focus() || r.has_focus()`). No pierdas lo que protege el comentario: un redondeo sin que nadie toque el control no cuenta.

## Pruebas (módulo de pruebas de app.rs; si no hay, créalo con `#[cfg(test)]`)
- `ampliar_rango_incluye_valor`: (90, 90, 120) → (90, 120); (90, 90, 40) → (40, 90); (30, 60, 45) → (30, 60).
- `controles_ajustes_amplia_specs`: con un `egui::Context` sin ventana (copia el patrón de la prueba `dibujar_con_camara_dentro_no_falla` de escena.rs), basta comprobar que llamar a `controles_ajustes` con specs 90–90 no entra en pánico. La lógica de ampliar se prueba en la función pura.
