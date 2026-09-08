param([ValidateSet('zh', 'es')][string]$Language = 'zh', [int]$Seconds = 120)
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Windows.Forms
Add-Type -AssemblyName System.Drawing
# https://learn.microsoft.com/windows/win32/api/winuser/nf-winuser-setprocessdpiawarenesscontext
Add-Type @'
using System;
using System.Runtime.InteropServices;
public static class FixtureDpi {
    [DllImport("user32.dll")] public static extern bool SetProcessDpiAwarenessContext(IntPtr value);
}
'@
[void][FixtureDpi]::SetProcessDpiAwarenessContext([IntPtr](-4))
# This standalone fixture owns its window; it never changes display settings.
[System.Windows.Forms.Application]::EnableVisualStyles()
$screen = [System.Windows.Forms.Screen]::AllScreens | Where-Object { !$_.Primary } | Select-Object -First 1
if ($null -eq $screen) { $screen = [System.Windows.Forms.Screen]::PrimaryScreen }
$form = New-Object System.Windows.Forms.Form
$form.Text = 'Prollyglot native visual fixture'
$form.FormBorderStyle = 'None'
$form.StartPosition = 'Manual'
$form.Bounds = $screen.Bounds
$form.TopMost = $true
$form.BackColor = [System.Drawing.Color]::FromArgb(25, 25, 25)
$form.KeyPreview = $true
$form.Add_KeyDown({ if ($_.KeyCode -eq 'Escape') { $form.Close() } })
$label = New-Object System.Windows.Forms.Label
$label.AutoSize = $false
$label.SetBounds(($screen.Bounds.Width / 2 - 490), ($screen.Bounds.Height - 200), 980, 180)
$label.TextAlign = 'MiddleCenter'
$label.ForeColor = [System.Drawing.Color]::White
$font = New-Object System.Drawing.Font('Microsoft YaHei', 24, [System.Drawing.FontStyle]::Regular, [System.Drawing.GraphicsUnit]::Pixel)
$label.Font = $font
$label.Text = if ($Language -eq 'zh') { [string]::Concat([char]0x4f60,[char]0x597d,[char]0x4e16,[char]0x754c,[char]0x3002,[char]0x6b22,[char]0x8fce,[char]0x56de,[char]0x6765,[char]0x3002) } else { "Buenos d$([char]0xed)as. $([char]0xbf)C$([char]0xf3)mo est$([char]0xe1)s?" }
$form.Controls.Add($label)
$marker = New-Object System.Windows.Forms.Panel
$marker.SetBounds(16, 16, 24, 24)
$form.Controls.Add($marker)
$timer = New-Object System.Windows.Forms.Timer
$timer.Interval = 100
$started = [DateTime]::UtcNow
$timer.Add_Tick({
    $marker.BackColor = if ($marker.BackColor -eq [System.Drawing.Color]::DarkSlateGray) { [System.Drawing.Color]::Gray } else { [System.Drawing.Color]::DarkSlateGray }
    if (([DateTime]::UtcNow - $started).TotalSeconds -ge $Seconds) { $form.Close() }
})
$timer.Start()
try { [void]$form.ShowDialog() } finally { $timer.Dispose(); $form.Dispose(); $font.Dispose() }
