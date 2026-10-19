# Windows

Download `arrpi-windows-x86_64.zip` and its SHA-256 checksum from the
[latest release](https://github.com/legacy3/arrpi/releases/latest). Extract the ZIP
and open PowerShell in that directory.

```powershell
.\arrpi.exe --version
.\arrpi.exe serve --database "$env:LOCALAPPDATA\ArrPi\arrpi.sqlite"
```

In another PowerShell window, print the setup key:

```powershell
.\arrpi.exe config key --database "$env:LOCALAPPDATA\ArrPi\arrpi.sqlite"
```

The API listens on `127.0.0.1:8780`. The executable includes the API, CLI and MCP
server. The optional Next.js UI is a separate image; see the
[installation instructions](https://github.com/legacy3/arrpi#web-ui).

For CLI calls, set `ARRPI_URL` and `ARRPI_API_KEY` in that PowerShell session:

```powershell
$env:ARRPI_URL = "http://127.0.0.1:8780"
$env:ARRPI_API_KEY = "<your client key>"
.\arrpi.exe health
.\arrpi.exe tool search_library --arguments '{"available":true,"min_rating":7}'
```

Keep the setup key for administration and create scoped client keys for normal
queries. Back up the SQLite database with ArrPi stopped. To update, stop ArrPi,
replace `arrpi.exe` and restart it with the same database path.

To check the download:

```powershell
Get-FileHash .\arrpi-windows-x86_64.zip -Algorithm SHA256
Get-Content .\arrpi-windows-x86_64.zip.sha256
```
