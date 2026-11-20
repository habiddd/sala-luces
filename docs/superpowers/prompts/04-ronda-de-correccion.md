# Ronda de corrección (al mismo implementador)

**Modelo:** el mismo agente Haiku, retomado con SendMessage para que conserve su contexto. **Cuándo:** si la revisión trae hallazgos Críticos o Importantes.

```
Task review found issues in src/modelo.rs. Fix round 1 — fix these, re-run tests, append a fix report (changes, tests, command, output) to your report file task-2-report.md, then reply with the same short status contract. Only edit src/modelo.rs (another agent is editing src/inventario.rs; don't touch it).

Important 1 — `reparar` ignores empty/whitespace-only ids from a loaded file (modelo.rs:330-375). After trimming:
- drop carriles, tomas and luces whose id/código is empty, one warning each;
- a luz with `toma: Some("")` → None with a warning;
- a luz with empty `carril` → reassign it to the first carril (if there are no carriles, create one with `anadir_carril()` first), with a warning. Never create a carril with id "".
- add test `reparar_codigos_vacios` covering these.

Important 2 — missing tests for real `reparar` branches. Add:
- `reparar_quita_duplicados` (duplicate carril id, toma code, luz code — later ones dropped, 3 warnings; include a whitespace-padded duplicate like " C1" vs "C1");
- `reparar_toma_inexistente` (luz.toma refers to a missing toma → None + 1 warning);
- extend the missing-carril case to assert exactly one warning is produced for creating carril "X";
- `ajustar_limites_minimos` (ancho 0.2 → 1.0; altura_carriles 9.0 with alto_techo 4.0 → 4.0; altura_carriles 0.1 → 0.5).

Also (data-loss prevention, controller ruling): make `guardar` atomic — write to "<ruta>.tmp" then `std::fs::rename` over the target; on failure return Err and don't leave the original truncated.
```

**Clave:** cada hallazgo lleva el archivo y la línea, qué cambiar y qué prueba lo demuestra. Las decisiones del coordinador van marcadas como *controller ruling*.
