-- SPDX-License-Identifier: AGPL-3.0-only
--
-- Sets the output of the CB1 mpv for each video. The DRM output reads its
-- options only when it starts, so this script sets them before mpv opens
-- the decoder and the output:
--
--   * Dolby Vision videos play with CB1 in the Dolby Vision mode, on an
--     XRGB8888 plane without dithering.
--   * All other videos play with the HDR output of mpv, on a 10-bit plane.
--   * The display mode gets the refresh rate of the video.
--
-- The script-opts key htpc_output-resolution sets the size of the display
-- mode, for example 3840x2160. Without it, the display keeps its preferred
-- mode.

local options = { resolution = "" }
require("mp.options").read_options(options, "htpc_output")

-- The profiles that CB1 plays as TV-led Dolby Vision
local cb1_profiles = { [5] = true, [7] = true, [8] = true }

-- The refresh rates of TV modes. A video plays at the rate nearest to its
-- frame rate. mpv names a mode by its rate with two decimals.
local rates = { 23.976, 24, 25, 29.97, 30, 50, 59.94, 60 }

local function rate_name(rate)
    return (string.format("%.2f", rate):gsub("%.?0+$", ""))
end

local function display_mode(fps)
    if options.resolution == "" or not fps then
        return "preferred"
    end
    local best = rates[1]
    for _, rate in ipairs(rates) do
        if math.abs(rate - fps) < math.abs(best - fps) then
            best = rate
        end
    end
    return options.resolution .. "@" .. rate_name(best)
end

local function first_video_track()
    for _, track in ipairs(mp.get_property_native("track-list", {})) do
        if track.type == "video" and not track.image and not track.albumart then
            return track
        end
    end
end

local function output_settings(track)
    local dolby_vision = track and cb1_profiles[track["dolby-vision-profile"]]
    local settings = {
        ["drm-mode"] = display_mode(track and track["demux-fps"]),
    }
    if dolby_vision then
        settings["cb1"] = "dolby-vision"
        settings["drm-dvbridge"] = "dolby-vision"
        settings["drm-format"] = "xrgb8888"
        settings["dither-depth"] = "no"
        settings["target-colorspace-hint"] = "no"
    else
        settings["cb1"] = "no"
        settings["drm-dvbridge"] = "no"
        settings["drm-format"] = "xrgb2101010"
        settings["dither-depth"] = "auto"
        settings["target-colorspace-hint"] = "yes"
    end
    return settings
end

-- The options that only a new start of the DRM output applies
local restart_options = { "drm-mode", "drm-format", "drm-dvbridge" }

mp.add_hook("on_preloaded", 50, function()
    local track = first_video_track()
    if not track then
        return
    end
    local settings = output_settings(track)
    local restart = false
    for _, name in ipairs(restart_options) do
        if mp.get_property(name) ~= settings[name] then
            restart = true
        end
    end
    -- A video output that runs from an earlier file or from the menu
    -- window must start again with the new options. mpv ignores a set of
    -- the same value, so the output goes to "null" until the options are
    -- set. Else the old output draws its last frame with the new options.
    local vo = mp.get_property("vo")
    restart = restart and mp.get_property_bool("vo-configured")
    if restart then
        mp.set_property("vo", "null")
    end
    for name, value in pairs(settings) do
        mp.set_property(name, value)
    end
    if restart then
        mp.set_property("vo", vo)
    end
    mp.msg.info(string.format("Output for this video: CB1 %s, mode %s",
        settings["cb1"], settings["drm-mode"]))
end)
