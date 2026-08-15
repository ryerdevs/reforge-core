@echo off
rem Shim local: reenvia `bash` a Git Bash real (el shim WSL falla sin distro).
"C:\Program Files\Git\usr\bin\bash.exe" %*
