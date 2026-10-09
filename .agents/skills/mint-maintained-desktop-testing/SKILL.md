---
name: mint-maintained-desktop-testing
description: Validate the MINT native GUI with existing egui tests or an isolated visible app session, including demo data, themes, scaling, interactions and cleanup.
---

# Desktop testing

Read [desktop-ui.md](../../../docs/architecture/desktop-ui.md) for owners and test boundaries, and [integration.md](../../../docs/architecture/integration.md) before testing game-writing actions.

## Select and isolate the target

- Resolve the exact build, process/window, and task-owned data directory. Start a preview with `cargo run --locked -- --appdata <isolated-directory>`; do not reuse a personal config or another running instance by accident.
- Treat only the chosen disposable directory as a test playground. Keep source fixtures and real game files outside cleanup. `--appdata` does not isolate game writes; inspect/clear the demo's auto-discovered `drg_pak_path` and avoid installation, uninstall, launch, or self-update unless those operations are in scope.
- Use synthetic profiles, groups, priorities, versions, long names, enabled/disabled entries and missing metadata for demos. Keep provider credentials empty. UI placeholder paks do not prove successful pak parsing or integration.

## Exercise and report

1. Extend the existing `src/gui/tests.rs` harness for sorting, target selection, clicks, saved state, or layout behavior it can observe. Use existing mock providers for deterministic network-error cases; do not introduce an automation framework merely to reproduce a supported test.
2. For visible testing, confirm the intended window and state before acting or capturing evidence. Cover affected states, both themes, meaningful display scaling, resizing, keyboard/focus behavior and error paths. Preserve the user's normal desktop settings.
3. Keep screenshots limited to useful, confirmed states. Never substitute an illustration or a screenshot from another build for actual validation.
4. Stop only task-owned processes, unless the user asks to keep the app open. Delete only confirmed task-owned temporary data after its process exits; retain requested comparison images or demo sessions.
5. Report the build, data isolation, actions, observed results and gaps. Distinguish headless egui tests, visible desktop interaction, provider tests, and actual in-game validation.
