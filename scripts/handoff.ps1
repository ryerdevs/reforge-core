# handoff.ps1 - close-of-session handoff (rule 19): updates documentation/progress.md.
# Uso: powershell -ExecutionPolicy Bypass -File scripts\handoff.ps1 -Message "slice 64 done: X"
param([string]$Message)

$path = Join-Path $PSScriptRoot '..\documentation\progress.md'
$stamp = Get-Date -Format 'yyyy-MM-dd HH:mm'
$head = git rev-parse --short HEAD 2>$null; if (-not $head) { $head = 'n/a' }
$status = (git status --short 2>$null) -join '; '; if (-not $status) { $status = 'clean' }
$eol = "`n"
$text = if (Test-Path $path) { [IO.File]::ReadAllText($path, [Text.Encoding]::UTF8) } else { '' }
if ($text.Contains("`r`n")) { $eol = "`r`n" }

$entry = "## Handoff" + $eol + $eol + "- $stamp | HEAD $head | $status"
if ($Message) { $entry += $eol + "- $Message" }

if ($text -notmatch '(?m)^## Handoff') {
    $text = $text.TrimEnd() + $eol + $eol + $entry + $eol
} else {
    $text = [regex]::Replace($text, '(?ms)^## Handoff.*?(?=^## |\z)', $entry + $eol + $eol)
}
# "Last update" or Spanish "?ltimo update" (codepoints keep the source ASCII-safe).
$upd = '(?mi)^(?:last|' + [char]0xDA + 'ltimo|' + [char]0xFA + 'ltimo) update:.*$'
if ($text -match $upd) {
    $text = [regex]::Replace($text, $upd, "Last update: $stamp - handoff.ps1")
} else {
    $text = $text.TrimEnd() + $eol + $eol + "Last update: $stamp - handoff.ps1" + $eol
}
[IO.File]::WriteAllText($path, $text, [Text.UTF8Encoding]::new($false))
Write-Host "handoff written: $path"
