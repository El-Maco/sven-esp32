@echo off
setlocal

:: List USB devices
usbipd list

:: Prompt for action (a/d)
:PromptAction
set /p ACTION="Do you want to (a)ttach or (d)tach:"

:AttachUSB
:: Prompt the user for the bus ID
set /p BUSID="Enter the bus ID of the USB device to bind (e.g. 1-1): "

:: Bind the USB device
usbipd bind --busid %BUSID%

:: Check if the bind command was successful
if %errorlevel% equ 0 (
    echo USB device bound successfully!
) else (
    echo Failed to bind the USB device. Please check the bus ID and try again.
)
goto :EOF

:DetachUSB
usbipd detach --budid 1-7
goto :EOF

if /i "%ACTION%"=="a" (
    call :AttachUSB
) else (
    call :DetachUSB
)

pause
endlocal
