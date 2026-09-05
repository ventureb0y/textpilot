[CmdletBinding()]
param(
    [string]$PrivateKeyPath = (Join-Path $env:USERPROFILE ".tauri/textpilot.key"),
    [switch]$PrepareOnly
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"
$projectRoot = Split-Path -Parent $PSScriptRoot
$config = Get-Content -Raw -LiteralPath (Join-Path $projectRoot "src-tauri/tauri.conf.json") | ConvertFrom-Json
$installer = Join-Path $projectRoot "src-tauri/target/release/bundle/nsis/TextPilot_$($config.version)_x64-setup.exe"
if (-not (Test-Path -LiteralPath $installer)) { throw "Installer not found: $installer" }
$archive = "$installer.zip"

# Preserve existing archives: a published URL must retain the exact signed bytes.
if (-not (Test-Path -LiteralPath $archive)) {
    Compress-Archive -LiteralPath $installer -DestinationPath $archive -CompressionLevel Optimal
}
$zip = [IO.Compression.ZipFile]::OpenRead($archive)
try {
    if ($zip.Entries.Count -ne 1 -or $zip.Entries[0].FullName -cne [IO.Path]::GetFileName($installer)) {
        throw "The archive must contain only the installer at its root."
    }
    $stream = $zip.Entries[0].Open()
    try { $archiveHash = (Get-FileHash -InputStream $stream -Algorithm SHA256).Hash }
    finally { $stream.Dispose() }
    if ($archiveHash -ne (Get-FileHash -LiteralPath $installer -Algorithm SHA256).Hash) {
        throw "Existing archive differs from the installer. Use a new release version."
    }
}
finally { $zip.Dispose() }

if ($PrepareOnly) { Write-Host "Archive ready (not signed): $archive"; return }

$keyPath = (Resolve-Path -LiteralPath $PrivateKeyPath).Path
$previousPassword = $env:TAURI_SIGNING_PRIVATE_KEY_PASSWORD
$previousKey = $env:TAURI_SIGNING_PRIVATE_KEY
Push-Location $projectRoot
try {
    if ($null -eq $previousPassword) {
        $securePassword = Read-Host "Tauri key password (Enter for no password)" -AsSecureString
        $env:TAURI_SIGNING_PRIVATE_KEY_PASSWORD = [System.Net.NetworkCredential]::new("", $securePassword).Password
    }
    # The build command accepts a path in this variable; signer expects key text.
    $env:TAURI_SIGNING_PRIVATE_KEY = $null
    & bun run tauri signer sign --private-key-path $keyPath $archive
    if ($LASTEXITCODE -ne 0) { throw "Archive signing failed." }
    if (-not (Test-Path -LiteralPath "$archive.sig")) { throw "Archive signature was not created." }
    Write-Host "Upload to GitVerse Releases: $archive"
    Write-Host "Use this signature for latest.json: $archive.sig"
}
finally {
    $env:TAURI_SIGNING_PRIVATE_KEY_PASSWORD = $previousPassword
    $env:TAURI_SIGNING_PRIVATE_KEY = $previousKey
    Pop-Location
}
