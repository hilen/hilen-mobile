//! The `[bluetooth]` table of `hilen.toml`, for an app on the `bluetooth`
//! feature of hilen:
//!
//! ```toml
//! [bluetooth]
//! usage = "The app drives its turntable over Bluetooth."
//! ```
//!
//! iOS shows `usage` when it asks the user for Bluetooth, and it ends an app
//! that uses Bluetooth with no such text. Android gets the permissions in
//! its manifest, the engine asks the user for them at run time and Android
//! shows its own text.

use std::{
    fs::{read_to_string, write},
    path::Path,
};

use anyhow::{Context, Result, ensure};
use plist::{Dictionary, Value as Plist};
use serde::Deserialize;
use toml::Value;

/// The text of iOS 13 and later.
const ALWAYS_USAGE: &str = "NSBluetoothAlwaysUsageDescription";
/// The text iOS 12 reads.
const PERIPHERAL_USAGE: &str = "NSBluetoothPeripheralUsageDescription";

/// The line of the template the permissions go before.
const APPLICATION: &str = "    <application";

/// Android 12 and later ask for scan and connect. Before that a scan needs
/// the old Bluetooth permissions and the fine location.
const ANDROID_LINES: &str = r#"    <uses-feature android:name="android.hardware.bluetooth_le" android:required="false"/>
    <uses-permission android:name="android.permission.BLUETOOTH_SCAN" android:usesPermissionFlags="neverForLocation"/>
    <uses-permission android:name="android.permission.BLUETOOTH_CONNECT"/>
    <uses-permission android:name="android.permission.BLUETOOTH" android:maxSdkVersion="30"/>
    <uses-permission android:name="android.permission.BLUETOOTH_ADMIN" android:maxSdkVersion="30"/>
    <uses-permission android:name="android.permission.ACCESS_FINE_LOCATION" android:maxSdkVersion="30"/>

"#;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Table {
    usage: String,
}

#[derive(Debug, PartialEq, Eq)]
pub struct BluetoothOptions {
    usage: String,
}

impl BluetoothOptions {
    /// `None` for an app with no `[bluetooth]` table.
    pub fn read(project_info: &Value) -> Result<Option<Self>> {
        let Some(table) = project_info.get("bluetooth") else {
            return Ok(None);
        };
        let table: Table = table.clone().try_into().context("[bluetooth] table in hilen.toml")?;
        ensure!(
            !table.usage.trim().is_empty(),
            "usage of [bluetooth] in hilen.toml is empty, iOS shows it when it asks the user for Bluetooth"
        );
        Ok(Some(Self { usage: table.usage }))
    }

    /// Writes the usage text into the generated iOS app folder,
    /// `mobile/iOS/<name>`.
    pub fn apply_ios(&self, app_dir: &Path) -> Result<()> {
        change(&app_dir.join("Info.plist"), |text| self.info_plist(text))
    }

    /// Writes the permissions into the generated Android project,
    /// `mobile/android`.
    pub fn apply_android(&self, android_dir: &Path) -> Result<()> {
        change(
            &android_dir.join("app/src/main/AndroidManifest.xml"),
            manifest,
        )
    }

    fn info_plist(&self, text: &str) -> Result<String> {
        let mut dict = parse(text)?;
        for key in [ALWAYS_USAGE, PERIPHERAL_USAGE] {
            dict.insert(key.to_string(), Plist::from(self.usage.as_str()));
        }
        print(dict)
    }
}

fn manifest(text: &str) -> Result<String> {
    ensure!(
        text.matches(APPLICATION).count() == 1,
        "the AndroidManifest.xml of the template has another shape, the Bluetooth permissions cannot go in"
    );
    Ok(text.replacen(APPLICATION, &format!("{ANDROID_LINES}{APPLICATION}"), 1))
}

fn change(path: &Path, edit: impl Fn(&str) -> Result<String>) -> Result<()> {
    let text = read_to_string(path).with_context(|| format!("{} not found", path.display()))?;
    let changed = edit(&text).with_context(|| path.display().to_string())?;
    write(path, changed)?;
    println!("Applied the [bluetooth] table of hilen.toml to {}", path.display());
    Ok(())
}

fn parse(text: &str) -> Result<Dictionary> {
    Plist::from_reader_xml(text.as_bytes())?
        .into_dictionary()
        .context("the plist is not a dictionary")
}

fn print(dict: Dictionary) -> Result<String> {
    let mut xml = Vec::new();
    Plist::Dictionary(dict).to_writer_xml(&mut xml)?;
    Ok(String::from_utf8(xml)?)
}

#[cfg(test)]
mod test {
    use anyhow::Result;
    use plist::Value as Plist;
    use toml::Value;

    use super::{ALWAYS_USAGE, BluetoothOptions, PERIPHERAL_USAGE, manifest, parse};

    // The real template files, so a change of the template is tested too.
    const INFO: &str =
        include_str!("../../mobile-template/iOS/HILEN_MOBILE_PROJECT_NAME_CAMEL_CASE/Info.plist");
    const MANIFEST: &str = include_str!("../../mobile-template/android/app/src/main/AndroidManifest.xml");

    const USAGE: &str = "The app drives its turntable over Bluetooth.";

    fn options(toml: &str) -> Result<Option<BluetoothOptions>> {
        BluetoothOptions::read(&toml.parse::<Value>()?)
    }

    fn turntable() -> BluetoothOptions {
        options(&format!("[bluetooth]\nusage = \"{USAGE}\"")).unwrap().unwrap()
    }

    #[test]
    fn no_table_no_bluetooth() -> Result<()> {
        assert_eq!(options("project_name = \"turnscan\"")?, None);
        Ok(())
    }

    #[test]
    fn wrong_tables_are_refused() {
        for toml in [
            "[bluetooth]",
            "[bluetooth]\nusage = \"  \"",
            "[bluetooth]\nusage = 1",
            "[bluetooth]\nusage = \"x\"\nscan = true",
        ] {
            assert!(options(toml).is_err(), "{toml}");
        }
    }

    #[test]
    fn ios_gets_the_usage_text_and_keeps_every_key_of_the_template() -> Result<()> {
        let info = parse(&turntable().info_plist(INFO)?)?;
        for key in [ALWAYS_USAGE, PERIPHERAL_USAGE] {
            assert_eq!(info.get(key).and_then(Plist::as_string), Some(USAGE), "{key}");
        }
        for (key, value) in &parse(INFO)? {
            assert_eq!(info.get(key), Some(value), "{key}");
        }
        Ok(())
    }

    #[test]
    fn android_gets_the_permissions_outside_the_application() -> Result<()> {
        let manifest = manifest(MANIFEST)?;
        for permission in [
            "BLUETOOTH_SCAN",
            "BLUETOOTH_CONNECT",
            "BLUETOOTH\"",
            "BLUETOOTH_ADMIN",
            "ACCESS_FINE_LOCATION",
        ] {
            let line = format!("android:name=\"android.permission.{permission}");
            assert_eq!(manifest.matches(&line).count(), 1, "{permission}");
        }
        assert!(manifest.contains("android:usesPermissionFlags=\"neverForLocation\""));
        assert!(manifest.contains("android.hardware.bluetooth_le"));
        let application = manifest.find("<application").unwrap();
        assert!(manifest.rfind("<uses-permission").unwrap() < application);
        assert!(manifest.find("<manifest").unwrap() < manifest.find("<uses-feature").unwrap());
        Ok(())
    }
}
