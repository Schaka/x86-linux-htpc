# Desktop

The TV interface is KDE Plasma Bigscreen. It is a Plasma session for TVs, with large tiles and control by remote keys. It runs on KWin with Wayland.

## Session

| Topic | Design |
|---|---|
| Package | `plasma-bigscreen` of the distribution |
| Login | The display manager logs in the HTPC user automatically and starts the session `plasma-bigscreen-wayland`. Fedora 45 uses Plasma Login Manager, other distributions use SDDM. Both read an `[Autologin]` drop-in file. |
| Screen lock | Off. The HTPC has no personal data on the screen. |
| Direct scanout | Off (`KWIN_DRM_NO_DIRECT_SCANOUT=1` in `~/.config/environment.d/`). With the Android TV image of Waydroid on Intel GPUs, the direct scanout of KWin shows black flicker and shifted video on the TV. A screenshot does not show it. The variable applies at the next login. |
| KWallet | Off. The automatic login has no password to unlock the wallet, so each request of a program would show a password dialog on the TV. |
| Automatic suspend | Off. A suspended HTPC cannot receive a cast. |
| Screen edges | No action. The pointer of an air mouse often reaches a corner by accident. In a corner, KWin opens its overview, and a remote cannot close it. |
| User | A normal user without administrator rights. The user name is in `/etc/x86-linux-htpc/htpc.conf`. |

## Display

At each login, the user service `htpc-display.service` of the component `bigscreen` sets the output mode from the configuration file: the resolution and the refresh rate from `DISPLAY_MODE`, and HDR from `DISPLAY_HDR`. It uses `kscreen-doctor` and selects the mode by its ID, because the name of a mode has the rounded rate only. So `3840x2160@60` gets 60 Hz and not 59.94 Hz. If the TV offers two modes with the rate, the service keeps the current one. A change in the display settings of Plasma lasts until the next login. The players change the refresh rate for each video. [video.md](video.md) explains this.

## Launcher entries

Plasma Bigscreen shows the applications that have a desktop entry (a `.desktop` file). The install scripts add these entries:

| Entry | Starts |
|---|---|
| Jellyfin | The library browser of `jellyfin-mpv-shim`. The shim runs in the background and also waits for a cast from a Jellyfin client. |
| Kodi | Kodi with CB1 on its own VT |
| Each Android app | The app in Waydroid |

## Home screen

The component `home-screen` sets the apps of the home screen from `HOME_SCREEN_APPS` in the configuration file. The value is a list of desktop file names. The home screen then shows these apps in its Favorites row, and the rows Recent, Applications and Games are off. The search of the home screen still finds all apps.

| Part | Where Bigscreen keeps it |
|---|---|
| Favorites | `~/.config/bigscreen-favs`, one group `[Favs][N]` for each app. Bigscreen reads it when the Plasma shell starts, so the component restarts the shell. |
| Rows | The D-Bus interface `org.kde.biglauncher` of the home screen, methods `setShowRecent`, `setShowApplications` and `setShowGames`. The home screen saves the values itself. |

The Home key of a remote (`Home Page`) runs the global shortcut "Toggle Bigscreen Home Screen". It shows the home screen over an open app, for example over Android. Inside the CB1 mpv, the same key hides the window of `jellyfin-mpv-shim` and goes back to Plasma.

Bigscreen sorts the Favorites row in its own order, not in the order of `HOME_SCREEN_APPS`.

The component comes last in the install order, because the other components add the desktop entries of the apps.

## Focus of the home screen

The Bigscreen home screen shows its tiles only while its window has the keyboard focus. Without the focus, the TV shows only the wallpaper, and the keys of a remote reach no window. A click at the pointer can still start an app from a tile that the TV does not show.

When the TV output goes away and comes back, Plasma creates the window of the home screen again. Two events do this: the TV or the receiver turns on, or a player switches to its own VT. If another window takes the focus at that moment and closes, no window has the focus.

The component `bigscreen` installs the KWin script `x86-linux-htpc-home` (folder `~/.local/share/kwin/scripts/`). It has two jobs:

| Job | How |
|---|---|
| Focus | One second after no window has the focus, or after the active window is minimized, the script gives the focus to the home screen. |
| Back from a player | The CB1 mpv and Kodi play on their own VT and open no Plasma window, so the launch screen of their tile stays. When they give the screen back, they call the KWin shortcut "x86-linux-htpc show home". The script closes the launch screen, minimizes the other windows and gives the focus to the home screen. It tries again every half second for 10 seconds, because KWin gets the display back a moment after the VT switch. |

## Power

The HTPC stays on at all times. A suspended HTPC cannot receive a cast from Jellyfin. The component `bigscreen` makes logind and Plasma ignore the power key, the sleep key, the lid and the idle time. The screen neither dims nor turns off. It also masks the sleep targets. So nothing suspends or hibernates the HTPC, and no key shuts it down. To turn it off, run `systemctl poweroff`.

The power key of a remote turns the TV and the receiver on and off through the component `power-key`. The HTPC stays on. [power-key.md](power-key.md) describes it.
