$Version = "0.1.0"
$Repository = "pascalvanderheiden/turbo-agent-skills-cli"

$ErrorActionPreference = "Stop"

if (-not [System.Runtime.InteropServices.RuntimeInformation]::IsOSPlatform(
    [System.Runtime.InteropServices.OSPlatform]::Windows
)) {
    throw "This installer supports Windows x64 only."
}
if ([System.Runtime.InteropServices.RuntimeInformation]::OSArchitecture -ne
    [System.Runtime.InteropServices.Architecture]::X64) {
    throw "Unsupported operating system or CPU. TASC supports Windows x64."
}

$archiveName = "tasc-v$Version-x86_64-pc-windows-msvc.tar.gz"
$baseUrl = "https://github.com/$Repository/releases/download/v$Version"
$temporary = Join-Path ([System.IO.Path]::GetTempPath()) ([System.Guid]::NewGuid().ToString())
$installDirectory = if ($env:TASC_INSTALL_DIR) {
    $env:TASC_INSTALL_DIR
} else {
    Join-Path $env:LOCALAPPDATA "Programs/TASC"
}

try {
    New-Item -ItemType Directory -Path $temporary | Out-Null
    $archivePath = Join-Path $temporary $archiveName
    $checksumsPath = Join-Path $temporary "SHA256SUMS.txt"
    Invoke-WebRequest -Uri "$baseUrl/$archiveName" -OutFile $archivePath
    Invoke-WebRequest -Uri "$baseUrl/SHA256SUMS.txt" -OutFile $checksumsPath

    $escapedName = [regex]::Escape($archiveName)
    $matches = [regex]::Matches(
        (Get-Content -Raw $checksumsPath),
        "(?m)^([0-9a-fA-F]{64})  $escapedName$"
    )
    if ($matches.Count -ne 1) {
        throw "SHA256SUMS.txt has no unique checksum for $archiveName."
    }
    $expected = $matches[0].Groups[1].Value
    $actual = (Get-FileHash -Path $archivePath -Algorithm SHA256).Hash
    if ($actual -ne $expected) {
        throw "SHA-256 verification failed for $archiveName."
    }

    & tar.exe -xzf $archivePath -C $temporary tasc.exe
    if ($LASTEXITCODE -ne 0 -or -not (Test-Path (Join-Path $temporary "tasc.exe"))) {
        throw "The verified archive does not contain the expected tasc.exe executable."
    }
    New-Item -ItemType Directory -Force -Path $installDirectory | Out-Null
    Copy-Item (Join-Path $temporary "tasc.exe") (Join-Path $installDirectory "tasc.exe") -Force
    Write-Output "Installed TASC v$Version to $installDirectory"
    if (($env:PATH -split [IO.Path]::PathSeparator) -notcontains $installDirectory) {
        Write-Output "Add $installDirectory to your PATH to run tasc."
    }
} finally {
    Remove-Item -Recurse -Force $temporary -ErrorAction SilentlyContinue
}
