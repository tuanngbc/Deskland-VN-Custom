# Durable development handoff

UTC: 2026-11-21 00:26. Inspected SHA: `1d5d4c99a868fd4a475b1a1e160976d19ea29d35`; this documentation checkpoint has a later SHA.
Branch: `codex/rust-dock`, [Draft PR #1](https://github.com/tuanngbc/Deskland-VN-Custom/pull/1). Base: `codex/rust-test`, accepted `a604519`.

Owner assigned implementation/commit/push/CI in 21/11 handover, superseding old read-only audit. Previous AI writer paused. D9 requires one integrated Deskland product, not companion demos. No merge/deployment/owner test before safe RC and OWNER_NATIVE_ACCEPTED.

## Checkpoint scope

D0 lifecycle/resource/API audit, five living documents, D9 architecture/gate. No runtime changes in this checkpoint. Stable checkout/binary/profile untouched.

Verified CI: [44839614362](https://github.com/tuanngbc/Deskland-VN-Custom/actions/runs/44839614362), SUCCESS at inspected SHA. 42 Rust (seven Dock included), five existing native separately passed. JS 32 renderer + seven shortcuts. No Dock GUI evidence. This docs checkpoint needs own CI before D0 closes.

## Next in order

CI correction: source 16b73d0 failed [44845761358](https://github.com/tuanngbc/Deskland-VN-Custom/actions/runs/44845761358) on `DWMWA_CLOAKED` i32 vs API u32. Explicit cast fixed in following commit. Also preserve Stage HWNDs when only Dock toggles (and vice versa), update Stage side by repositioning rather than rebuilding both modules. Native lifecycle assertion strengthened; workflow bounded at 20 minutes. Await replacement CI before PASS.

Foundation code increment after D0: managed same-process runtime and canonical settings/tray/UI; independent wallpaper toggle and legacy migration; coalesced hooks/cache/work-area refresh; local activation/minimized/cloaked fixes. Source is the commit containing this entry (parent docs 45bacf4). Local JS 32+7+4 PASS; Rust and new isolated native lifecycle test require this increment's CI. D0 own CI is now PASS: [44844337708](https://github.com/tuanngbc/Deskland-VN-Custom/actions/runs/44844337708). Do not treat partial D1/D9 as milestone completion.

1. Verify checkpoint CI, then D1 local/global selection, collector/filtering, coalesced lifecycle model.
2. D9 main-owned configuration/migration, managed runtime, Settings/tray roundtrip, independent module controls and cleanup. Default new modules off.
3. D2/D3 UI/cache/previews/workspaces; D4/D5 reversible actions and monitor/DPI recovery, each CI tested.
4. D6 classified statuses, D7 safety review, D8 regression/performance/RC.

All interactive Dock/Stage acceptance NATIVE_PENDING. OWNER ACTION: NONE. D7 safe tray parity unresolved; keep Compatibility Mode and report blocker, no invented/private Shell API.

Build only in `.github/workflows/build-rust-test.yml`. No local Rust/SDK install or candidate EXE download/run. Push only user repository development branch; no force/history reset. Do not touch installed E:\deskland EXE, baseline or %USERPROFILE%\.deskland-rs. No credentials/target/logs in git. Next session fetch/read actual HEAD and this file before edits.
