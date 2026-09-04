[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)]
    [string]$Version,

    [Parameter(Mandatory = $true)]
    [string]$InstallerUrl,

    [Parameter(Mandatory = $true)]
    [string]$SignaturePath,

    [string]$Notes = "Исправления и улучшения TextPilot.",

    [string]$OutputPath = ""
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$projectRoot = Split-Path -Parent $PSScriptRoot
$normalizedVersion = $Version -replace "^v", ""

if ($normalizedVersion -notmatch "^\d+\.\d+\.\d+([+-][0-9A-Za-z.-]+)?$") {
    throw "Версия '$Version' не соответствует SemVer."
}

$parsedUrl = $null
if (
    -not [Uri]::TryCreate($InstallerUrl, [UriKind]::Absolute, [ref]$parsedUrl) -or
    $parsedUrl.Scheme -ne "https"
) {
    throw "InstallerUrl должен быть абсолютным HTTPS-адресом."
}

$resolvedSignaturePath = (Resolve-Path -LiteralPath $SignaturePath).Path
$signature = (Get-Content -Raw -LiteralPath $resolvedSignaturePath).Trim()
if ([string]::IsNullOrWhiteSpace($signature)) {
    throw "Файл подписи пуст."
}

if ([string]::IsNullOrWhiteSpace($OutputPath)) {
    $OutputPath = Join-Path $projectRoot "site/latest.json"
}

$outputDirectory = Split-Path -Parent $OutputPath
if (-not [string]::IsNullOrWhiteSpace($outputDirectory)) {
    New-Item -ItemType Directory -Force -Path $outputDirectory | Out-Null
}

$manifest = [ordered]@{
    version = $normalizedVersion
    notes = $Notes
    pub_date = [DateTimeOffset]::UtcNow.ToString("yyyy-MM-ddTHH:mm:ssZ")
    platforms = [ordered]@{
        "windows-x86_64" = [ordered]@{
            signature = $signature
            url = $InstallerUrl
        }
    }
}

$json = $manifest | ConvertTo-Json -Depth 5
$utf8WithoutBom = [Text.UTF8Encoding]::new($false)
[IO.File]::WriteAllText($OutputPath, "$json$([Environment]::NewLine)", $utf8WithoutBom)

Write-Host "Манифест обновления создан: $OutputPath"
