@echo off
rem Starts the qupath-rs MCP server on Windows: %QUPATH_RS_BIN%, qupath-rs.exe on PATH, or a build in this checkout.
setlocal
if defined QUPATH_RS_BIN if exist "%QUPATH_RS_BIN%" ( "%QUPATH_RS_BIN%" mcp %* & exit /b %ERRORLEVEL% )
for %%I in (qupath-rs.exe) do if not "%%~$PATH:I"=="" ( "%%~$PATH:I" mcp %* & exit /b %ERRORLEVEL% )
set "CHECKOUT=%~dp0..\.."
if exist "%CHECKOUT%\target\release\qupath-rs.exe" ( "%CHECKOUT%\target\release\qupath-rs.exe" mcp %* & exit /b %ERRORLEVEL% )
echo qupath-rs MCP: cannot find qupath-rs.exe. Download it from https://github.com/TobiasPol/qupath/releases, put it on PATH or set QUPATH_RS_BIN. 1>&2
exit /b 127
