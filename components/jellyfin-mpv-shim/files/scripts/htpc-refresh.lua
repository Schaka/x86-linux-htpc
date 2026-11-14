-- SPDX-License-Identifier: AGPL-3.0-only
--
-- Gives the display the refresh rate of the video, for the mpv of the
-- distribution inside Plasma. No Wayland protocol lets a program change the
-- display mode, so the script asks KWin through kscreen-doctor.
--
-- When a file starts, the script picks the mode with the size of the
-- current mode and the refresh rate that matches the frame rate. When mpv
-- has no file or ends, it sets the mode from before the first change.
--
-- The script-opts key htpc_refresh-output names the output, for example
-- HDMI-A-1.

local utils = require("mp.utils")

local options = { output = "" }
require("mp.options").read_options(options, "htpc_refresh")

-- A rate matches a frame rate if it is the same or two times the same, with
-- this difference at most. 23.976 and 24 differ by 0.1 %, so they do not
-- match each other.
local TOLERANCE = 0.0005

-- The mode before the first change, as a kscreen-doctor mode id
local original_mode = nil
-- The mode that the script set last
local current_mode = nil

local function kscreen(args)
    local result = mp.command_native({
        name = "subprocess",
        args = args,
        capture_stdout = true,
        capture_stderr = true,
        playback_only = false,
    })
    if result.status ~= 0 then
        mp.msg.warn("kscreen-doctor failed: " .. (result.stderr or ""))
        return nil
    end
    return result.stdout
end

local function find_output()
    local json = kscreen({ "kscreen-doctor", "-j" })
    local data = json and utils.parse_json(json)
    if not data then
        return nil
    end
    for _, output in ipairs(data.outputs or {}) do
        if output.name == options.output and output.enabled then
            return output
        end
    end
end

local function mode_by_id(output, id)
    for _, mode in ipairs(output.modes or {}) do
        if mode.id == id then
            return mode
        end
    end
end

-- The mode of the same size whose rate is the frame rate, or else two times
-- the frame rate. A TV shows 25 fps at 25 Hz or 50 Hz without judder.
local function matching_mode(output, current, fps)
    for _, factor in ipairs({ 1, 2 }) do
        local target = fps * factor
        for _, mode in ipairs(output.modes or {}) do
            if mode.size.width == current.size.width
                and mode.size.height == current.size.height
                and math.abs(mode.refreshRate - target) / target <= TOLERANCE then
                return mode
            end
        end
    end
end

local function set_mode(id)
    if kscreen({ "kscreen-doctor", "output." .. options.output .. ".mode." .. id }) then
        current_mode = id
        return true
    end
end

local function restore()
    if original_mode and current_mode ~= original_mode then
        mp.msg.info("Display mode back to mode " .. original_mode)
        set_mode(original_mode)
    end
    original_mode = nil
    current_mode = nil
end

-- The hook runs before mpv opens the video output, so the first frame
-- already shows in the new mode.
mp.add_hook("on_preloaded", 50, function()
    if options.output == "" then
        return
    end
    -- In this hook, container-fps has no value and no track is selected
    -- yet. The first video track has the frame rate of the demuxer.
    local fps = nil
    for _, track in ipairs(mp.get_property_native("track-list", {})) do
        if track.type == "video" and not track.image and not track.albumart then
            fps = track["demux-fps"]
            break
        end
    end
    if not fps or fps <= 0 then
        mp.msg.verbose("The video has no frame rate.")
        return
    end
    local output = find_output()
    local current = output and mode_by_id(output, output.currentModeId)
    if not current then
        return
    end
    -- After the user changed the mode by hand, that mode is the new one to
    -- come back to.
    if current.id ~= current_mode then
        original_mode = current.id
    end
    local mode = matching_mode(output, current, fps)
    if not mode then
        mp.msg.info(string.format("No display mode for %.3f fps", fps))
        return
    end
    if mode.id ~= current.id then
        -- KWin rounds the rate in the mode name, for example 59.94 to 60.
        mp.msg.info(string.format("Display mode %dx%d at %.3f Hz for %.3f fps",
            mode.size.width, mode.size.height, mode.refreshRate, fps))
        set_mode(mode.id)
    else
        current_mode = current.id
    end
end)

-- The shim keeps mpv after the end of a video, without a file. Between two
-- episodes, mpv has no file for a short time. The wait keeps the TV from a
-- second resync in that gap.
local RESTORE_DELAY = 2
local restore_timer = nil

mp.observe_property("idle-active", "bool", function(_, idle)
    if restore_timer then
        restore_timer:kill()
        restore_timer = nil
    end
    if idle then
        restore_timer = mp.add_timeout(RESTORE_DELAY, restore)
    end
end)

mp.register_event("shutdown", restore)
