# ==============================================================================
# Skrypt automatycznej synchronizacji kodu do wersji publicznej (SQLite)
#
# Cel:
#   Kopiuje wszystkie wspólne moduły (TUI, agent, providers, syntax, vsix, repomap itd.)
#   z głównego repozytorium do podkatalogu `sqlite_version/`.
#
# ZASADA BEZWZGLĘDNA:
#   Pliki `src/database.rs` oraz `src/archival.rs` w `sqlite_version/` SĄ OMIJANE,
#   aby nie nadpisać silnika SQLite kodem prywatnej bazy DevFortDB!
# ==============================================================================

param(
    [switch]$NoCheck
)

$ErrorActionPreference = "Stop"

$Root = Resolve-Path "$PSScriptRoot\.."
$SrcDir = Join-Path $Root "src"
$TestsDir = Join-Path $Root "tests"
$SqliteDir = Join-Path $Root "sqlite_version"
$SqliteSrcDir = Join-Path $SqliteDir "src"
$SqliteTestsDir = Join-Path $SqliteDir "tests"

if (-not (Test-Path $SqliteDir)) {
    Write-Error "Nie znaleziono katalogu sqlite_version w: $SqliteDir"
}

Write-Host "🔄 Synchronizacja wspólnego kodu do sqlite_version..." -ForegroundColor Cyan

# Lista plików wykluczonych (unikalne dla silnika bazy danych):
$ExcludedFiles = @("database.rs", "archival.rs")

$CopiedCount = 0
$SkippedCount = 0

Get-ChildItem -Path $SrcDir -Recurse -File | ForEach-Object {
    $RelativePath = $_.FullName.Substring($SrcDir.Length).TrimStart("\", "/")
    $FileName = $_.Name

    if ($ExcludedFiles -contains $FileName) {
        Write-Host "  🛡️  Pominięto silnik bazodanowy: $RelativePath" -ForegroundColor Yellow
        $SkippedCount++
        return
    }

    $TargetFile = Join-Path $SqliteSrcDir $RelativePath
    $TargetParent = Split-Path $TargetFile -Parent

    if (-not (Test-Path $TargetParent)) {
        New-Item -ItemType Directory -Path $TargetParent -Force | Out-Null
    }

    Copy-Item -Path $_.FullName -Destination $TargetFile -Force
    $CopiedCount++
}

# Synchronizacja katalogu tests/
if (Test-Path $TestsDir) {
    if (-not (Test-Path $SqliteTestsDir)) {
        New-Item -ItemType Directory -Path $SqliteTestsDir -Force | Out-Null
    }
    Copy-Item -Path "$TestsDir\*" -Destination $SqliteTestsDir -Recurse -Force
}

# Synchronizacja wtyczki bridge-extension (dla VS Code / Cursor / Windsurf / Trae)
$BridgeSrc = Join-Path $Root "bridge-extension"
$BridgeDest = Join-Path $SqliteDir "bridge-extension"
if (Test-Path $BridgeSrc) {
    if (-not (Test-Path $BridgeDest)) {
        New-Item -ItemType Directory -Path $BridgeDest -Force | Out-Null
    }
    Copy-Item (Join-Path $BridgeSrc "package.json") $BridgeDest -Force
    Copy-Item (Join-Path $BridgeSrc "tsconfig.json") $BridgeDest -Force
    Copy-Item "$BridgeSrc\*.vsix" $BridgeDest -Force
    Copy-Item -Recurse "$BridgeSrc\src" $BridgeDest -Force
    Copy-Item -Recurse "$BridgeSrc\out" $BridgeDest -Force
}

# Synchronizacja dokumentacji
Copy-Item -Path (Join-Path $Root "DISTRIBUTION.md") -Destination (Join-Path $SqliteDir "DISTRIBUTION.md") -Force

Write-Host "✅ Zsynchronizowano $CopiedCount plików (pominięto $SkippedCount bazodanowych)." -ForegroundColor Green

if (-not $NoCheck) {
    Write-Host "🧪 Weryfikacja kompilacji w sqlite_version (cargo check)..." -ForegroundColor Cyan
    Push-Location $SqliteDir
    try {
        cargo check
        if ($LASTEXITCODE -eq 0) {
            Write-Host "✨ sqlite_version kompiluje się w 100% poprawnie!" -ForegroundColor Green
        } else {
            Write-Error "Błąd kompilacji w sqlite_version!"
        }
    } finally {
        Pop-Location
    }
}
