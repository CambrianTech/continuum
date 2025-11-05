tell application "{{APP_NAME}}"
	-- First pass: Look for exact match only (highest priority)
	repeat with w in windows
		set tabIndex to 1
		repeat with t in tabs of w
			set currentURL to URL of t
			if (currentURL is equal to "{{URL_PATTERN}}") then
				set active tab index of w to tabIndex
				set index of w to 1
				activate
				-- Refresh the tab
				tell t to reload
				return "refreshed-exact"
			end if
			set tabIndex to tabIndex + 1
		end repeat
	end repeat
	
	-- Second pass: Look for exact match with trailing slash
	repeat with w in windows
		set tabIndex to 1
		repeat with t in tabs of w
			set currentURL to URL of t
			if (currentURL is equal to "{{URL_PATTERN}}/") then
				set active tab index of w to tabIndex
				set index of w to 1
				activate
				-- Refresh the tab
				tell t to reload
				return "refreshed-slash"
			end if
			set tabIndex to tabIndex + 1
		end repeat
	end repeat
	
	-- Third pass: Look for query params or fragments
	repeat with w in windows
		set tabIndex to 1
		repeat with t in tabs of w
			set currentURL to URL of t
			if (currentURL starts with "{{URL_PATTERN}}?") or ¬
			   (currentURL starts with "{{URL_PATTERN}}#") then
				set active tab index of w to tabIndex
				set index of w to 1
				activate
				-- Refresh the tab
				tell t to reload
				return "refreshed-params"
			end if
			set tabIndex to tabIndex + 1
		end repeat
	end repeat
	
	return "not found"
end tell