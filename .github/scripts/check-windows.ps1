$ErrorActionPreference = 'Stop'
$PSNativeCommandUseErrorActionPreference = $true
Set-Location (Join-Path $PSScriptRoot '../..')

cargo build --release --locked -p arrpi --target x86_64-pc-windows-msvc

$exe = Resolve-Path target/x86_64-pc-windows-msvc/release/arrpi.exe
& $exe --version
$db = Join-Path $env:RUNNER_TEMP 'arrpi windows smoke/arrpi.sqlite'
$server = Start-Process -FilePath $exe -ArgumentList "serve --database `"$db`"" -PassThru -RedirectStandardOutput "$env:RUNNER_TEMP/arrpi.stdout" -RedirectStandardError "$env:RUNNER_TEMP/arrpi.stderr"
try {
  $ready = $false
  for ($attempt = 0; $attempt -lt 30; $attempt++) {
    if ($server.HasExited) { throw 'ArrPi exited before readiness' }
    try {
      Invoke-RestMethod http://127.0.0.1:8780/health | Out-Null
      $ready = $true
      break
    } catch { Start-Sleep -Milliseconds 500 }
  }
  if (-not $ready) { throw 'ArrPi did not become ready' }
  $key = & $exe config key --database $db
  $status = Invoke-RestMethod http://127.0.0.1:8780/v1/status -Headers @{ Authorization = "Bearer $key" }
  if ($status.sources.Count -ne 0) { throw 'Fresh database contains unexpected sources' }
  & $exe health --url http://127.0.0.1:8780
  if (-not (Test-Path $db)) { throw 'Database was not created' }
} finally {
  if (-not $server.HasExited) { Stop-Process -Id $server.Id -Force }
}

New-Item -ItemType Directory -Path dist | Out-Null
Copy-Item target/x86_64-pc-windows-msvc/release/arrpi.exe dist/
Copy-Item LICENSE dist/
Copy-Item docs/windows.md dist/README.md
Compress-Archive -Path dist/* -DestinationPath arrpi-windows-x86_64.zip
$hash = (Get-FileHash arrpi-windows-x86_64.zip -Algorithm SHA256).Hash.ToLowerInvariant()
Set-Content -Path arrpi-windows-x86_64.zip.sha256 -Value "$hash  arrpi-windows-x86_64.zip" -Encoding ascii
