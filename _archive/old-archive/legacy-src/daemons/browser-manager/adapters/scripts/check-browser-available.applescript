try
	tell application "{{APP_NAME}}"
		return "true"
	end tell
on error
	return "false"
end try