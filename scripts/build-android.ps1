# Builds the JuiceBox Remote phone app (apps/mobile) as an APK.
#
#   powershell -File scripts/build-android.ps1                 # debug APK for BlueStacks / emulators (x86_64)
#   powershell -File scripts/build-android.ps1 -Release        # release APK for phones (arm64, arm, x86_64)
#
# Uses the Android tools in %USERPROFILE%\.android-dev (Java 17, SDK, NDK) unless JAVA_HOME /
# ANDROID_HOME / NDK_HOME are already set. Nothing is changed in Windows' settings.
#
# Why not just `tauri android build`: it links the Rust library into the Android project with a
# symbolic link, which Windows only allows with Developer Mode on. So Tauri builds the Rust part
# (and stops at that link), this script copies the library in, then runs Gradle itself.
param(
    [switch]$Release,
    [string[]]$Targets
)
# "Continue": Windows PowerShell turns a tool's normal stderr lines into errors under "Stop".
# Failures are checked by exit code below.
$ErrorActionPreference = "Continue"
$root = Split-Path $PSScriptRoot -Parent
$mobile = Join-Path $root "apps\mobile"
$android = Join-Path $mobile "src-tauri\gen\android"

$dev = Join-Path $env:USERPROFILE ".android-dev"
if (-not $env:JAVA_HOME) { $env:JAVA_HOME = Join-Path $dev "jdk" }
if (-not $env:ANDROID_HOME) { $env:ANDROID_HOME = Join-Path $dev "sdk" }
if (-not $env:NDK_HOME) {
    $ndk = Get-ChildItem (Join-Path $env:ANDROID_HOME "ndk") -Directory -ErrorAction SilentlyContinue | Sort-Object Name | Select-Object -Last 1
    if (-not $ndk) { throw "No Android NDK found in $env:ANDROID_HOME\ndk" }
    $env:NDK_HOME = $ndk.FullName
}
$env:Path = "$env:JAVA_HOME\bin;$env:ANDROID_HOME\platform-tools;$env:Path"
# Gradle's Java makes small socket files in a temp folder; a long folder name, or one with a
# space (a user name like "Jane Doe"), stops it ("Unable to establish loopback connection").
$sock = Join-Path $dev "tmp"
New-Item -ItemType Directory -Force $sock | Out-Null
$short = (New-Object -ComObject Scripting.FileSystemObject).GetFolder($sock).ShortPath
if (-not $env:JAVA_TOOL_OPTIONS) { $env:JAVA_TOOL_OPTIONS = "-Djdk.net.unixdomain.tmpdir=$short" }

if (-not $Targets) { $Targets = if ($Release) { @("aarch64", "armv7", "x86_64") } else { @("x86_64") } }
$abi = @{ aarch64 = "arm64-v8a"; armv7 = "armeabi-v7a"; i686 = "x86"; x86_64 = "x86_64" }
$triple = @{ aarch64 = "aarch64-linux-android"; armv7 = "armv7-linux-androideabi"; i686 = "i686-linux-android"; x86_64 = "x86_64-linux-android" }
$profile = if ($Release) { "release" } else { "debug" }
$lib = "libjuicebox_remote_lib.so"

Push-Location $mobile
try {
    foreach ($t in $Targets) {
        Write-Host "== Rust library for $t ($profile)"
        $cliArgs = @("tauri", "android", "build", "--apk", "--target", $t, "--ci")
        if (-not $Release) { $cliArgs += "--debug" }
        # Expected to stop at the symbolic link; any other failure is real.
        $out = & npx @cliArgs 2>&1 | ForEach-Object { "$_" }
        $so = Join-Path $root "target\$($triple[$t])\$profile\$lib"
        $linkOnly = ($out -join "`n") -match "symbolic link"
        if ($LASTEXITCODE -ne 0 -and -not $linkOnly) { $out | Select-Object -Last 30; throw "Rust build for $t failed" }
        if (-not (Test-Path $so)) { $out | Select-Object -Last 30; throw "No $so after the build" }
        $dest = Join-Path $android "app\src\main\jniLibs\$($abi[$t])"
        New-Item -ItemType Directory -Force $dest | Out-Null
        Copy-Item $so (Join-Path $dest $lib) -Force
    }
    # Only the ABIs just built go in the APK.
    $abiList = ($Targets | ForEach-Object { $abi[$_] }) -join ","
    $cap = $profile.Substring(0, 1).ToUpper() + $profile.Substring(1)
    Write-Host "== APK ($abiList)"
    Push-Location $android
    try {
        & .\gradlew.bat --no-daemon -q "assembleUniversal$cap" -x "rustBuildUniversal$cap" "-PabiList=$abiList"
        if ($LASTEXITCODE -ne 0) { throw "Gradle failed" }
    } finally { Pop-Location }
    $apk = Get-ChildItem (Join-Path $android "app\build\outputs\apk\universal\$profile") -Filter *.apk | Select-Object -First 1
    Write-Host "Built $($apk.FullName) ($([math]::Round($apk.Length / 1MB, 1)) MB)"
} finally { Pop-Location }
