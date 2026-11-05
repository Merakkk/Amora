param(
    [ValidateSet('debug','release')][string]$Profile = 'debug',
    [string]$TargetDir = 'target-dev/prism-black',
    [ValidateSet('normal','software')][string]$Mode = 'normal',
    [ValidateSet('Musik','Fokus','Minum','To-do','Jadwal','Shelf','Clipboard','Kontrol','Setelan')][string]$Page = 'Setelan',
    [ValidateRange(0,100)][int]$Transparency = 80,
    [ValidateRange(0,48)][int]$Blur = 24,
    [switch]$DemoMusic,
    [switch]$Hover,
    [switch]$PrimaryMonitor
)
$ErrorActionPreference = 'Stop'
$projectDir = Split-Path $PSScriptRoot -Parent
$executable = Join-Path (Join-Path $projectDir $TargetDir) "$Profile\amora.exe"
if (-not (Test-Path -LiteralPath $executable)) { throw "Build belum tersedia: $executable" }
$testData = Join-Path (Split-Path $executable -Parent) 'amora-preview-data'
New-Item -ItemType Directory -Force -Path $testData | Out-Null
$settingsPath = Join-Path $testData 'settings.json'
if (Test-Path -LiteralPath $settingsPath) {
    $settings = Get-Content -LiteralPath $settingsPath -Raw | ConvertFrom-Json
} else {
    $settings = [pscustomobject]@{appearance=[pscustomobject]@{transparency=80;blur=24};auto_hide=$false;mascot=$false;clipboard=$false;tidy_tables=$false}
}
if ($PSBoundParameters.ContainsKey('Transparency')) { $settings.appearance.transparency = $Transparency }
if ($PSBoundParameters.ContainsKey('Blur')) { $settings.appearance.blur = $Blur }
if ($PrimaryMonitor) { $settings | Add-Member -NotePropertyName monitor -NotePropertyValue $null -Force }
$settings | ConvertTo-Json -Depth 12 | Set-Content -LiteralPath $settingsPath -Encoding utf8
$start = [System.Diagnostics.ProcessStartInfo]::new($executable)
$start.UseShellExecute = $false
$start.CreateNoWindow = $true
$start.WindowStyle = [System.Diagnostics.ProcessWindowStyle]::Hidden
$start.WorkingDirectory = $projectDir
$start.Arguments = if ($Hover) { "--preview-hover $Page" } else { "--preview-prism $Page" }
$start.EnvironmentVariables.Remove('AMORA_FORCE_OPEN')
$start.EnvironmentVariables.Remove('AMORA_SOFTWARE')
$start.EnvironmentVariables.Remove('AMORA_DEMO_LYRIC')
if ($Mode -eq 'software') { $start.EnvironmentVariables['AMORA_SOFTWARE'] = '1' }
if ($DemoMusic) { $start.EnvironmentVariables['AMORA_DEMO_LYRIC'] = 'Ruang untuk tenang' }
$preview = [System.Diagnostics.Process]::Start($start)
[pscustomobject]@{ProcessId=$preview.Id;Mode=$Mode;Profile=$Profile;DataDirectory=$testData} | ConvertTo-Json
