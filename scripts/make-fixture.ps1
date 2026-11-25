# Synthesizes a 16 kHz mono 16-bit WAV fixture with the Windows speech synthesizer.
# Usage: scripts/make-fixture.ps1 -Text "Hello?" -Out fixtures/audio/x.wav [-Culture en-US]
param(
    [Parameter(Mandatory)] [string]$Text,
    [Parameter(Mandatory)] [string]$Out,
    [string]$Culture = "en-US"
)
Add-Type -AssemblyName System.Speech
$synth = New-Object System.Speech.Synthesis.SpeechSynthesizer
$voice = $synth.GetInstalledVoices() | Where-Object { $_.VoiceInfo.Culture.Name -eq $Culture } | Select-Object -First 1
if (-not $voice) { throw "No installed voice for $Culture" }
$synth.SelectVoice($voice.VoiceInfo.Name)
$format = New-Object System.Speech.AudioFormat.SpeechAudioFormatInfo(16000, [System.Speech.AudioFormat.AudioBitsPerSample]::Sixteen, [System.Speech.AudioFormat.AudioChannel]::Mono)
$synth.SetOutputToWaveFile((Resolve-Path -LiteralPath (Split-Path $Out) | Join-Path -ChildPath (Split-Path $Out -Leaf)), $format)
$synth.Speak($Text)
$synth.Dispose()
Write-Output "wrote $Out with $($voice.VoiceInfo.Name)"
