tell application "{{APP_NAME}}"
	set tabCount to 0
	repeat with w in windows
		repeat with t in tabs of w
			set currentURL to URL of t
			-- Match ONLY the exact URL (no paths, only query params/fragments allowed)
			if (currentURL is equal to "{{URL_PATTERN}}") or ¬
			   (currentURL is equal to "{{URL_PATTERN}}/") or ¬
			   (currentURL starts with "{{URL_PATTERN}}?") or ¬
			   (currentURL starts with "{{URL_PATTERN}}#") then
				set tabCount to tabCount + 1
			end if
		end repeat
	end repeat
	return tabCount
end tell