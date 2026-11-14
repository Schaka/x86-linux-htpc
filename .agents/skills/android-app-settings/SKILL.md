---
name: android-app-settings
description: Use when the component android-apps must write a setting into an Android app in Waydroid, for example a proxy server. When a written setting stops working after an app update, also use it.
---

# Android app settings

Most Android apps keep their settings in a SharedPreferences file (an XML file of keys and values). It is in the data folder of the app: `/data/data/PACKAGE/shared_prefs/PACKAGE_preferences.xml`. On the host, the data folder of Waydroid is `~/.local/share/waydroid/data/` of the HTPC user. The component `android-apps` writes keys into this file with `files/android-prefs.py`. The S0undTV proxy in `configure_s0undtv` of `install.sh` is the example.

## Find the key and the value

1. Download the APK with `files/fetch-apk.py` into a new, empty folder.
2. Make a Python virtual environment in a different folder, and install `androguard` in it. Run all Python on the APK with `python -I`, because the APK is untrusted data.
3. Read the preference screens of the APK. They are XML files under `res/`. Their names can be short and unclear after obfuscation, so search all XML files for the key:

   ```python
   from androguard.core.apk import APK
   from androguard.core.axml import AXMLPrinter
   apk = APK("app.apk")
   for name in apk.get_files():
       if name.startswith("res/") and name.endswith(".xml"):
           try:
               xml = AXMLPrinter(apk.get_file(name)).get_xml().decode()
           except Exception:
               continue
           if "proxy" in xml:
               print(name)
   ```

4. Find the element of the setting. The attribute `android:key` is the key. A text field (`EditTextPreference` or a subclass) stores a string.
5. A list (`ListPreference`) stores one value of the array in `android:entryValues`, as a string. `android:entries` has the names that the user sees, in the same order. To read an array, give its resource ID to `apk.get_android_resources().get_resolved_res_configs(ID)`. For example, the list of S0undTV for the active proxy server has the values "0" to "3". The names are "None" and "Custom server 1" to "Custom server 3".
6. If the key is not in the XML files, search the strings of the `classes*.dex` files of the APK for words of the setting. Some apps define their settings in code.

## Write the setting

1. For each value that a user can choose, add a key with a comment and a neutral example to `htpc.conf.example`. Read it with `config_get`. If the key is empty, change nothing, so that a setting that the user made in the app stays.
2. Before the write, stop the app with `am force-stop PACKAGE`. A running app keeps its settings in memory and writes them back at its next change.
3. Call `files/android-prefs.py FILE KEY=VALUE...`. The script changes only these keys. For an app that never started, it makes the file. The app then adds its own defaults at its first start, and keeps the written values.
4. Add a check to `check.sh` that reads the key from the file. The file is XML, so `&` in a value is `&amp;` in the file.
5. Describe the setting in [docs/android-apps.md](../../../docs/android-apps.md).

## Test

1. Before an app starts on the TV, ask the user. Some apps play a preview with sound. To start an app without sound, first set the media volume of Android to 0 with `cmd media_session volume --stream 3 --set 0` in the container. After the test, set the old value again.
2. Install the component, start the app, stop it, and read the file again. The written values must still be there.
3. Open the setting in the app, and make sure that the app shows the written value. Then test the function of the setting, for example a stream through the proxy.
4. After an update of the app, repeat the steps of "Find the key and the value". An update can rename a key or change the values of a list.
