param(
    [ValidateSet('Init', 'Build', 'Check', 'Test')]
    [string]$Action = 'Build'
)
$ErrorActionPreference = 'Stop'
$env:DEBUG = $null
$projectRoot = Split-Path -Parent $PSScriptRoot
Set-Location -LiteralPath $projectRoot

$localCargo = Join-Path $projectRoot '.tools\cargo'
if (Test-Path -LiteralPath (Join-Path $localCargo 'bin\cargo.exe')) {
    $env:CARGO_HOME = $localCargo
    $env:RUSTUP_HOME = Join-Path $projectRoot '.tools\rustup'
    $env:PATH = "$localCargo\bin;$env:PATH"
}
if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) { throw 'Install Rust using rustup before building.' }
if (-not $env:ANDROID_HOME) { $env:ANDROID_HOME = Join-Path $env:LOCALAPPDATA 'Android\Sdk' }
if (-not $env:NDK_HOME) {
    $ndk = Get-ChildItem -LiteralPath (Join-Path $env:ANDROID_HOME 'ndk') -Directory |
        Sort-Object { [version]$_.Name } -Descending | Select-Object -First 1
    if (-not $ndk) { throw 'Install Android NDK (side by side) using SDK Manager.' }
    $env:NDK_HOME = $ndk.FullName
}
$studioJdk = 'C:\Program Files\Android\Android Studio\jbr'
if (-not $env:JAVA_HOME) {
    $jdkCandidates = @($studioJdk) + @(Get-ChildItem -LiteralPath 'C:\Program Files\Java' -Directory -ErrorAction SilentlyContinue |
        Sort-Object Name -Descending | Select-Object -ExpandProperty FullName)
    $env:JAVA_HOME = $jdkCandidates | Where-Object {
        (Test-Path -LiteralPath (Join-Path $_ 'lib\jvm.cfg')) -and (Test-Path -LiteralPath (Join-Path $_ 'bin\javac.exe'))
    } | Select-Object -First 1
}
if (-not $env:JAVA_HOME -or -not (Test-Path -LiteralPath (Join-Path $env:JAVA_HOME 'lib\jvm.cfg'))) {
    throw 'JAVA_HOME must point to a complete JDK 17 or newer.'
}
if ($env:JAVA_HOME) { $env:PATH = "$env:JAVA_HOME\bin;$env:PATH" }

switch ($Action) {
    'Init' { & npm.cmd run tauri -- android init --ci }
    'Check' { & cargo check --manifest-path src-tauri/Cargo.toml }
    'Test' { & cargo test --manifest-path src-tauri/Cargo.toml --lib }
    'Build' {
        & npm.cmd run tauri -- android build --target aarch64 --apk --debug
        if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
        $apkCandidates = @(
            'src-tauri\gen\android\app\build\outputs\apk\arm64\debug\app-arm64-debug.apk',
            'src-tauri\gen\android\app\build\outputs\apk\universal\debug\app-universal-debug.apk'
        ) | ForEach-Object { Join-Path $projectRoot $_ }
        $apk = $apkCandidates | Where-Object { Test-Path -LiteralPath $_ } | Select-Object -First 1
        if (-not $apk) { throw 'Build completed without the expected APK.' }
        $artifacts = Join-Path $projectRoot 'artifacts'
        New-Item -ItemType Directory -Path $artifacts -Force | Out-Null
        Copy-Item -LiteralPath $apk -Destination (Join-Path $artifacts 'iDroidLoader-arm64-debug.apk')
    }
}
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
