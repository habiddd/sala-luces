# Revisión final de todo el proyecto

**Modelo:** Opus (el más capaz). **Cuándo:** con todas las tareas hechas, antes de entregar.

```
You are doing the final whole-project review of a small Rust/eframe 0.37 Windows desktop app before it is handed to the user. Read-only: modify nothing; spawn no subagents.

Project: C:\Users\WinterOS\Desktop\LUCES (no git). Code: src/main.rs, src/modelo.rs, src/inventario.rs, src/geom.rs, src/escena.rs, src/app.rs (read all in full). Spec (binding): docs/superpowers/specs/2026-11-03-sala-luces-design.md. Plan: docs/superpowers/plans/2026-11-03-sala-luces.md.
Ledger with rulings, deferred minors and a manual UI test the controller already ran: docs/superpowers/plans/2026-11-03-sala-luces-progreso.md — triage the "minor (deferred)" and "minor (controller visual)" lines: which must be fixed before handing over.

Each task already passed a task-scoped review; 41 tests pass and `cargo build` has zero warnings (don't re-run the suite unless a specific doubt needs one focused test: `$env:USERPROFILE\.cargo\bin\cargo.exe test <name>`).

Focus on what task reviews can't see:
- cross-module consistency (app uses model rules correctly; every mutation path keeps invariants: one luz per toma, unique codes, positions clamped, sucio set);
- spec requirements that fell between tasks (walk the spec's Reglas and Interfaz lists one by one and point to the code that implements each, or report it missing);
- data-loss risks (unsaved guard, save atomicity, inventory protection);
- panics reachable from user input or file content;
- anything a user of this app would hit in the first ten minutes.

Keep the report compact. Output:
### Spec walk (one line per requirement: ✅ file:line | ❌ what's missing)
### Issues — Critical / Important / Minor (file:line, what, why, fix)
### Deferred minors triage (fix now / leave, one line each)
### Verdict — Ready | Needs fixes, 1-2 sentences.
No preamble.
```

**Qué salió:** encontró que la app redibujaba sin parar y que un carril saltaba de lado al arrastrarlo lejos de su centro. Ninguna de las dos cosas la podía ver una revisión por tarea.
