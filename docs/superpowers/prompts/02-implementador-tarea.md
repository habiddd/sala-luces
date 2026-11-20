# Implementador de una tarea del plan

**Modelo:** Haiku. **Cuándo:** una tarea por agente, en orden; el brief es la sección de esa tarea copiada del plan junto con la cabecera de restricciones globales.

```
You are implementing Task 2: Data model and rules (`src/modelo.rs`) of a Rust/eframe Windows desktop app ("Sala de luces 3D"): a recording room with ceiling rails (carriles), lights (luces) on rails, and wall outlets (tomas).

## Task Description
Read your task brief first: <SCRATCHPAD>\sdd\task-2-brief.md
It is your requirements, with the exact signatures, values and tests to use verbatim. The design spec is C:\Users\WinterOS\Desktop\LUCES\docs\superpowers\specs\2026-11-03-sala-luces-design.md — its "Modelo de datos" and "Reglas" sections are the authority if the brief is unclear.

## Context
- Work from: C:\Users\WinterOS\Desktop\LUCES. Task 1 created the Cargo project; `src/modelo.rs` is an empty file already declared as `mod modelo;` in main.rs. Only edit `src/modelo.rs`.
- Use `$env:USERPROFILE\.cargo\bin\cargo.exe` (cargo may not be on PATH).
- Not a git repo; skip commits, do not create a repo.
- Nothing uses the module yet: put `#![allow(dead_code)]` as the first line of modelo.rs to keep `cargo build` warning-free (Task 6 removes it).
- Codes/ids are trimmed before storing and comparing; empty after trim is rejected.
- "first free" ids: smallest n ≥ 1 whose id ("C{n}" or "T-{n:02}") is not in use.
- Error messages in Spanish.
- TDD: write the tests first, run `cargo test modelo` to see them fail (compile failure counts), then implement.

## You Do Not Dispatch Subagents
Do all the work yourself.

## When You're in Over Your Head
Report BLOCKED or NEEDS_CONTEXT with specifics.

## Self-review before reporting
Every test in the brief exists with the brief's values; every listed method exists with the exact signature; `cargo test` passes; `cargo build` has zero warnings; nothing extra.

## Report Format
Write your full report to: <SCRATCHPAD>\sdd\task-2-report.md
(implemented items, TDD evidence RED and GREEN with commands and relevant output, files changed, concerns).
Then reply with ONLY (under 15 lines): Status, one-line test summary, concerns, report path.
```

**Variante útil (tareas con APIs nuevas):** añadir un bloque como este:

```
## egui 0.37 API
It is newer than your training data. Before using Painter APIs (...) confirm signatures in
`$env:USERPROFILE\.cargo\registry\src\*\egui-0.37*\src\` and `epaint-0.37*`. Do not guess.
```
