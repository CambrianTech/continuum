# Install script for Windows

Write-Output "Starting installation..."

# Check if the service already exists
$serviceName = "MyWindowsService"
$serviceExists = Get-Service -Name $serviceName -ErrorAction SilentlyContinue

if ($serviceExists) {
    Write-Output "Service already installed."
} else {
    # Install the service
    Write-Output "Installing service..."
    New-Service -Name $serviceName -BinaryPathName "C:\Path\To\Your\Executable.exe" -StartType Automatic

    if ($?) {
        Write-Output "Service installed successfully."
    } else {
        Write-Error "Failed to install service."
    }

    # Start the service if it was just installed
    Start-Service -Name $serviceName
    if ($?) {
        Write-Output "Service started successfully."
    } else {
        Write-Error "Failed to start service."
    }
}