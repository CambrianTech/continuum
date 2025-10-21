tell application "{{APP_NAME}}"
	set closedCount to 0
	set foundFirst to false
	repeat with w in (get windows)
		set tabList to tabs of w
		repeat with i from (count of tabList) to 1 by -1
			set t to item i of tabList
			set currentURL to URL of t
			-- Match ONLY the exact URL (no paths, only query params/fragments allowed)
			if (currentURL is equal to "{{URL_PATTERN}}") or ¬
			   (currentURL is equal to "{{URL_PATTERN}}/") or ¬
			   (currentURL starts with "{{URL_PATTERN}}?") or ¬
			   (currentURL starts with "{{URL_PATTERN}}#") then
				if foundFirst then
					close t
					set closedCount to closedCount + 1
				else
					set foundFirst to true
				end if
			end if
		end repeat
	end repeat
	return closedCount
end tell