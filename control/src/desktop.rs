// SPDX-License-Identifier: AGPL-3.0-only
//! The Plasma session: the windows of the apps and the Flatpak apps.

use std::path::Path;
use std::time::Duration;

use tokio::process::Command;

const SCRIPT_NAME: &str = "htpc-control-home";

/// The KWin script of minimize_windows
const MINIMIZE_SCRIPT: &str = "for (const w of workspace.windowList()) {
    if (w.normalWindow && !w.minimized) w.minimized = true;
}
";

/// KWin runs the script after the call returns. The script must stay
/// loaded until then.
const SCRIPT_RUN_TIME: Duration = Duration::from_millis(500);

/// Minimizes all app windows through a KWin script. The home screen of
/// Plasma Bigscreen is the desktop, so it shows when no app window is open.
/// The shortcut of the Home key toggles the home screen, and Bigscreen
/// reports no state, so a toggle could hide the home screen instead.
pub async fn minimize_windows(runtime_dir: &Path) {
    let path = runtime_dir.join(format!("{SCRIPT_NAME}.js"));
    if let Err(error) = tokio::fs::write(&path, MINIMIZE_SCRIPT).await {
        tracing::warn!("cannot write the KWin script {}: {error}", path.display());
        return;
    }
    // A script of an earlier call with the same name blocks the load.
    unload_script().await;
    let script_path = path.display().to_string();
    let loaded = kwin_scripting(&["loadScript", "ss", &script_path, SCRIPT_NAME]).await;
    // busctl prints the answer as its type and value, for example "i 3".
    let script_id = loaded
        .as_deref()
        .and_then(|answer| answer.split_whitespace().nth(1))
        .filter(|id| id.chars().all(|c| c.is_ascii_digit()));
    if let Some(script_id) = script_id {
        let object = format!("/Scripting/Script{script_id}");
        busctl(&[
            "call",
            "org.kde.KWin",
            &object,
            "org.kde.kwin.Script",
            "run",
        ])
        .await;
        tokio::time::sleep(SCRIPT_RUN_TIME).await;
    }
    unload_script().await;
    let _ = tokio::fs::remove_file(&path).await;
}

/// Ends a Flatpak app with all its processes.
pub async fn kill_flatpak(app_id: &str) {
    match Command::new("flatpak")
        .args(["kill", app_id])
        .status()
        .await
    {
        Ok(status) if status.success() => {}
        Ok(status) => tracing::warn!("flatpak kill {app_id} failed: {status}"),
        Err(error) => tracing::warn!("cannot run flatpak: {error}"),
    }
}

async fn unload_script() {
    kwin_scripting(&["unloadScript", "s", SCRIPT_NAME]).await;
}

/// Calls a method of the scripting interface of KWin.
async fn kwin_scripting(method_and_arguments: &[&str]) -> Option<String> {
    let mut args = vec![
        "call",
        "org.kde.KWin",
        "/Scripting",
        "org.kde.kwin.Scripting",
    ];
    args.extend_from_slice(method_and_arguments);
    busctl(&args).await
}

/// Runs busctl on the session bus and returns its output.
async fn busctl(args: &[&str]) -> Option<String> {
    let output = Command::new("busctl")
        .arg("--user")
        .args(args)
        .output()
        .await
        .ok()?;
    Some(String::from_utf8_lossy(&output.stdout).into_owned())
}
