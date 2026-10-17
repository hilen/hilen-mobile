//! The `[widget]` table of `hilen.toml`, a WidgetKit extension next to the
//! app on iOS, for home screen widgets and Live Activities:
//!
//! ```toml
//! [widget]
//! extension = "mobile-src/widget"
//! app = "mobile-src/app"
//! shared = "mobile-src/shared"
//! minimum = "17.2"
//! live_activities = true
//! push = true
//! ```
//!
//! Each folder holds Swift files of the app. `extension` is compiled into the
//! extension, `app` into the app target and `shared` into both, like the
//! `ActivityAttributes` of a Live Activity. The files stay where they are, the
//! project points at them. The app target runs no Swift by itself, so a class
//! `HilenAppStart` with a static `start` is called from `main.m` before the
//! engine starts, when the app has one.
//!
//! Both targets share a keychain group, its full name is `HilenKeychainGroup`
//! in both `Info.plist` files. `minimum` is the iOS the extension needs, the
//! app keeps its own. `push` adds `aps-environment`, for Live Activities and
//! widgets that a backend updates by push.

mod project;

use std::{
    fs::{create_dir_all, read_dir, read_to_string, write},
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, ensure};
use plist::{Dictionary, Value as Plist};
use serde::Deserialize;
use toml::Value;

/// The key the engine and the Swift of an app read the keychain group from.
pub const KEYCHAIN_GROUP_KEY: &str = "HilenKeychainGroup";
const KEYCHAIN_GROUPS: &str = "keychain-access-groups";
const APS_ENVIRONMENT: &str = "aps-environment";
const LIVE_ACTIVITIES: &str = "NSSupportsLiveActivities";
const FREQUENT_UPDATES: &str = "NSSupportsLiveActivitiesFrequentUpdates";

const MAIN_START: &str = "    return hilen_start_app();";
const SWIFT_START: &str = r#"    Class start = NSClassFromString(@"HilenAppStart");
    if (start) {
        [start performSelector:@selector(start)];
    }
"#;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Table {
    extension:       String,
    app:             Option<String>,
    shared:          Option<String>,
    minimum:         Option<String>,
    #[serde(default)]
    live_activities: bool,
    #[serde(default)]
    push:            bool,
}

#[derive(Debug, PartialEq, Eq)]
pub struct WidgetOptions {
    extension:       String,
    app:             Option<String>,
    shared:          Option<String>,
    minimum:         String,
    live_activities: bool,
    push:            bool,
}

/// The Swift files of each folder, as paths from the root of the app.
#[derive(Debug, Default)]
pub struct SwiftFiles {
    extension: Vec<String>,
    app:       Vec<String>,
    shared:    Vec<String>,
}

impl SwiftFiles {
    fn in_app(&self) -> bool {
        !self.app.is_empty() || !self.shared.is_empty()
    }
}

/// What the generated files are named after.
pub struct WidgetNames<'a> {
    /// The app target, `Banda`.
    pub camel:   &'a str,
    pub title:   &'a str,
    pub bundle:  &'a str,
    pub version: &'a str,
}

impl WidgetNames<'_> {
    fn target(&self) -> String {
        format!("{}Widget", self.camel)
    }

    fn keychain_group(&self) -> String {
        format!("$(AppIdentifierPrefix){}.shared", self.bundle)
    }
}

impl WidgetOptions {
    /// `None` for an app with no `[widget]` table.
    pub fn read(project_info: &Value) -> Result<Option<Self>> {
        let Some(table) = project_info.get("widget") else {
            return Ok(None);
        };
        let table: Table = table.clone().try_into().context("[widget] table in hilen.toml")?;
        let minimum = table.minimum.unwrap_or_else(|| "17.0".to_string());
        ensure!(
            minimum
                .split('.')
                .all(|part| !part.is_empty() && part.chars().all(|c| c.is_ascii_digit())),
            "minimum of [widget] in hilen.toml is no iOS version, write it like 17.2"
        );
        for folder in [Some(&table.extension), table.app.as_ref(), table.shared.as_ref()]
            .into_iter()
            .flatten()
        {
            ensure!(
                !folder.is_empty() && !Path::new(folder).is_absolute() && !folder.contains(".."),
                "{folder:?} in [widget] of hilen.toml must be a folder inside the app, like \
                 mobile-src/widget"
            );
        }
        Ok(Some(Self {
            extension: table.extension,
            app: table.app,
            shared: table.shared,
            minimum,
            live_activities: table.live_activities,
            push: table.push,
        }))
    }

    /// Reads the Swift files of the folders, from the root of the app.
    pub fn files(&self) -> Result<SwiftFiles> {
        let files = SwiftFiles {
            extension: swift_files(&self.extension)?,
            app:       self.app.as_deref().map(swift_files).transpose()?.unwrap_or_default(),
            shared:    self.shared.as_deref().map(swift_files).transpose()?.unwrap_or_default(),
        };
        ensure!(
            !files.extension.is_empty(),
            "{} of [widget] in hilen.toml has no Swift file, the extension would be empty",
            self.extension
        );
        Ok(files)
    }

