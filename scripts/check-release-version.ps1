[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)]
    [string]$Tag
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$projectRoot = Split-Path -Parent $PSScriptRoot
$normalizedTag = $Tag -replace "^refs/tags/", ""
$expectedVersion = $normalizedTag -replace "^v", ""

if ($expectedVersion -notmatch "^\d+\.\d+\.\d+([+-][0-9A-Za-z.-]+)?$") {
    throw "Тег '$Tag' не содержит корректную SemVer-версию."
}

$packageJson = Get-Content -Raw -LiteralPath (Join-Path $projectRoot "package.json") | ConvertFrom-Json
$tauriConfig = Get-Content -Raw -LiteralPath (Join-Path $projectRoot "src-tauri/tauri.conf.json") | ConvertFrom-Json
$cargoToml = Get-Content -Raw -LiteralPath (Join-Path $projectRoot "src-tauri/Cargo.toml")
$cargoLock = Get-Content -Raw -LiteralPath (Join-Path $projectRoot "src-tauri/Cargo.lock")

$cargoVersionMatch = [regex]::Match($cargoToml, '(?m)^version = "([^"]+)"')
$lockVersionMatch = [regex]::Match(
    $cargoLock,
    '(?ms)\[\[package\]\]\s*name = "textpilot"\s*version = "([^"]+)"'
)

if (-not $cargoVersionMatch.Success -or -not $lockVersionMatch.Success) {
    throw "Не удалось определить версию TextPilot в Cargo-файлах."
}

$versions = [ordered]@{
    "package.json" = [string]$packageJson.version
    "src-tauri/Cargo.toml" = $cargoVersionMatch.Groups[1].Value
    "src-tauri/Cargo.lock" = $lockVersionMatch.Groups[1].Value
    "src-tauri/tauri.conf.json" = [string]$tauriConfig.version
}

$mismatches = @(
    $versions.GetEnumerator() |
        Where-Object { $_.Value -ne $expectedVersion }
)

if ($mismatches.Count -gt 0) {
    $details = $mismatches |
        ForEach-Object { "$($_.Key) содержит версию $($_.Value)" }
    throw "Версии не соответствуют тегу ${normalizedTag}: $($details -join '; ')."
}

Write-Host "Все версии соответствуют тегу $normalizedTag."
