<#
.SYNOPSIS
  Build a signed Windows release (NSIS .exe + MSI, plus updater .sig files).

.DESCRIPTION
  Two signing options, pick one:

  A) Azure Trusted Signing (recommended: no certificate to manage, ~$10/mo)
     Install:  dotnet tool install --global AzureSignTool   (or use `trusted-signing-cli`)
     Set in tauri.conf.json → bundle.windows.signCommand, e.g.
       "signCommand": "trusted-signing-cli -e https://eus.codesigning.azure.net -a <account> -c <profile> %1"
     and authenticate with AZURE_TENANT_ID / AZURE_CLIENT_ID / AZURE_CLIENT_SECRET env vars.

  B) Your own code-signing certificate (OV/EV .pfx)
     Set WINDOWS_CERT_THUMBPRINT to the cert's SHA-1 thumbprint (cert in the
     CurrentUser\My store; EV certs live on a token and still work this way).
     Tauri then calls signtool with that thumbprint + the timestamp URL in
     tauri.conf.json → bundle.windows.timestampUrl.

  Updater signing needs TAURI_SIGNING_PRIVATE_KEY (contents of your .key file)
  and, if the key is password-protected, TAURI_SIGNING_PRIVATE_KEY_PASSWORD.

.EXAMPLE
  $env:WINDOWS_CERT_THUMBPRINT = "ABCDEF..."; .\scripts\release-windows.ps1
#>
$ErrorActionPreference = "Stop"
Set-Location (Join-Path $PSScriptRoot "..")

$conf = Get-Content src-tauri/tauri.conf.json -Raw | ConvertFrom-Json
$signCommand = $conf.bundle.windows.signCommand

if ($env:WINDOWS_CERT_THUMBPRINT) {
  Write-Host "→ signing: certificate thumbprint $($env:WINDOWS_CERT_THUMBPRINT.Substring(0,8))…"
  # Tauri reads this env var as an override for bundle.windows.certificateThumbprint.
  $env:TAURI_BUNDLE_WINDOWS_CERTIFICATE_THUMBPRINT = $env:WINDOWS_CERT_THUMBPRINT
} elseif ($signCommand) {
  Write-Host "→ signing: custom signCommand from tauri.conf.json"
} else {
  Write-Warning "no signing configured — installers will be UNSIGNED (SmartScreen will warn)."
}

if ($env:TAURI_SIGNING_PRIVATE_KEY) {
  Write-Host "→ updater artifacts: signed"
} else {
  Write-Host "→ updater artifacts: not signed (fine if the updater is disabled)"
}

# Only SQLCipher builds (or Turso legacy imports) use vendored OpenSSL.
$starter = Get-Content starter.config.json -Raw | ConvertFrom-Json
if ($starter.database.engine -eq "sqlcipher" -or $starter.database.importSqlcipher) {
  foreach ($tool in @("perl", "nasm")) {
    if (-not (Get-Command $tool -ErrorAction SilentlyContinue)) {
      Write-Warning "$tool not found on PATH — building the vendored OpenSSL for SQLCipher will fail. winget install StrawberryPerl.StrawberryPerl NASM.NASM"
    }
  }
}

npm run tauri build -- --bundles nsis,msi
if ($LASTEXITCODE -ne 0) { throw "tauri build failed" }

$bundle = "src-tauri/target/release/bundle"
Write-Host "`nArtifacts:"
Get-ChildItem "$bundle/nsis/*.exe", "$bundle/nsis/*.sig", "$bundle/msi/*.msi" -ErrorAction SilentlyContinue | ForEach-Object { Write-Host "  $($_.FullName)" }

Write-Host "`nSignature check:"
Get-ChildItem "$bundle/nsis/*.exe" | ForEach-Object {
  $sig = Get-AuthenticodeSignature $_.FullName
  Write-Host "  $($_.Name): $($sig.Status) $($sig.SignerCertificate.Subject)"
}
Write-Host "`nPublish: node scripts/publish-feed.mjs --version <x.y.z> --notes '…'"
