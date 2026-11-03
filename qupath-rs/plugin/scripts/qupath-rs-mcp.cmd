@echo off
rem Starts the qupath-rs MCP server on Windows: %QUPATH_RS_BIN%, qupath-rs.exe on PATH, or a build in this checkout.
rem Builds from before the MCP server open the QuPath window for `mcp`, so binaries without the server are skipped.
setlocal
set "BIN="
set "STALE="
set "ONPATH="
for %%I in (qupath-rs.exe) do set "ONPATH=%%~$PATH:I"
for %%B in ("%QUPATH_RS_BIN%" "%ONPATH%" "%~dp0..\..\target\release\qupath-rs.exe") do if not defined BIN if not "%%~B"=="" if exist "%%~B" (
  findstr /m /c:"io.modelcontextprotocol/protocolVersion" "%%~B" >nul 2>&1 && set "BIN=%%~B" || if not defined STALE set "STALE=%%~B"
)
if not defined BIN goto missing
"%BIN%" mcp %*
exit /b %ERRORLEVEL%

:missing
if defined STALE (
  echo qupath-rs MCP: "%STALE%" is too old: it has no MCP server. Download a newer release from https://github.com/TobiasPol/qupath/releases or rebuild with: cargo install --path . --bin qupath-rs 1>&2
) else (
  echo qupath-rs MCP: cannot find qupath-rs.exe. Download it from https://github.com/TobiasPol/qupath/releases, put it on PATH or set QUPATH_RS_BIN. 1>&2
)
exit /b 127
