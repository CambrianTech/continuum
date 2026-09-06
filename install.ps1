# Install script for Windows
# This script installs a Windows service with supervision and optional uu.cmd shim.

function Ensure-ServiceSupervision {
    param (
        [string]$serviceName,
        [string]$executablePath
    )

    # Check if the service already exists
    $serviceExists = Get-Service -Name $serviceName -ErrorAction SilentlyContinue

    if ($serviceExists) {
        Write-Output "Service '$serviceName' already installed."
        return $true
    } else {
        Write-Output "Installing service '$serviceName'..."

        # Supervised Service Path with uu.cmd shim
        $shimPath = "C:\Program Files\MyWindowsService\uu.cmd"

        if (Test-Path -Path $executablePath) {
            Write-Output "Found executable at '$executablePath'. Creating supervision script..."
            # Create the uu.cmd shim file with content to call your actual executable
            Add-Content -Path $shimPath -Value "@echo off`nCALL $executablePath"
        } else {
            Write-Error "Executable not found at '$executablePath'. Aborting."
            return $false
        }

        # Install the service using the shim path
        New-Service -Name $serviceName -BinaryPathName $shimPath -StartType Automatic

        if ($?) {
            Write-Output "Supervised service installed successfully."
            Start-Service -Name $serviceName
            if ($?) {
                Write-Output "Supervised service started successfully."
                return $true
            } else {
                Write-Error "Failed to start supervised service."
                return $false
            }
        } else {
            Write-Error "Failed to install supervised service."
            return $false
        }
    }
}

Write-Output "Starting installation..."

# Check if the service already exists
$serviceName = "MyWindowsService"
$serviceExists = Get-Service -Name $serviceName -ErrorAction SilentlyContinue

if ($serviceExists) {
    Write-Output "Service already installed."
} else {
# Install the service using the supervised path
    Ensure-ServiceSupervision -serviceName $serviceName -executablePath "C:\Path\To\Your\Executable.exe"

    if ($?) {
        Write-Output "Supervised service installed successfully."
    } else {
        Write-Error "Failed to install supervised service."
    }

    # Start the service if it was just installed
    Start-Service -Name $serviceName
    if ($?) {
        Write-Output "Service started successfully."
    } else {
        Write-Error "Failed to start service."
    }
}