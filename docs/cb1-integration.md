# CB1 integration

This page explains how a player uses the CB1 engine, and where each part goes in mpv. The facts come from the CB1 source and the Kodi patches of libreelec-x86-DV, at the commit in [kernel/upstream.conf](../kernel/upstream.conf):

| Source | Path in libreelec-x86-DV |
|---|---|
| CB1 library and its documents | `CB1/src/`, `CB1/docs/API.md`, `CB1/docs/BUILD.md` |
| FFmpeg and libplacebo changes of CB1 | `CB1/patches/ffmpeg/`, `CB1/patches/libplacebo/` |
| Pinned FFmpeg and libplacebo versions | `CB1/config/dependencies.json` |
| Kodi renderer | `projects/Generic/patches/kodi/kodi-9990-cb1-video.patch`, file `DVBridgeGLES.cpp` |
| Kodi display and DRM state | `kodi-9992-cb1-display.patch`, files `DVBridgeState.cpp` and `WinSystemGbmGLESContext.cpp` |

## What CB1 is

CB1 is a static C library (`dvbridge_core`). It runs inside the render path of the player. It has no own process, window or HDMI code.

| The player owns | CB1 owns |
|---|---|
| Demux, decode, timing, audio, subtitles, menus | The reconstruction of the Dolby Vision picture from the base layer, the second layer and the RPU (the Dolby Vision metadata of each frame) |
| The OpenGL ES context and the output surfaces | The output picture, with the Dolby Vision metadata packed into the pixels for TV-led output |
| The DRM connector properties and the page flips | The HDR10+ packet for each frame, which the player sends to the kernel |

The interface uses FFmpeg and libplacebo types. It is not a stable binary interface. CB1, FFmpeg and libplacebo must come from the same pinned versions with the CB1 patches, and the player must link exactly these builds.

## Requirements for the player

| Requirement | Reason |
|---|---|
| FFmpeg 9.0 with the CB1 patches | The patches add the raw RPU bytes and the extension blocks to the Dolby Vision side data of each frame. CB1 reads these fields. |
| libplacebo at the pinned commit with the CB1 patches | CB1 renders with libplacebo and uses new fields of `pl_render_params` and `pl_color_map_params` |
| OpenGL ES 3 through EGL | CB1 creates its renderer from a libplacebo GPU of type `pl_opengl`. It draws into GL framebuffers. Vulkan is not supported. |
| DRM master on its own VT | TV-led Dolby Vision needs bit-exact pixels on the HDMI link. A compositor changes the pixels. |
| One primary plane in RGB with 8 bits for each color | The Dolby Vision tunnel packs 12-bit YCbCr 4:2:2 into the RGB pixels. Scaling, dithering, color conversion or a second plane breaks it. |
| An RGB HDMI link | The kernel refuses Dolby Vision on a YCbCr link. A port that runs 4K at 60 Hz only as YCbCr 4:2:0 carries Dolby Vision only up to 4K at 30 Hz. [hardware.md](hardware.md) describes a firmware limit that causes this. |
| A 10-bit HDMI link for the HDR10 mode | 10-bit RGB at 4K 24 Hz needs a TMDS clock of 371 MHz |

## The call sequence

The Kodi renderer `CDVBridgeGLES` uses these calls of `dvbridge_render.h`, `dvbridge_gl_frame.h` and `dvbridge_placebo.h`. The list shows the path for the mode "Dolby Vision Reference" with a single-layer source. Profiles 5 and 8 have one layer. Profile 7 without its second layer uses the same path.

