$ErrorActionPreference = "Stop"

$root = Resolve-Path "$PSScriptRoot\..\.."
$richclip = Join-Path $root "target\debug\richclip.exe"
$cargo = (Get-Command cargo -ErrorAction SilentlyContinue).Source
if (-not $cargo) {
    $cargo = Join-Path $HOME ".cargo\bin\cargo.exe"
}

function Assert-Equal {
    param(
        [string]$Expected,
        [string]$Actual,
        [string]$Message
    )

    if ($Expected -cne $Actual) {
        throw "$Message`nExpected: '$Expected'`nActual:   '$Actual'"
    }
}

& $cargo build --quiet
if ($LASTEXITCODE -ne 0) {
    throw "Failed to build richclip"
}

"richclip unicode ✓" | & $richclip copy --one-shot
if ($LASTEXITCODE -ne 0) {
    throw "Failed to copy Unicode text"
}
Assert-Equal "richclip unicode ✓" (& $richclip paste) "richclip could not read its own Unicode text"
Assert-Equal "richclip unicode ✓" (Get-Clipboard -Raw).TrimEnd() "Windows could not read richclip Unicode text"

Set-Clipboard -Value "windows unicode ✓"
Assert-Equal "windows unicode ✓" (& $richclip paste) "richclip could not read Windows Unicode text"

"<b>richclip html</b>" | & $richclip copy --type text/plain --type text/html
if ($LASTEXITCODE -ne 0) {
    throw "Failed to copy multiple formats"
}
$types = & $richclip paste --list-types
if ($types -notcontains "text/plain;charset=utf-8" -or $types -notcontains "text/html") {
    throw "Expected text and HTML formats, got: $($types -join ', ')"
}
Assert-Equal "<b>richclip html</b>" (& $richclip paste --type text/html) "HTML round trip failed"

"custom-data" | & $richclip copy --type application/x-richclip-test
if ($LASTEXITCODE -ne 0) {
    throw "Failed to copy a custom format"
}
Assert-Equal "custom-data" (& $richclip paste --type application/x-richclip-test) "Custom format round trip failed"
