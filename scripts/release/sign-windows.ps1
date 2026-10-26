# Ported unchanged from tC4 scripts/sign-windows.ps1 (unfoldingWord/translationCore4), card G3 step 7.
# Sign the Windows build with Azure Artifact Signing (#456).
#
#   -Setup        Fetch the pinned signing tools into -ToolDir and write metadata.json.
#   -Sign <path>  Sign each file that has no signature, then require Valid.
#                 A folder means every .exe, .dll and .node file under it.
#                 A file is signed whatever its extension (Inno Setup passes .tmp files).
#
# release.yml runs -Setup once, then -Sign on the staged app folder before the manifest hashes it.
# Inno Setup runs -Sign on the installer and on its uninstaller (tc4.iss).
#
# A file that Microsoft or another publisher signed keeps its signature: only
# NotSigned files are signed. Any other non-Valid status stops the build.
#
# Authentication is GitHub OIDC through the Azure WorkloadIdentityCredential:
# AZURE_TENANT_ID and AZURE_CLIENT_ID name an app registration that holds the
# "Artifact Signing Certificate Profile Signer" role on the certificate profile and
# trusts this repository's GitHub tokens. The job needs `id-token: write`, which
# gives it ACTIONS_ID_TOKEN_REQUEST_URL and ACTIONS_ID_TOKEN_REQUEST_TOKEN. No
# secret is stored.
param(
  [Parameter(Mandatory = $true)][string]$ToolDir,
  [switch]$Setup,
  [string[]]$Sign
)
$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'

# Pins. signtool must be 10.0.2261.755 or later, and not the 20348 SDK (Microsoft
# Learn, "Set up signing integrations to use Artifact Signing"). The dlib needs
# a .NET 8 or later runtime (its runtimeconfig.json rolls forward to a major).
$Packages = @(
  @{ Id = 'microsoft.windows.sdk.buildtools'; Version = '10.0.26100.4188'; Dir = 'buildtools'
     Sha256 = '180deb372659029864c10a0c04787833234d64aacd1d2c0661d2c00295d8e022' },
  @{ Id = 'microsoft.artifactsigning.client'; Version = '1.0.128'; Dir = 'client'
     Sha256 = '74bd7d27e6ce1051409c38d9b46bc8df0400ecd643d51ffbf2ac00869061e40b' }
)
$SignTool = Join-Path $ToolDir 'buildtools\bin\10.0.26100.0\x64\signtool.exe'
$Dlib = Join-Path $ToolDir 'client\bin\x64\Azure.CodeSigning.Dlib.dll'
$Metadata = Join-Path $ToolDir 'metadata.json'
$Timestamp = 'http://timestamp.acs.microsoft.com'
$TokenFile = Join-Path $ToolDir 'github-oidc-token'

# A GitHub OIDC token expires after minutes, and signing runs late in a long build.
# So each signtool run gets a fresh token, which WorkloadIdentityCredential reads
# from AZURE_FEDERATED_TOKEN_FILE.
function Write-FederatedToken {
  $uri = "$env:ACTIONS_ID_TOKEN_REQUEST_URL&audience=api://AzureADTokenExchange"
  $headers = @{ Authorization = "bearer $env:ACTIONS_ID_TOKEN_REQUEST_TOKEN" }
  for ($try = 1; ; $try++) {
    try { $token = (Invoke-RestMethod -Uri $uri -Headers $headers).value; break }
    catch { if ($try -ge 5) { throw "GitHub OIDC token request failed $try times: $($_.Exception.Message)" }; Start-Sleep -Seconds (5 * $try) }
  }
  if (-not $token) { throw 'GitHub returned no OIDC token' }
  [System.IO.File]::WriteAllText($TokenFile, $token)
  $env:AZURE_FEDERATED_TOKEN_FILE = $TokenFile
  return $token
}