| Step | Call | When |
|---|---|---|
| 1. Start | `pl_opengl_create` with the current EGL display and context, then `dvbridge_retirement_owner_create` and `dvbridge_renderer_create_with_owner` | Once, after the start of the GL context |
| 2. Options | `dvbridge_renderer_set_optimized_dv`, `dvbridge_renderer_set_fel_filter`, `dvbridge_renderer_set_processing_optimizations`, `dvbridge_renderer_set_overlay_visible`, `dvbridge_renderer_set_l2_trims_disabled`, `dvbridge_renderer_set_cmv40` | Before each prepared frame. The values come from the settings of the player. |
| 3. Frame input | `dvbridge_gl_frame_import` wraps the GL textures of the decoded frame (NV12, P010 or P012) into a `pl_frame`. `dvbridge_gl_frame_set_chroma` sets the chroma location. | For each frame |
| 4. Prepare | `dvbridge_render_dv_policy_prepare_packed`. Its inputs are the policy (the mode) and the identity (stream, picture and revision numbers). Further inputs are the `pl_frame`, the bytes of `AV_FRAME_DATA_DOVI_METADATA`, the timestamp, the geometry and the output texture. | For each frame. The result is Ready, Pending, Busy or Failed. |
| 5. Wait | `dvbridge_render_dv_policy_poll`, or `dvbridge_render_dv_policy_wait` with a time limit | While the result is Pending |
| 6. Resolve | `dvbridge_render_dv_policy_resolve` writes the final picture into the output texture | When the result is Ready, before the swap |
| 7. Commit | `dvbridge_render_dv_policy_commit` | After the page flip of this picture. The next frame uses the committed state. |
| 8. Cancel | `dvbridge_render_dv_policy_cancel` | When the frame is not shown, for example after a seek |
| 9. End | `dvbridge_renderer_destroy`, `dvbridge_retirement_owner_destroy` | At the end of playback |

The identity binds each piece of work to one picture. The player gives each stream a number, counts the pictures, and raises the revision after a seek or a change of settings. CB1 refuses work with a wrong identity, so that an old frame never replaces a newer one.

The output texture is a `pl_tex` that wraps the framebuffer of the EGL window. Kodi gets it with `pl_opengl_wrap`.

A profile 5 file has no HDR10 base layer. In the HDR10 modes, CB1 builds the picture from the Dolby Vision metadata, as for player-led Dolby Vision, and writes it as PQ with BT.2020 primaries. The player then sends HDR10 metadata for this picture.

For the HDR10 and HDR10+ output modes, Kodi uses `dvbridge_render_hdr10_policy_direct` and `dvbridge_render_hdr10_policy_resolve` in place of steps 4 and 6. For HDR10+, it then gets the HDR10+ packet with `cb1_hdr10plus_vsif` and sends it with the connector property `HDR10PLUS_METADATA`.

## The second layer of profile 7 FEL

A profile 7 file has a second video layer, the enhancement layer (EL). In a Matroska file, the EL units travel inside the packets of the base layer, as NAL units of type 63. FFmpeg alone decodes only the base layer. A full EL (FEL) holds picture data. A minimal EL (MEL) holds none.

CB1 has a second decoder for the EL in `dvbridge_fel.h`. Kodi and mpv use it in the same way:

| Step | Call | When |
|---|---|---|
| 1. Start | `dvbridge_fel_create` with the codec parameters of the stream and the time base of the packets | When the stream is profile 7 with `el_present_flag` in its Dolby Vision configuration |
| 2. Device | `dvbridge_fel_device` with the VA-API device of the base layer decoder | When the base layer decoder has its hardware frames |
| 3. Input | `dvbridge_fel_submit` with each packet. At the end of the stream, `dvbridge_fel_drain`. If the drain fails, it can give the reason. | After the base layer decoder accepted the packet |
| 4. Pairing | `dvbridge_fel_take` with the timestamp of a decoded base layer picture. If the picture has its own RPU, `dvbridge_fel_take_mel` with the timestamp and the RPU. | For each base layer picture. The result is a pair, no EL picture yet, a picture to drop before the first EL picture after a seek, or a failure. `dvbridge_fel_take_mel` can also give a MEL picture that needs no EL picture. |
| 5. Reset | `dvbridge_fel_reset` | At each flush of the base layer decoder, for example after a seek |

A profile 7 MEL stream can come without EL units. If the RPU of a picture shows that the picture needs no residual, `dvbridge_fel_take_mel` gives it without an EL picture. The picture then plays with its RPU only. FFmpeg keeps the parsed metadata of an earlier picture on a picture without an RPU. So Kodi and mpv call `dvbridge_fel_take_mel` only for a picture that carries its own intact RPU in `AV_FRAME_DATA_DOVI_RPU_BUFFER`.

The filter `dovi_split` of FFmpeg takes the EL units out of the packets. The EL has its own HEVC configuration. Matroska stores it in the block addition mapping `hvcE`, and FFmpeg gives it as the side data `AV_PKT_DATA_HEVC_CONF`.

