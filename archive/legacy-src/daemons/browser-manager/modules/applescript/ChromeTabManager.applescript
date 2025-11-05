-- Chrome Tab Manager AppleScript
-- Provides tab counting, closing, and focusing for Google Chrome

-- Count tabs matching URL pattern
on countTabs(urlPattern)
    tell application "Google Chrome"
        set tabCount to 0
        repeat with w in windows
            repeat with t in tabs of w
                if (URL of t contains urlPattern) then
                    set tabCount to tabCount + 1
                end if
            end repeat
        end repeat
        return tabCount
    end tell
end countTabs

-- Close tabs matching URL pattern (keeping first one)
on closeTabs(urlPattern)
    tell application "Google Chrome"
        set closedCount to 0
        set foundFirst to false
        repeat with w in (get windows)
            set tabList to tabs of w
            repeat with i from (count of tabList) to 1 by -1
                set t to item i of tabList
                if (URL of t contains urlPattern) then
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
end closeTabs

-- Focus tab matching URL pattern
on focusTab(urlPattern)
    tell application "Google Chrome"
        repeat with w in windows
            repeat with t in tabs of w
                if (URL of t contains urlPattern) then
                    set active tab index of w to index of t
                    set index of w to 1
                    activate
                    return "found"
                end if
            end repeat
        end repeat
        return "not found"
    end tell
end focusTab