if ($Setup) {
  $required = 'AZURE_SIGNING_ENDPOINT', 'AZURE_SIGNING_ACCOUNT', 'AZURE_SIGNING_PROFILE',
              'AZURE_TENANT_ID', 'AZURE_CLIENT_ID',
              'ACTIONS_ID_TOKEN_REQUEST_URL', 'ACTIONS_ID_TOKEN_REQUEST_TOKEN'
  $missing = @($required | Where-Object { -not [Environment]::GetEnvironmentVariable($_) })
  if ($missing.Count) { throw "Signing is on, but these values are empty: $($missing -join ', ')" }

  # Check the trust now, not an hour into the build. Print the token's subject
  # (not secret), then ask Entra to exchange the token. A wrong subject, audience,
  # tenant or client id fails here. (The Signer role is proven only by signing.)
  New-Item -ItemType Directory -Force -Path $ToolDir | Out-Null
  try {
    $token = Write-FederatedToken
    $payload = $token.Split('.')[1].Replace('-', '+').Replace('_', '/')
    $payload += '=' * ((4 - $payload.Length % 4) % 4)
    $claims = [Text.Encoding]::UTF8.GetString([Convert]::FromBase64String($payload)) | ConvertFrom-Json
    Write-Host "GitHub OIDC subject: $($claims.sub)"
    try {
      Invoke-RestMethod -Method Post -Uri "https://login.microsoftonline.com/$env:AZURE_TENANT_ID/oauth2/v2.0/token" -Body @{
        client_id             = $env:AZURE_CLIENT_ID
        grant_type            = 'client_credentials'
        scope                 = 'https://management.azure.com/.default'
        client_assertion_type = 'urn:ietf:params:oauth:client-assertion-type:jwt-bearer'
        client_assertion      = $token
      } | Out-Null
    } catch {
      $detail = $_.ErrorDetails.Message
      if ($detail) { try { $detail = ($detail | ConvertFrom-Json).error_description } catch {} }
      throw "Azure refused the GitHub OIDC token for subject $($claims.sub): $detail"
    }
    Write-Host 'Azure accepts the GitHub OIDC token'
  } finally {
    if (Test-Path -LiteralPath $TokenFile) { Remove-Item -LiteralPath $TokenFile -Force }
  }

  Add-Type -AssemblyName System.IO.Compression.FileSystem
  foreach ($p in $Packages) {
    $nupkg = Join-Path $ToolDir "$($p.Id).$($p.Version).nupkg"
    if (-not (Test-Path -LiteralPath $nupkg)) {
      Invoke-WebRequest -UseBasicParsing -OutFile $nupkg `
        -Uri "https://api.nuget.org/v3-flatcontainer/$($p.Id)/$($p.Version)/$($p.Id).$($p.Version).nupkg"
    }
    $actual = (Get-FileHash -LiteralPath $nupkg -Algorithm SHA256).Hash.ToLowerInvariant()
    if ($actual -ne $p.Sha256) { throw "Checksum mismatch for $($p.Id) $($p.Version): expected $($p.Sha256), got $actual" }
    $dest = Join-Path $ToolDir $p.Dir
    if (Test-Path -LiteralPath $dest) { Remove-Item -LiteralPath $dest -Recurse -Force }
    [System.IO.Compression.ZipFile]::ExtractToDirectory($nupkg, $dest)
  }
  foreach ($tool in $SignTool, $Dlib) {
    if (-not (Test-Path -LiteralPath $tool)) { throw "Signing tool not found after setup: $tool" }
  }
  # WorkloadIdentityCredential only: the other credential types would each be
  # tried and fail on a CI runner. (EnvironmentCredential has no secret, so it
  # reports itself unavailable and is skipped.)
  [ordered]@{
    Endpoint               = $env:AZURE_SIGNING_ENDPOINT
    CodeSigningAccountName = $env:AZURE_SIGNING_ACCOUNT
    CertificateProfileName = $env:AZURE_SIGNING_PROFILE
    ExcludeCredentials     = @('ManagedIdentityCredential',
      'SharedTokenCacheCredential', 'VisualStudioCredential', 'VisualStudioCodeCredential',
      'AzureCliCredential', 'AzurePowerShellCredential', 'AzureDeveloperCliCredential',
      'InteractiveBrowserCredential')
  } | ConvertTo-Json | Set-Content -LiteralPath $Metadata -Encoding Ascii
  Write-Host "signing tools ready: signtool $($Packages[0].Version), Artifact Signing client $($Packages[1].Version)"
}

if ($Sign) {
  $files = @(foreach ($path in $Sign) {
    if (Test-Path -LiteralPath $path -PathType Container) {
      Get-ChildItem -LiteralPath $path -Recurse -File | Where-Object { $_.Extension -in '.exe', '.dll', '.node' }
    } else {
      Get-Item -LiteralPath $path
    }
  })
  $toSign = @()
  foreach ($f in $files) {
    $status = (Get-AuthenticodeSignature -LiteralPath $f.FullName).Status
    if ($status -eq 'NotSigned') { $toSign += $f.FullName }
    elseif ($status -ne 'Valid') { throw "$($f.FullName) has signature status $status; refusing to sign over it" }
  }
  # Batches keep each signtool command line well under the Windows limit.
  try {
    for ($i = 0; $i -lt $toSign.Count; $i += 50) {
      $batch = @($toSign[$i..([Math]::Min($i + 49, $toSign.Count - 1))])
      Write-FederatedToken | Out-Null
      # Windows PowerShell 5.1 turns native stderr into a terminating error under
      # 'Stop' when output is redirected; the exit code decides instead.
      $ErrorActionPreference = 'Continue'
      & $SignTool sign /v /fd SHA256 /tr $Timestamp /td SHA256 /dlib $Dlib /dmdf $Metadata @batch 2>&1 | Write-Host
      $exit = $LASTEXITCODE
      $ErrorActionPreference = 'Stop'
      if ($exit -ne 0) { throw "signtool failed (exit $exit)" }
    }
  } finally {
    if (Test-Path -LiteralPath $TokenFile) { Remove-Item -LiteralPath $TokenFile -Force }
  }
  foreach ($path in $toSign) {
    $sig = Get-AuthenticodeSignature -LiteralPath $path
    if ($sig.Status -ne 'Valid' -or -not $sig.TimeStamperCertificate) {
      throw "$path is not Valid and timestamped after signing (status $($sig.Status))"
    }
  }
  Write-Host "signed $($toSign.Count) file(s); $($files.Count - $toSign.Count) already carried a valid signature"
}
