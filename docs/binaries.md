# Linux and macOS

Download the archive and SHA-256 checksum for your platform from
[Releases](https://github.com/legacy3/arrpi/releases/latest).

| Platform            | Archive                      |
| ------------------- | ---------------------------- |
| Linux x86-64        | `arrpi-linux-x86_64.tar.gz`  |
| Linux ARM64         | `arrpi-linux-aarch64.tar.gz` |
| macOS Intel         | `arrpi-macos-x86_64.tar.gz`  |
| macOS Apple Silicon | `arrpi-macos-aarch64.tar.gz` |

Linux builds require glibc 2.36 or newer (Debian 12, Ubuntu 24.04 or newer).
They use the same executable tested in the corresponding Docker image. For
Alpine or older systems, use the Docker image.

Verify the checksum with `sha256sum -c <archive>.sha256` on Linux or
`shasum -a 256 -c <archive>.sha256` on macOS, then extract the archive:

```sh
tar -xzf < archive > .tar.gz
./arrpi --version
./arrpi serve --database "$HOME/.local/share/arrpi/arrpi.sqlite"
```

In another terminal, print the setup key:

```sh
./arrpi config key --database "$HOME/.local/share/arrpi/arrpi.sqlite"
```

The API listens on `127.0.0.1:8780`. The binary includes the API, CLI and MCP
server; the web UI is a separate optional Docker image.

Use `ARRPI_URL` and `ARRPI_API_KEY` for CLI queries. Keep the setup key for
administration and create scoped client keys for normal use. Stop ArrPi before
copying its SQLite database. To update, stop ArrPi, replace the executable and
restart it with the same database path.

macOS binaries are not Developer ID signed or notarized. If Gatekeeper blocks a downloaded
binary, approve it under System Settings → Privacy & Security after verifying
its checksum.