    /// Adds the extension to the generated iOS project, `mobile/iOS`.
    pub fn apply_ios(&self, ios_dir: &Path, names: &WidgetNames, files: &SwiftFiles) -> Result<()> {
        let app_dir = ios_dir.join(names.camel);
        let widget_dir = ios_dir.join(names.target());
        create_dir_all(&widget_dir)?;
        write(widget_dir.join("Info.plist"), self.widget_info(names)?)?;
        write(
            widget_dir.join(format!("{}.entitlements", names.target())),
            self.widget_entitlements(names)?,
        )?;

        change(&app_dir.join("Info.plist"), |text| self.app_info(text, names))?;
        change(&app_dir.join(format!("{}.entitlements", names.camel)), |text| {
            self.app_entitlements(text, names)
        })?;
        if files.in_app() {
            change(&app_dir.join("main.m"), main_m)?;
        }
        change(
            &ios_dir.join(format!("{}.xcodeproj/project.pbxproj", names.camel)),
            |text| project::add(text, self, names, files),
        )
    }

    fn widget_info(&self, names: &WidgetNames) -> Result<String> {
        let mut dict = Dictionary::new();
        for (key, value) in [
            ("CFBundleDevelopmentRegion", "$(DEVELOPMENT_LANGUAGE)"),
            ("CFBundleDisplayName", names.title),
            ("CFBundleExecutable", "$(EXECUTABLE_NAME)"),
            ("CFBundleIdentifier", "$(PRODUCT_BUNDLE_IDENTIFIER)"),
            ("CFBundleInfoDictionaryVersion", "6.0"),
            ("CFBundleName", "$(PRODUCT_NAME)"),
            ("CFBundlePackageType", "$(PRODUCT_BUNDLE_PACKAGE_TYPE)"),
            // App Store Connect refuses an extension whose version is not the
            // one of its app. The template app has build 1.
            ("CFBundleShortVersionString", names.version),
            ("CFBundleVersion", "1"),
        ] {
            dict.insert(key.to_string(), Plist::from(value));
        }
        dict.insert(
            KEYCHAIN_GROUP_KEY.to_string(),
            Plist::from(names.keychain_group()),
        );
        let mut extension = Dictionary::new();
        extension.insert(
            "NSExtensionPointIdentifier".to_string(),
            Plist::from("com.apple.widgetkit-extension"),
        );
        dict.insert("NSExtension".to_string(), Plist::Dictionary(extension));
        print(dict)
    }

    fn widget_entitlements(&self, names: &WidgetNames) -> Result<String> {
        let mut dict = Dictionary::new();
        dict.insert(
            KEYCHAIN_GROUPS.to_string(),
            Plist::Array(vec![Plist::from(names.keychain_group())]),
        );
        self.add_push(&mut dict);
        print(dict)
    }

    fn app_info(&self, text: &str, names: &WidgetNames) -> Result<String> {
        let mut dict = parse(text)?;
        dict.insert(
            KEYCHAIN_GROUP_KEY.to_string(),
            Plist::from(names.keychain_group()),
        );
        if self.live_activities {
            dict.insert(LIVE_ACTIVITIES.to_string(), Plist::Boolean(true));
            dict.insert(FREQUENT_UPDATES.to_string(), Plist::Boolean(true));
        }
        print(dict)
    }

    /// The group of the app itself comes first. A new keychain item goes into
    /// the first group of the list, so the secrets of the app stay its own and
    /// only what is put into the shared group on purpose is shared.
    fn app_entitlements(&self, text: &str, names: &WidgetNames) -> Result<String> {
        let mut dict = parse(text)?;
        dict.insert(
            KEYCHAIN_GROUPS.to_string(),
            Plist::Array(vec![
                Plist::from(format!("$(AppIdentifierPrefix){}", names.bundle)),
                Plist::from(names.keychain_group()),
            ]),
        );
        self.add_push(&mut dict);
        print(dict)
    }

    /// An archive for the App Store turns `development` into `production`.
    fn add_push(&self, dict: &mut Dictionary) {
        if self.push {
            dict.insert(APS_ENVIRONMENT.to_string(), Plist::from("development"));
        }
    }
}

fn main_m(text: &str) -> Result<String> {
    ensure!(
        text.matches(MAIN_START).count() == 1,
        "main.m of the template has another shape, the start of the Swift of the app cannot go in"
    );
    Ok(text.replacen(MAIN_START, &format!("{SWIFT_START}{MAIN_START}"), 1))
}

/// The `.swift` files under `folder`, sorted, with `/` between the parts.
fn swift_files(folder: &str) -> Result<Vec<String>> {
    let mut files = Vec::new();
    collect(Path::new(folder), &mut files)
        .with_context(|| format!("{folder} of [widget] in hilen.toml cannot be read"))?;
    let mut files: Vec<String> = files
        .into_iter()
        .map(|path| {
            path.components()
                .map(|part| part.as_os_str().to_string_lossy().into_owned())
                .collect::<Vec<_>>()
                .join("/")
        })
        .collect();
    files.sort();
    Ok(files)
}

fn collect(dir: &Path, files: &mut Vec<PathBuf>) -> Result<()> {
    for entry in read_dir(dir)? {
        let path = entry?.path();
        if path.is_dir() {
            collect(&path, files)?;
        } else if path.extension().is_some_and(|extension| extension == "swift") {
            files.push(path);
        }
    }
    Ok(())
}

fn change(path: &Path, edit: impl Fn(&str) -> Result<String>) -> Result<()> {
    let text = read_to_string(path).with_context(|| format!("{} not found", path.display()))?;
    let changed = edit(&text).with_context(|| path.display().to_string())?;
    write(path, changed)?;
    println!("Applied the [widget] table of hilen.toml to {}", path.display());
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
mod test;
