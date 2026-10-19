$Version = "0.1.0"
$Repository = "pascalvanderheiden/turbo-agent-skills-cli"

$ErrorActionPreference = "Stop"
[Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12

function Write-PresentationEvent {
    param(
        [ValidateSet("Input", "TASC", "Prompt", "Info", "Working", "Warning", "Error", "Done", "Next")]
        [string]$Category,
        [string]$Body
    )
    $styles = @{
        Input = @("›", "36"); TASC = @("◆", "35"); Prompt = @("?", "36")
        Info = @("·", "34"); Working = @("...", "36"); Warning = @("!", "33")
        Error = @("×", "31"); Done = @("✓", "32"); Next = @("↳", "2;36")
    }
    $isError = $Category -in @("Prompt", "Working", "Warning", "Error")
    $redirected = if ($isError) { [Console]::IsErrorRedirected } else { [Console]::IsOutputRedirected }
    $label = "$($Category.ToUpperInvariant()):"
    if (-not $redirected -and -not (Test-Path Env:NO_COLOR) -and $env:TERM -ne "dumb") {
        $escape = [char]27
        $style = $styles[$Category]
        $label = "$escape[$($style[1])m$($style[0]) $Category$escape[0m"
    }
    $Body = $Body.Replace([string][char]27, '\e').Replace("`r", '\r')
    if ($isError) {
        [Console]::Error.WriteLine("$label $Body")
    } else {
        Write-Output "$label $Body"
    }
}

trap {
    Write-PresentationEvent Error $_.Exception.Message
    exit 1
}

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
    Invoke-WebRequest -UseBasicParsing -Uri "$baseUrl/$archiveName" -OutFile $archivePath
    Invoke-WebRequest -UseBasicParsing -Uri "$baseUrl/SHA256SUMS.txt" -OutFile $checksumsPath

    $escapedName = [regex]::Escape($archiveName)
    $matches = [regex]::Matches(
        (Get-Content -Raw $checksumsPath),
        "(?m)^([0-9a-fA-F]{64})  $escapedName\r?$"
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
    Write-PresentationEvent Done "Installed TASC v$Version to $installDirectory"
    if (($env:PATH -split [IO.Path]::PathSeparator) -notcontains $installDirectory) {
        Write-PresentationEvent Next "Add $installDirectory to your PATH to run tasc."
    }
} finally {
    Remove-Item -Recurse -Force $temporary -ErrorAction SilentlyContinue
}