For rendering, `dvbridge_dmabuf_import` maps the VA-API surface of the EL picture to GL textures. `dvbridge_gl_frame_import` wraps them as a `pl_frame`, and the field `enhancement_layer` of the base layer frame points to it. The render calls then get the same timestamp for both layers. CB1 refuses a profile 7 FEL picture without its EL picture. If `dvbridge_map_color` with a paired EL reports `nlq_active`, the EL is a FEL.

## Connector state during Dolby Vision

When TV-led Dolby Vision starts, Kodi saves the current values and sets these properties in one atomic commit. At the end, it sets the saved values back. The kernel then sends the Dolby Vision VSIF (the HDMI packet that tells the TV the signal type) and stops changing the pixels.

| Object | Property | Value during Dolby Vision |
|---|---|---|
| Connector | `DVBRIDGE_DV_STANDARD_LAB` | 1 |
| Connector | `DVBRIDGE_HDR10_DEEP_COLOR` | 0 |
| Connector | `CB1_LLDV_FORMAT` | Off |
| Connector | `HDR_OUTPUT_METADATA` | 0 (no blob) |
| Connector | `max bpc` | 8 |
| Connector | `Broadcast RGB` | Full |
| Connector | `Colorspace` | Default |
| CRTC | `CTM`, `GAMMA_LUT`, `DEGAMMA_LUT` | 0 (no blob) |
| CRTC | `VRR_ENABLED` | 0 |
| Plane | `rotation` | 0 degrees |
| Plane | `alpha` | 65535 (opaque) |
| Plane | `pixel blend mode` | None |

The kernel keeps `DVBRIDGE_DV_STANDARD_LAB`, `DVBRIDGE_HDR10_DEEP_COLOR` and `CB1_LLDV_FORMAT` after the player ends. If a player ends without its restore, for example in a crash, the connector stays in the Dolby Vision mode. A compositor such as KWin does not change properties that it does not know. So Kodi turns these properties off in its first SDR modeset. If the CB1 mpv starts without `--drm-dvbridge`, it turns them off. At the end of a CB1 mode, it sets them to 0. Such a player also leaves `max bpc` at 8, and KWin keeps that value, so HDR10 gets an 8-bit link. So the CB1 mpv without `--drm-dvbridge` sets `max bpc` to the top of its range when it starts. At the end of a CB1 mode, it does the same.

If another plane is active on the same CRTC, Kodi does not start Dolby Vision. [kernel.md](kernel.md) describes the properties of the patched kernel.

## Connector state during the HDR10 mode

The HDR10 mode uses the table above with these changes:

| Object | Property | Value during the HDR10 mode |
|---|---|---|
| Connector | `DVBRIDGE_DV_STANDARD_LAB` | 0 |
| Connector | `DVBRIDGE_HDR10_DEEP_COLOR` | 1 |
| Connector | `HDR_OUTPUT_METADATA` | A blob with PQ and the peak of the Dolby Vision metadata |
| Connector | `max bpc` | 10 |
| Connector | `Colorspace` | BT2020_RGB |

`DVBRIDGE_HDR10_DEEP_COLOR` makes the kernel refuse a link with fewer than 10 bits. The kernel accepts it only in the same commit as the HDR metadata and the BT.2020 colorspace.

## The places in mpv

mpv 0.41 has all the parts that the integration needs. Four patches in `mpv/patches/` hold the changes. mpv links the CB1 library, so the patches contain no code of CB1. They contain only the code between mpv and the CB1 calls.

| Patch | Content |
|---|---|
| `0001-drm-dvbridge-connector-state.patch` | The option `--drm-dvbridge` and the connector state of the DRM output |
| `0002-cb1-output.patch` | The option `--cb1`, the adapter `video/out/gpu_next/cb1.c` and the build option `-Dcb1=enabled` |
| `0003-cb1-fel-decoder.patch` | The option `--vd-lavc-dovi-fel`, the EL decoder `video/decode/cb1_fel.c` and the `hvcE` configuration in the Matroska demuxer |
| `0004-drm-own-vt.patch` | The options `--drm-vt` and `--drm-master-wait`, so that mpv without root takes the screen on its own VT. A missing display mode falls back to the preferred mode. mpv needs libsystemd for the call to logind. |

