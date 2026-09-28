# EVJ PPTX export — run by evj-import.exe. Drives PowerPoint without a window.
# Output (stdout), one line per slide:
#   SLIDE|<image>|<video or ->|<step,step,...>|<end seconds>|<notes as hex UTF-8>
param([string]$Pptx, [string]$Out, [int]$Width, [int]$Height)
$ErrorActionPreference = 'Stop'
$inv = [Globalization.CultureInfo]::InvariantCulture

function Hex([string]$s) {
    if (-not $s) { return '' }
    ($([Text.Encoding]::UTF8.GetBytes($s)) | ForEach-Object { $_.ToString('x2') }) -join ''
}

# Click boundaries of a slide's main animation sequence (seconds from the slide's start).
function Timeline($slide) {
    $t = 0.0
    $tr = $slide.SlideShowTransition
    if ($tr.EntryEffect -ne 0) { $t = [double]$tr.Duration }
    $steps = New-Object System.Collections.ArrayList
    [void]$steps.Add(0.0)
    $prevEnd = $t; $groupStart = $t
    $seq = $slide.TimeLine.MainSequence
    for ($k = 1; $k -le $seq.Count; $k++) {
        $tm = $seq.Item($k).Timing
        $dur = [double]$tm.Duration * [Math]::Max(1.0, [double]$tm.RepeatCount)
        switch ([int]$tm.TriggerType) {
            1 { [void]$steps.Add($prevEnd); $start = $prevEnd + $tm.TriggerDelayTime; $groupStart = $start }   # on click
            4 { [void]$steps.Add($prevEnd); $start = $prevEnd + $tm.TriggerDelayTime; $groupStart = $start }   # on shape click
            3 { $start = $prevEnd + $tm.TriggerDelayTime; $groupStart = $start }                                 # after previous
            default { $start = $groupStart + $tm.TriggerDelayTime }                                               # with previous
        }
        $prevEnd = [Math]::Max($prevEnd, $start + $dur)
    }
    $media = $false
    foreach ($sh in $slide.Shapes) {
        if ($sh.Type -eq 16 -and $sh.MediaType -eq 3) {
            $media = $true
            $prevEnd = [Math]::Max($prevEnd, $t + $sh.MediaFormat.Length / 1000.0)
        }
    }
    return @{ steps = $steps; end = $prevEnd + 0.3; animated = ($seq.Count -gt 0 -or $media -or $t -gt 0) }
}

$app = New-Object -ComObject PowerPoint.Application
$app.DisplayAlerts = 1   # ppAlertsNone
$pres = $null
try {
    $pres = $app.Presentations.Open($Pptx, -1, 0, 0)   # read-only, no window
    $n = $pres.Slides.Count
    for ($i = 1; $i -le $n; $i++) {
        $s = $pres.Slides.Item($i)
        $img = 'slide{0:D3}.png' -f $i
        $s.Export((Join-Path $Out $img), 'PNG', $Width, $Height)
        $notes = ''
        try {
            $ph = $s.NotesPage.Shapes.Placeholders
            for ($k = 1; $k -le $ph.Count; $k++) {
                if ($ph.Item($k).PlaceholderFormat.Type -eq 2) { $notes = $ph.Item($k).TextFrame.TextRange.Text }
            }
        } catch {}
        $tl = Timeline $s
        $video = '-'
        if ($tl.animated) {
            # A one-slide copy where every click is "after previous", recorded as a video.
            $tmp = Join-Path $env:TEMP ('evj-slide-{0}-{1}.pptx' -f $PID, $i)
            $pres.SaveCopyAs($tmp)
            $one = $app.Presentations.Open($tmp, 0, 0, 0)
            try {
                for ($k = $one.Slides.Count; $k -ge 1; $k--) { if ($k -ne $i) { $one.Slides.Item($k).Delete() } }
                $only = $one.Slides.Item(1)
                $seq = $only.TimeLine.MainSequence
                for ($k = 1; $k -le $seq.Count; $k++) {
                    $tm = $seq.Item($k).Timing
                    if ($tm.TriggerType -eq 1 -or $tm.TriggerType -eq 4) { $tm.TriggerType = 3 }
                }
                $only.SlideShowTransition.AdvanceOnTime = -1
                $only.SlideShowTransition.AdvanceTime = $tl.end
                $video = 'slide{0:D3}.mp4' -f $i
                $one.CreateVideo((Join-Path $Out $video), -1, 5, $Height, 30, 85)
                $wait = 0
                while ($one.CreateVideoStatus -eq 1 -or $one.CreateVideoStatus -eq 2) {
                    Start-Sleep -Milliseconds 250
                    $wait += 1
                    if ($wait -gt 2400) { throw "video export of slide $i timed out" }
                }
                if ($one.CreateVideoStatus -ne 3) { $video = '-' }
            } finally {
                $one.Close()
                Remove-Item $tmp -ErrorAction SilentlyContinue
            }
        }
        $steps = ($tl.steps | ForEach-Object { ([double]$_).ToString($inv) }) -join ','
        Write-Output ('SLIDE|{0}|{1}|{2}|{3}|{4}' -f $img, $video, $steps, ([double]$tl.end).ToString($inv), (Hex $notes))
    }
} finally {
    if ($pres) { $pres.Close() }
    if ($app.Presentations.Count -eq 0) { $app.Quit() }
}
