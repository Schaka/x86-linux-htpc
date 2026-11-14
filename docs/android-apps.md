# Android apps

The HTPC runs Android TV apps for YouTube and Twitch in Waydroid. Waydroid runs a full Android system in a container and shows it as a window in Wayland. It shares the kernel of the host.

## Apps

| App | Service | Source | Code |
|---|---|---|---|
| SmartTube | YouTube, with an Android TV interface | GitHub releases of `yuliskov/SmartTube`. All releases there are beta releases. | The universal APK, with x86 and ARM code |
| TizenTube Cobalt | YouTube, with the TV interface of YouTube | GitHub releases of `reisxd/TizenTubeCobalt` | ARM only. libhoudini in the image translates it. Some YouTube sessions of the TV client get only encrypted video. Then no video plays, because the image has no secure decoder. |
| S0undTV | Twitch, with an Android TV interface | GitHub releases of `S0und/S0undTV`. Each beta build is an APK in the release `beta`. After the first install, the app updates itself. | The APK has x86 and ARM code |

The component `android-apps` installs the apps of `ANDROID_APPS` in the configuration file. `files/apps.conf` of the component lists each app with its repository and the name pattern of its APK. The script takes the newest release with a matching APK, also a beta release. In a release with several matching APKs, the newest upload wins. It compares the download with the SHA256 digest that GitHub lists for the file, and installs it with the package manager of Android. A new release replaces the app at the next install run. An app that the component installed earlier, and that `ANDROID_APPS` no longer lists, is removed from Android and from the home screen.

Waydroid makes desktop entries only for apps with a phone launcher icon, and TV apps have none. So the component writes the entry `htpc-android-NAME.desktop` for each app. The component `home-screen` puts these entries on the Bigscreen home screen.

Twitch puts its ads into the stream itself. S0undTV removes them only through a proxy server in a country without Twitch ads. The component writes `TWITCH_PROXY_URL` of the configuration file into S0undTV, as "Custom server 1", and makes it the active proxy server. In S0undTV, the value is in the menu `Settings` under `Proxy server`. The skill `android-app-settings` describes how to find and write such a value of an Android app.

## Android image

Waydroid needs an Android image. Two kinds exist:

1. The standard LineageOS image of Waydroid. It has a phone and tablet interface, and it decodes video in software.
2. An Android TV image for x86_64 from the WayDroid-ATV project. It has the TV launcher, so TV apps behave as on a TV device. It contains libhoudini for ARM apps and the Intel media driver for VA-API decode.

The component `waydroid` installs the Android TV 16 image of WayDroid-ATV by default. `WAYDROID_SYSTEM_OTA` and `WAYDROID_VENDOR_OTA` in the configuration file name its OTA channels. `waydroid init` downloads the image and compares it with the SHA256 sum of the channel. The image contains Google services, but the apps do not need them.

## Kernel

Waydroid needs the binder driver of Android. Fedora builds it into the kernel, and Waydroid mounts the binder file system by itself. Other distributions can need a module or a kernel option.

On Fedora, SELinux runs in enforcing mode. The policy of `waydroid-selinux` runs Android in the domain `container_runtime_t`. Some services of the Android TV image start in the domain `spc_t`, and SELinux denies their binder calls to the other domain. Android then does not finish its start. The component `waydroid` loads the SELinux module `htpc-waydroid` from `files/htpc-waydroid.te`. It allows only the binder calls between these two domains.

## Display, sound and keys

| Topic | Behavior |
|---|---|
| Window | Waydroid shows the full Android interface full screen. Each app also has its own launcher entry in Plasma Bigscreen. |
| Video decode | The app decodes the video through the codecs of Android. The images of WayDroid-ATV contain the Intel media driver and decode video through VA-API on Intel and AMD GPUs. The standard LineageOS image of Waydroid decodes in software. The Android TV image turns on zero-copy decoding on Intel GPUs at each start (`debug.ffmpeg-codec2.hwaccel.drm=1`). The decoder then takes about 12 % of one CPU core for 4K VP9. With the direct scanout of KWin, these frames show shifted and black parts on the TV, and the Android interface flickers. So the component `bigscreen` turns direct scanout off. [desktop.md](desktop.md) describes the setting. Software decode of 4K VP9 or AV1 is too slow for an N100 class CPU. With the hardware decoder, SmartTube plays 4K VP9 at 60 fps on an N100 class GPU. |
| HDR | Waydroid sends no HDR to the host. Android reports a display without HDR types and without wide color, so YouTube sends SDR video. The apps play SDR, also when Plasma is in HDR mode. |
| Sound | Waydroid plays PCM through PipeWire. Passthrough is not possible. |
| Keys | The arrow keys, Enter and Escape reach Android as the keys of a TV remote |
