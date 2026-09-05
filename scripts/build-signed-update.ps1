[CmdletBinding()]
param(
    [string]$PrivateKeyPath = (Join-Path $env:USERPROFILE ".tauri/textpilot.key"),
    [switch]$SkipTests
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$projectRoot = Split-Path -Parent $PSScriptRoot
$tauriConfig = Get-Content -Raw -LiteralPath (Join-Path $projectRoot "src-tauri/tauri.conf.json") |
    ConvertFrom-Json
$appVersion = [string]$tauriConfig.version

$resolvedKeyPath = (Resolve-Path -LiteralPath $PrivateKeyPath).Path
$securePassword = Read-Host "Пароль ключа Tauri (Enter, если ключ без пароля)" -AsSecureString
$passwordText = [System.Net.NetworkCredential]::new("", $securePassword).Password

Push-Location $projectRoot
try {
    if (-not $SkipTests) {
        & bun run check
        if ($LASTEXITCODE -ne 0) {
            throw "Проверка Svelte завершилась с ошибкой."
        }

        & cargo test --manifest-path src-tauri/Cargo.toml
        if ($LASTEXITCODE -ne 0) {
            throw "Тесты Rust завершились с ошибкой."
        }
    }

    $env:TAURI_SIGNING_PRIVATE_KEY = $resolvedKeyPath
    $env:TAURI_SIGNING_PRIVATE_KEY_PASSWORD = $passwordText

    & bun run app:build
    if ($LASTEXITCODE -ne 0) {
        throw "Сборка Tauri завершилась с ошибкой."
    }

    $bundleDirectory = Join-Path $projectRoot "src-tauri/target/release/bundle/nsis"
    $installerName = "TextPilot_${appVersion}_x64-setup.exe"
    $installer = Get-Item -LiteralPath (Join-Path $bundleDirectory $installerName) -ErrorAction SilentlyContinue

    if ($null -eq $installer) {
        throw "NSIS-установщик не найден в $bundleDirectory."
    }

    $signaturePath = "$($installer.FullName).sig"
    if (-not (Test-Path -LiteralPath $signaturePath)) {
        throw "Подпись обновления не создана: $signaturePath."
    }

    & (Join-Path $PSScriptRoot "package-update.ps1") -PrivateKeyPath $resolvedKeyPath

    Write-Host ""
    Write-Host "Подписанная сборка готова:"
    Write-Host "  Установщик: $($installer.FullName)"
    Write-Host "  Подпись:    $signaturePath"
}
finally {
    Remove-Item Env:TAURI_SIGNING_PRIVATE_KEY -ErrorAction SilentlyContinue
    Remove-Item Env:TAURI_SIGNING_PRIVATE_KEY_PASSWORD -ErrorAction SilentlyContinue
    $passwordText = $null
    Pop-Location
}
