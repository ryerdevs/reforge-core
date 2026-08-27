---
Type: Guardrail
Status: Current
Audience: Contributors, agents
Last verified: 2026-08-10
---

# Guardrail: world-entry crash (0xC0000374) — postmortem

Postmortem of the client heap-corruption crash that blocked world entry (2026-08-08 → 2026-08-09). **Closed: field test 2/2.** Full narrative and evidence: `../../AGENTS.md` "World-entry crash" section and `../../CHANGELOG.md` (2026-08-09 sessions).

## Timeline

| Date | Event |
|---|---|
| 2026-08-08 | Intermittent client crash `0xC0000374` (heap corruption) ~8–17s after `player_load`, ~75% of entries; identical WER signature since 15:00 |
| 2026-08-08 (session 2) | Deterministic part found: both characters in DB with garbage coordinates `(960155, 269313)`/`(960970, 271421)` on map 41 → `UPDATE player SET x=969600, y=278400` (units) |
| 2026-08-09 (session 3) | Root cause of the intermittent part found via the client's own minidumps: over-read in `string_replace_word` (`PythonSkill.cpp:62`) → corrupted skill formulas in `m_SkillDataMap` → heap corruption on world entry |
| 2026-08-09 (session 3, 4th part) | Fix deployed (bounds check, build C7EAD7CC) + coordinates fixed → **field test 2/2 consecutive entries — CLOSED** |

## Root cause

`string_replace_word` did `memcmp(base + cur, src, src_len)` WITHOUT checking `cur + src_len <= base_len` → over-read past the end of the string `base` (a `std::string` in `TokenVector[POINT_POLY]` from parsing `SkillTable.txt` in the character-select phase). A garbage read could spuriously "match" tokens → corrupted skill formulas → heap corruption on evaluation at world entry.

## Fix

2-line bounds check `cur + src_len <= base_len` before the `memcmp` (`PythonSkill.cpp:72-90`). Rebuild Release|Win32 → `C:\projects\metin2-extra\client\metin2client.exe` 5,115,904 B, hash `C7EAD7CC...`. Evidence: exception `0xC0000005` in `string_replace_word` at RVA 0x95110 with ECX=0x96510FFD (garbage pointer) in `C:\projects\metin2-extra\client\logs\metin2client_*.dmp` (EterExceptionFilter).

## Diagnostic lessons (do not repeat)

- **Rule:** the server syserr will NEVER see client crashes (local memory; the server only sees the socket close). Client close errors are in `C:\projects\metin2-extra\client\logs\*.dmp` (binary; parse with dumpbin/cdb or the session's `parse_dump3.py`).
- **Rule:** App Verifier Heaps changes the detection timing (guard pages detect the over-read at the write) — useful to isolate, not to reproduce the original symptom.
- **Rule:** the cdb/WER detectors (`granny2.dll`, `igc32.dll`/`igdumdim32.dll`) were **victims**, not causes — different detectors, same corrupted heap. Never chase the detector.
- **Rule:** character coordinates in the DB are **units**, not cells (see [`data-and-encoding.md`](data-and-encoding.md) §5).

## Status

- **Rule:** treat any new `0xC0000374` on world entry as a *data or parsing* bug first (coordinates, skill table parse) — the historical detectors are symptoms.
- **Status:** Closed (2026-08-09, field test 2/2). Tools remain installed for future diagnostics (Debugging Tools + LocalDumps `C:\dumps` + PageHeap via gflags).
- **Evidence links:** [`../../AGENTS.md`](../../AGENTS.md) (World-entry crash section), [`../../CHANGELOG.md`](../../CHANGELOG.md) (2026-08-09 3rd session parts 2–4).
