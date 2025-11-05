try
	tell application "{{APP_NAME}}"
		return version
	end tell
on error
	return "unknown"
end try