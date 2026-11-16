// SPDX-License-Identifier: AGPL-3.0-only
//
// A KWin script that keeps the Plasma Bigscreen home screen usable on the
// TV. It has two jobs.
//
// 1. The home screen shows its tiles only while its window has the
//    keyboard focus. Without the focus, the TV shows only the wallpaper,
//    and the keys of a remote reach no window. When the TV output goes
//    away and comes back, Plasma creates the window of the home screen
//    again, and a window that takes the focus at that moment can close
//    again. Then no window has the focus. The script gives the focus to the
//    home screen when no window has it.
//
// 2. The CB1 mpv and Kodi play on their own VT and open no window in
//    Plasma. After a tile starts one of them, the launch screen of
//    Bigscreen waits for a window that never comes. The players call the
//    shortcut SHOW_HOME when they give the screen back to Plasma. The
//    script then closes the launch screen, minimizes the other windows and
//    gives the focus to the home screen.

const SHOW_HOME = "x86-linux-htpc show home";
// The launch screen of Bigscreen is a normal window of plasmashell with
// this class. The window of the home screen has the class "plasmashell".
const LAUNCH_SCREEN_CLASS = "org.kde.plasmashell";
// A window that takes the focus can close a moment later, so the script
// waits before it decides that no window has the focus.
const FOCUS_DELAY_MS = 1000;
// KWin gets the display back a moment after the switch to its VT, and
// Plasma creates the window of the home screen again only after that.
const SHOW_HOME_INTERVAL_MS = 500;
const SHOW_HOME_TRIES = 20;

function homeScreen() {
    return workspace.stackingOrder.find(window => window.desktopWindow);
}

function isLaunchScreen(window) {
    return window.normalWindow && window.resourceClass === LAUNCH_SCREEN_CLASS;
}

function nothingHasFocus() {
    const active = workspace.activeWindow;
    return !active || active.minimized;
}

function focusHomeScreen() {
    const home = homeScreen();
    if (home) {
        workspace.activeWindow = home;
    }
    return home !== undefined && workspace.activeWindow === home;
}

const focusTimer = new QTimer();
focusTimer.singleShot = true;
focusTimer.interval = FOCUS_DELAY_MS;
focusTimer.timeout.connect(() => {
    if (nothingHasFocus()) {
        focusHomeScreen();
    }
});

function checkFocusSoon() {
    focusTimer.start();
}

workspace.windowActivated.connect(window => {
    if (!window) {
        checkFocusSoon();
    }
});
workspace.windowRemoved.connect(checkFocusSoon);
workspace.windowAdded.connect(window => {
    if (window.desktopWindow) {
        checkFocusSoon();
    }
});
workspace.screensChanged.connect(checkFocusSoon);

function clearScreenForHome() {
    for (const window of workspace.stackingOrder) {
        if (isLaunchScreen(window)) {
            window.closeWindow();
        } else if (window.normalWindow && !window.minimized) {
            window.minimized = true;
        }
    }
}

let showHomeTriesLeft = 0;
const showHomeTimer = new QTimer();
showHomeTimer.interval = SHOW_HOME_INTERVAL_MS;

function tryShowHome() {
    clearScreenForHome();
    const done = focusHomeScreen() && !workspace.stackingOrder.some(isLaunchScreen);
    showHomeTriesLeft -= 1;
    if (done || showHomeTriesLeft <= 0) {
        showHomeTimer.stop();
    }
}

showHomeTimer.timeout.connect(tryShowHome);

registerShortcut(SHOW_HOME, "x86-linux-htpc: Show the home screen", "", () => {
    showHomeTriesLeft = SHOW_HOME_TRIES;
    showHomeTimer.start();
    tryShowHome();
});

checkFocusSoon();
