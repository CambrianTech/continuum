# continuum-tray.ps1 — Windows System Tray for Continuum
# Reads `continuum tray-data` JSON. Renders native NotifyIcon menu.
# Run: powershell -WindowStyle Hidden -File continuum-tray.ps1

Add-Type -AssemblyName System.Windows.Forms
Add-Type -AssemblyName System.Drawing

# Find CLI: native Windows exe, or route through WSL
$script:useWsl = $false
$script:continuum = "$env:LOCALAPPDATA\continuum\continuum.exe"
if (-not (Test-Path $script:continuum)) {
    # Try WSL — the CLI is a bash script inside WSL
    $wslCheck = & wsl.exe --exec which continuum 2>$null
    if ($wslCheck) {
        $script:useWsl = $true
    }
}

function Invoke-Continuum {
    param([string[]]$Args)
    if ($script:useWsl) {
        return & wsl.exe --exec continuum @Args 2>$null
    } else {
        return & $script:continuum @Args 2>$null
    }
}

function Get-TrayData {
    try {
        $json = Invoke-Continuum "tray-data"
        return $json | ConvertFrom-Json
    } catch {
        return @{
            status = "red"
            statusText = "CLI not found"
            docker = $false
            nodes = @()
            actions = @()
        }
    }
}

function Get-StatusIcon($status) {
    $bmp = New-Object System.Drawing.Bitmap(16, 16)
    $g = [System.Drawing.Graphics]::FromImage($bmp)
    $color = switch ($status) {
        "green"  { [System.Drawing.Color]::LimeGreen }
        "yellow" { [System.Drawing.Color]::Gold }
        default  { [System.Drawing.Color]::Red }
    }
    $g.FillEllipse((New-Object System.Drawing.SolidBrush($color)), 2, 2, 12, 12)
    $g.Dispose()
    return [System.Drawing.Icon]::FromHandle($bmp.GetHicon())
}

# Create NotifyIcon
$tray = New-Object System.Windows.Forms.NotifyIcon
$tray.Text = "Continuum"
$tray.Visible = $true

# Context menu
function Update-Menu {
    $data = Get-TrayData
    $tray.Icon = Get-StatusIcon $data.status
    $tray.Text = "Continuum — $($data.statusText)"

    $menu = New-Object System.Windows.Forms.ContextMenuStrip

    # Status header
    $header = $menu.Items.Add($data.statusText)
    $header.Enabled = $false
    $menu.Items.Add("-")

    # Actions
    foreach ($action in $data.actions) {
        $item = $menu.Items.Add($action.label)
        $cmd = $action.command
        $item.Add_Click({
            if ($script:useWsl) {
                Start-Process wsl.exe -ArgumentList "--exec $cmd" -WindowStyle Hidden
            } else {
                Start-Process cmd -ArgumentList "/c $cmd" -WindowStyle Hidden
            }
        })
    }

    # Grid nodes
    if ($data.nodes.Count -gt 0) {
        $menu.Items.Add("-")
        foreach ($node in $data.nodes) {
            $icon = if ($node.online) { "●" } else { "○" }
            $item = $menu.Items.Add("$icon $($node.name)")
            if ($node.url) {
                $url = $node.url
                $item.Add_Click({ Start-Process $url })
            } else {
                $item.Enabled = $false
            }
        }
    }

    $menu.Items.Add("-")
    $quit = $menu.Items.Add("Quit Continuum Tray")
    $quit.Add_Click({
        $tray.Visible = $false
        [System.Windows.Forms.Application]::Exit()
    })

    $tray.ContextMenuStrip = $menu
}

# Double-click opens browser
$tray.Add_DoubleClick({
    $data = Get-TrayData
    $url = if ($data.nodes | Where-Object { $_.uiOk }) {
        ($data.nodes | Where-Object { $_.uiOk } | Select-Object -First 1).url
    } else {
        "http://localhost:9003"
    }
    Start-Process $url
})

# Initial update + timer
Update-Menu
$timer = New-Object System.Windows.Forms.Timer
$timer.Interval = 30000 # 30 seconds
$timer.Add_Tick({ Update-Menu })
$timer.Start()

[System.Windows.Forms.Application]::Run()
