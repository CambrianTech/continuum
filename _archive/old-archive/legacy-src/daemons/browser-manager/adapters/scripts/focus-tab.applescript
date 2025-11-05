tell application "{{APP_NAME}}"
	-- First pass: Look for exact match only (highest priority)
	repeat with w in windows
		repeat with t in tabs of w
			set currentURL to URL of t
			if (currentURL is equal to "{{URL_PATTERN}}") then
				set active tab index of w to index of t
				set index of w to 1
				activate
				return "found-exact"
			end if
		end repeat
	end repeat
	
	-- Second pass: Look for exact match with trailing slash
	repeat with w in windows
		repeat with t in tabs of w
			set currentURL to URL of t
			if (currentURL is equal to "{{URL_PATTERN}}/") then
				set active tab index of w to index of t
				set index of w to 1
				activate
				return "found-slash"
			end if
		end repeat
	end repeat
	
	-- Third pass: Look for query params or fragments
	repeat with w in windows
		repeat with t in tabs of w
			set currentURL to URL of t
			if (currentURL starts with "{{URL_PATTERN}}?") or ¬
			   (currentURL starts with "{{URL_PATTERN}}#") then
				set active tab index of w to index of t
				set index of w to 1
				activate
				return "found-params"
			end if
		end repeat
	end repeat
	
	return "not found"
end tell