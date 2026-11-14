param(
    [ValidateSet('aarch64','x86_64')][string]$Target = 'aarch64',
    [ValidateSet('Debug','Release')][string]$Configuration = 'Debug'
)
$ErrorActionPreference = 'Stop'
. "$PSScriptRoot/Enter-Environment.ps1"
$projectRoot = Split-Path -Parent $PSScriptRoot
$androidRoot = Join-Path $projectRoot 'apps/powerwatch/src-tauri/gen/android'
$log = Join-Path $projectRoot '.cache/android-build.log'
Push-Location $projectRoot
try {
    $arguments = @('tauri','android','build','--target',$Target,'--apk','--ci')
    if ($Configuration -eq 'Debug') { $arguments += '--debug' }
    & pnpm @arguments *> $log
    if ($LASTEXITCODE -eq 0) { Get-Content -LiteralPath $log -Tail 12; return }
    $output = Get-Content -LiteralPath $log -Raw
    $finished = if ($Configuration -eq 'Debug') { 'Finished `dev` profile' } else { 'Finished `release` profile' }
    if ($output -notmatch 'Creation symbolic link is not allowed for this system' -or $output -notmatch [regex]::Escape($finished)) {
        Get-Content -LiteralPath $log -Tail 35
        throw 'Збірка Android не завершилася.'
    }
    # Windows без права symlink: лише точна копія щойно зібраної бібліотеки.
    $profile = $Configuration.ToLowerInvariant()
    $triple = if ($Target -eq 'aarch64') { 'aarch64-linux-android' } else { 'x86_64-linux-android' }
    $abi = if ($Target -eq 'aarch64') { 'arm64-v8a' } else { 'x86_64' }
    $arch = if ($Target -eq 'aarch64') { 'arm64' } else { 'x86_64' }
    $flavor = if ($Target -eq 'aarch64') { 'Arm64' } else { 'X86_64' }
    $targetRoot = if ($env:CARGO_TARGET_DIR) { [IO.Path]::GetFullPath($env:CARGO_TARGET_DIR) } else { Join-Path $projectRoot 'target' }
    $library = Join-Path $targetRoot "$triple/$profile/libpowerwatch_app_lib.so"
    $jni = Join-Path $androidRoot "app/src/main/jniLibs/$abi"
    New-Item -ItemType Directory -Path $jni -Force | Out-Null
    $copied = Join-Path $jni 'libpowerwatch_app_lib.so'
    Copy-Item -LiteralPath $library -Destination $copied -Force
    if ((Get-FileHash $library).Hash -ne (Get-FileHash $copied).Hash) { throw 'Не збігається бібліотека Android.' }
    Push-Location $androidRoot
    try {
        & ./gradlew.bat ":app:assemble${flavor}${Configuration}" -x ":app:rustBuild${flavor}${Configuration}" "-PtargetList=$Target" "-ParchList=$arch" "-PabiList=$abi" --no-daemon --console=plain
        if ($LASTEXITCODE -ne 0) { throw 'Не завершено пакування APK.' }
    } finally { Pop-Location }
} finally { Pop-Location }
