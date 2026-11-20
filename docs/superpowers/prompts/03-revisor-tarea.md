# Revisor de una tarea (spec + calidad)

**Modelo:** Sonnet. **Cuándo:** después de cada tarea, antes de pasar a la siguiente.

```
You are reviewing one task's implementation: first whether it matches its requirements, then whether it is well-built. This is a task-scoped gate, not a merge review.

## What Was Requested
Read the task brief: <SCRATCHPAD>\sdd\task-2-brief.md
Binding spec: C:\Users\WinterOS\Desktop\LUCES\docs\superpowers\specs\2026-11-03-sala-luces-design.md (sections "Modelo de datos" and "Reglas").

Global constraints that bind this task (verbatim from the plan):
- All UI text in Spanish. Units are meters. Axes: X = ancho, Z = largo, Y = up; origin at a floor corner.
- Dependencies: only `eframe`, `serde` (feature `derive`), `serde_json`, `rfd`. Nothing else.
- Codes and ids (luz, carril, toma, equipo) are trimmed; empty is rejected. Uniqueness is checked on the trimmed value.
- Errors are `Result<_, String>` with a Spanish message shown to the user; nothing panics on user input or file content.
- Every task ends with `cargo test` and `cargo build` passing with zero warnings.
- Controller ruling: `#![allow(dead_code)]` at the top of the module is allowed until Task 6.

## What the Implementer Claims
Read: <SCRATCHPAD>\sdd\task-2-report.md

## Code Under Review
There is no git repo. The task's whole change is the single file C:\Users\WinterOS\Desktop\LUCES\src\modelo.rs (new file). Read it in full. Do not modify any file.

## Do Not Trust the Report
Verify claims against the code; implementer rationales don't downgrade findings.

## Tests
The implementer ran them. Run a focused test (`$env:USERPROFILE\.cargo\bin\cargo.exe test <name>` from C:\Users\WinterOS\Desktop\LUCES) only if reading the code raises a specific doubt.

## You Do Not Dispatch Subagents

## Part 1: Spec Compliance — Missing / Extra / Misunderstood, with file:line.
Pay attention to: `reparar` order and warnings (duplicates, missing carril created, dangling toma refs, shared toma, ajustar), codes containing whitespace or empty after trim (also inside a loaded file), `ajustar_a_sala` limits (min sizes 1.0, altura_carriles in [0.5, alto_techo]), rename rejecting empty/duplicate without changing anything, `conectar` rules, save failure returning Err.

## Part 2: Code Quality — error handling, edge cases, tests verifying real behavior, structure.

## Calibration
Critical / Important (task can't be trusted until fixed: incorrect behavior, missed requirement, swallowed errors, tests asserting nothing) / Minor. Acknowledge what's done well.

## Output Format
### Spec Compliance (✅ | ❌ with file:line; ⚠️ cannot verify)
### Strengths
### Issues — Critical / Important / Minor (file:line, what, why, fix)
### Assessment — Task quality: Approved | Needs fixes; 1-2 sentence reasoning.
Begin directly with the spec-compliance verdict; no preamble.
```

**Qué salió:** detectó que un archivo cargado con códigos vacíos no se reparaba (Importante), y que faltaban pruebas para varias ramas de `reparar`.