| Part | File in mpv | Change |
|---|---|---|
| Options | `video/out/vo_gpu_next.c`, `video/out/drm_common.c` | `--cb1` turns CB1 on in `gpu-next`. `--drm-dvbridge` sets the connector state on the DRM output. Both take the mode `dolby-vision` or `hdr10`, and both must have the same mode. CB1 works only with `gpu-api=opengl` and `gpu-context=drm`. |
| Start and end | `video/out/gpu_next/cb1.c` | The adapter creates the CB1 renderer from the `pl_gpu` of the OpenGL context at the first frame. It destroys the renderer before the context. |
| Frame input | `video/out/vo_gpu_next.c`, function `draw_frame`, and `video/mp_image.c` | mpv maps each VA-API frame to a `pl_frame`. Hardware frames have textures only between their `acquire` and `release` calls. The adapter calls both, takes the GL textures of the planes, and imports them with `dvbridge_gl_frame_import`, as Kodi does. mpv converts the Dolby Vision side data into its own format. So the frame keeps a second reference to the FFmpeg buffer in the new field `dovi_av`. The adapter uses the frame that `pl_frame_mix_current` selects, without frame mixing, and calls `dvbridge_render_packed` in place of `pl_render_image_mix`. In the HDR10 mode, it calls `dvbridge_render_hdr10_direct` with the session of `dvbridge_hdr10_session_init` from the first frame of each video. If the video does not fill the screen, it calls `dvbridge_render_hdr10_rgb` and `dvbridge_render_hdr10`. |
| Enhancement layer | `video/decode/vd_lavc.c`, `video/decode/cb1_fel.c`, `demux/demux_mkv.c` | `--vd-lavc-dovi-fel` starts the EL decoder for a profile 7 stream with VA-API. Each base layer picture waits in `cb1_fel.c` until its EL picture is decoded, and then carries it in the new field `dovi_el` of `mp_image`. The hardware frame pool gets 16 more surfaces for the waiting pictures. If the EL fails, the video plays without it and mpv logs an error. A MEL picture that needs no EL picture plays without one. The adapter imports the EL picture and logs `CB1 enhancement layer: FEL` or `MEL`. |
| Output | `video/out/vo_gpu_next.c` | Give CB1 the texture of the swapchain frame (`swframe.fbo`) as the output texture. Draw no OSD over the picture in the first version. Later, the OSD goes through the overlay calls of CB1. |
| Commit | `video/out/vo_gpu_next.c`, function `flip_page` | After the swap of a CB1 picture, the adapter calls `dvbridge_render_commit`. If the picture was not presented, it resets the renderer. |
| Connector state | `video/out/drm_atomic.c`, `video/out/drm_common.c` | `drm_atomic_stage_dvbridge` saves the values of the table above and adds the new values to the modeset commit. In every mode, `drm_atomic_stage_other_planes_off` turns off all other planes on the CRTC, for example a cursor plane that a compositor left on. Without it, the pointer of Plasma stays over the picture of mpv. In the Dolby Vision mode, the DRM output then sends no HDR metadata of its own. In the HDR10 mode, the color space of the CB1 picture goes into the target parameters of the frame. The normal HDR code of the DRM output then sets the metadata, the colorspace and `DVBRIDGE_HDR10_DEEP_COLOR` in one commit. If a test commit shows that the mode has no 10-bit link, the output stays HDR10 with 8 bits and logs a warning. At the end of playback and at a VT switch, a commit of its own sets the saved values back. For the blob properties, it sets no blob, the CB1 transport properties go to 0, and `max bpc` goes to the top of its range. Then mpv restores the old mode. Without `--drm-dvbridge`, the DRM output turns off a CB1 transport that an earlier owner left on, and sets `max bpc` to the top of its range. `HDR10PLUS_METADATA` for HDR10+ output is open. |
| Output format | `video/out/opengl/context_drm_egl.c` and the options of mpv | CB1 needs a render target with four 8-bit channels, and the kernel accepts Dolby Vision only on an XRGB8888 plane. mpv renders into an ARGB8888 surface and shows the same buffer as an XRGB8888 framebuffer, as Kodi does. The HDR10 mode needs `drm-format=xrgb2101010`. Further options: `dither-depth=no` and the display mode of the video (`drm-mode`). |

For single-layer files, the decoder of mpv needs no change, because mpv uses the FFmpeg that it links. With the patched FFmpeg, the side data has the fields of CB1.

## Licenses

CB1 is GPL-2.0-or-later, with some files under GPL-3.0-or-later. `CB1/docs/LICENSE-MAP.md` lists each file. An mpv binary that links CB1 is therefore GPL-3.0. The mpv patches of this project stay under the license of mpv. They go into `mpv/patches/`.
