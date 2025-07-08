-- Opera Tab Manager AppleScript
-- Provides tab counting, closing, and focusing for Opera GX

-- Count tabs matching URL pattern
on countTabs(urlPattern)
    tell application "Opera GX"
        set tabCount to 0
        repeat with w in windows
            repeat with t in tabs of w
                set currentURL to URL of t
                -- Match ONLY the exact URL (no paths, only query params/fragments allowed)
                if (currentURL is equal to urlPattern) or ¬
                   (currentURL is equal to urlPattern & "/") or ¬
                   (currentURL starts with urlPattern & "?") or ¬
                   (currentURL starts with urlPattern & "#") then
                    set tabCount to tabCount + 1
                end if
            end repeat
        end repeat
        return tabCount
    end tell
end countTabs

-- Close tabs matching URL pattern (keeping first one)
on closeTabs(urlPattern)
    tell application "Opera GX"
        set closedCount to 0
        set foundFirst to false
        repeat with w in (get windows)
            set tabList to tabs of w
            repeat with i from (count of tabList) to 1 by -1
                set t to item i of tabList
                set currentURL to URL of t
                -- Match ONLY the exact URL (no paths, only query params/fragments allowed)
                if (currentURL is equal to urlPattern) or ¬
                   (currentURL is equal to urlPattern & "/") or ¬
                   (currentURL starts with urlPattern & "?") or ¬
                   (currentURL starts with urlPattern & "#") then
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
    tell application "Opera GX"
        -- First pass: Look for exact match only (highest priority)
        repeat with w in windows
            repeat with t in tabs of w
                set currentURL to URL of t
                if (currentURL is equal to urlPattern) then
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
                if (currentURL is equal to urlPattern & "/") then
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
                if (currentURL starts with urlPattern & "?") or ¬
                   (currentURL starts with urlPattern & "#") then
                    set active tab index of w to index of t
                    set index of w to 1
                    activate
                    return "found-params"
                end if
            end repeat
        end repeat
        
        return "not found"
    end tell
end focusTab