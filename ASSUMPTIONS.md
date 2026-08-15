# ASSUMPTIONS (loop "Base jugable", 2026-08-15)

## Contexto verificado

- Los 5 bugs del plan están arreglados y commiteados: workspace **584 passed / 0 failed**
  (verificado con `powershell -NoProfile -ExecutionPolicy Bypass -File scripts\loop_check.ps1`
  → `SCORE: 584, passed=584, failed=0`).
- El **goal check del loop falla por ENTORNO, no por el código**: pi-loop-mode ejecuta
  `pi.exec("bash", ["-lc", state.checkCommand])` (extensions/index.ts:389) y en Windows
  `bash` resuelve al shim de WSL (`C:\Windows\System32\bash.exe`) que falla porque
  **no hay ninguna distro WSL instalada** en esta máquina. El shim de WSL gana aunque
  Git Bash esté instalado (CreateProcess busca System32 antes que el PATH).

## Asunciones tomadas

1. **El check del loop se cambia a PowerShell** (el operador lo pidió explícitamente:
   "usa powershell y ya"). El script `scripts/loop_check.ps1` es el check canónico.
2. **Cómo actualizar el loop actual** (operador, en la TUI de Pi):
   - `Esc` (pausa) → `/loop goal <el mismo goal> --check "powershell -NoProfile -ExecutionPolicy Bypass -File scripts\loop_check.ps1"` → `/loop resume`
   - O reiniciar el loop con el mismo `--check` en PowerShell.
3. Se creó `bash.cmd` en la raíz (shim local bash → Git Bash) y se añadió
   `C:\Program Files\Git\usr\bin` al PATH de USUARIO — útiles para procesos futuros;
   el loop en marcha ya capturó su PATH al arrancar, por eso no los ve.

## Si el loop no puede cambiar el check

- La alternativa es instalar una distro WSL (`wsl --install -d Debian`) — NO se hizo
  (WSL está capado a 1 GB y es solo oracle box; el usuario no lo quiere para esto).
- El trabajo del loop (los 5 bugs) está COMPLETO y verificado; el check es el único
  bloqueo restante y es de entorno.
