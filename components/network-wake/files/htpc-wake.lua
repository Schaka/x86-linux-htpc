-- SPDX-License-Identifier: AGPL-3.0-only
--
-- Wakes the TV over the network when a file starts, so a cast from a
-- Jellyfin client turns on a TV in standby. The component network-wake
-- installs this script for both mpv programs of jellyfin-mpv-shim.

local WAKE = "/usr/local/libexec/x86-linux-htpc/wake-devices"

-- The packets go out in the background, so the start of the file does
-- not wait for them.
mp.add_hook("on_preloaded", 40, function()
    mp.command_native_async({
        name = "subprocess",
        args = { WAKE },
        playback_only = false,
    }, function(success, result)
        if not success or result.status ~= 0 then
            mp.msg.warn("wake-devices failed: " .. tostring(result and result.stderr))
        end
    end)
end)
