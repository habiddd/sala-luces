# Re-revisión acotada

**Modelo:** Sonnet. **Cuándo:** después de cada ronda de corrección; solo mira los hallazgos, no vuelve a revisar todo.

```
Scoped re-review of a fix round. Read-only: do not modify files, do not spawn subagents.

File: C:\Users\WinterOS\Desktop\LUCES\src\modelo.rs (no git; read the `reparar` and `guardar` functions and the tests module). Brief: <SCRATCHPAD>\sdd\task-2-brief.md. Implementer report with fix report appended: <SCRATCHPAD>\sdd\task-2-report.md

Findings to verdict, each ADDRESSED or NOT ADDRESSED with file:line:
1. `reparar` must, after trimming, drop carriles/tomas/luces with empty id/código (one warning each); luz.toma Some("") → None with warning; luz with empty carril → reassigned to the first carril (creating one via anadir_carril if none) with warning; never create a carril with id "". Test `reparar_codigos_vacios` covers it.
2. Tests added: `reparar_quita_duplicados` (carril/toma/luz duplicates incl. whitespace-padded, 3 warnings), `reparar_toma_inexistente` (1 warning), missing-carril case asserts exactly one warning, `ajustar_limites_minimos` (ancho 0.2→1.0; altura_carriles 9.0 with techo 4.0 → 4.0; 0.1 → 0.5). Tests must assert real values.
3. `guardar` atomic: writes "<ruta>.tmp" then `std::fs::rename` over target; Err on failure, original not truncated.

Also flag NEW Critical/Important breakage introduced in these functions only (e.g. order of operations in reparar now wrong, warnings miscounted, panics). Out-of-scope observations → list as "deferred minor".

Output: one line per finding with verdict, then "New breakage:" list or "none", then "Verdict: all findings addressed" or "open findings remain". No preamble.
```
