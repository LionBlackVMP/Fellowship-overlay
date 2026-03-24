param(
  [Parameter(Mandatory = $true)]
  [string]$Version
)

$ErrorActionPreference = "Stop"

$repoRoot = Split-Path -Parent $PSScriptRoot

$packageJsonPath = Join-Path $repoRoot "package.json"
$cargoTomlPath = Join-Path $repoRoot "src-tauri\Cargo.toml"
$tauriConfigPath = Join-Path $repoRoot "src-tauri\tauri.conf.json"

$packageJson = Get-Content $packageJsonPath -Raw | ConvertFrom-Json
$packageJson.version = $Version
$packageJson | ConvertTo-Json -Depth 100 | Set-Content $packageJsonPath -Encoding UTF8

$cargoToml = Get-Content $cargoTomlPath -Raw
$cargoToml = [regex]::Replace(
  $cargoToml,
  '(?m)^version = ".*"$',
  ('version = "{0}"' -f $Version),
  1
)
Set-Content $cargoTomlPath -Value $cargoToml -Encoding UTF8

$tauriConfig = Get-Content $tauriConfigPath -Raw | ConvertFrom-Json
$tauriConfig.version = $Version
$tauriConfig | ConvertTo-Json -Depth 100 | Set-Content $tauriConfigPath -Encoding UTF8

Write-Output "Applied version $Version"
