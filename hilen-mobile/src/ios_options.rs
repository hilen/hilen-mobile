//! The `[ios]` table of `hilen.toml`, the settings of the generated iOS
//! project:
//!
//! ```toml
//! [ios]
//! file_sharing = false
//! iphone_orientations = ["portrait"]
//! ```
//!
//! The template is cloned from the repo at run time and an installed tool can
//! be older than it. So the template stays the project of an app with no
//! settings, and the settings are applied to the copy made from it.

use std::{
    fs::{read_to_string, write},
    path::Path,
};

use anyhow::{Context, Result, ensure};
use plist::{Dictionary, Value as Plist};
use serde::Deserialize;
use toml::Value;

const ORIENTATIONS: &str = "UISupportedInterfaceOrientations";
const IPAD_ORIENTATIONS: &str = "UISupportedInterfaceOrientations~ipad";

/// What shows the documents folder of the app in the Files app and gives it
/// an iCloud documents container.
const FILE_SHARING_INFO: [&str; 3] = [
    "NSUbiquitousContainers",
    "UIFileSharingEnabled",
    "LSSupportsOpeningDocumentsInPlace",
];

const FILE_SHARING_ENTITLEMENTS: [&str; 3] = [
    "com.apple.developer.icloud-container-identifiers",
    "com.apple.developer.icloud-services",
    "com.apple.developer.ubiquity-container-identifiers",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
enum Orientation {
    Portrait,
    PortraitUpsideDown,
    LandscapeLeft,
    LandscapeRight,
}

impl Orientation {
    fn key(self) -> &'static str {
        match self {
            Self::Portrait => "UIInterfaceOrientationPortrait",
            Self::PortraitUpsideDown => "UIInterfaceOrientationPortraitUpsideDown",
            Self::LandscapeLeft => "UIInterfaceOrientationLandscapeLeft",
            Self::LandscapeRight => "UIInterfaceOrientationLandscapeRight",
        }
    }
}

#[derive(Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IosOptions {
    /// `false` takes the iCloud container and the file sharing keys out. Not
    /// set is the same as `true`, the project of the template.
    file_sharing:        Option<bool>,
    /// The orientations of an iPhone. The iPad keeps the ones of the
    /// template. Not set leaves the template, the same on both.
    iphone_orientations: Option<Vec<Orientation>>,
}

impl IosOptions {
    pub fn read(project_info: &Value) -> Result<Self> {
        let Some(table) = project_info.get("ios") else {
            return Ok(Self::default());
        };
        let options: Self = table.clone().try_into().context("[ios] table in hilen.toml")?;
        if let Some(orientations) = &options.iphone_orientations {
            ensure!(
                !orientations.is_empty(),
                "iphone_orientations in hilen.toml names no orientation"
            );
            for (index, orientation) in orientations.iter().enumerate() {
                ensure!(
                    !orientations[..index].contains(orientation),
                    "iphone_orientations in hilen.toml names {orientation:?} twice"
                );
            }
        }
        Ok(options)
    }

    /// Applies the settings to the generated app folder,
    /// `mobile/iOS/<name>`. A file that no setting changes is not written, so
    /// an app with no settings keeps every byte of the template.
    pub fn apply(&self, app_dir: &Path, name: &str) -> Result<()> {
        let info = app_dir.join("Info.plist");
        let entitlements = app_dir.join(format!("{name}.entitlements"));
        for (path, change) in [
            (
                info,
                Self::info_plist as fn(&Self, &str) -> Result<Option<String>>,
            ),
            (entitlements, Self::entitlements),
        ] {
            let text = read_to_string(&path).with_context(|| format!("{} not found", path.display()))?;
            if let Some(changed) = change(self, &text).with_context(|| path.display().to_string())? {
                write(&path, changed)?;
                println!("Applied the [ios] settings of hilen.toml to {}", path.display());
            }
        }
        Ok(())
    }

    fn no_file_sharing(&self) -> bool {
        self.file_sharing == Some(false)
    }

    /// The changed `Info.plist`, `None` when no setting touches it.
    fn info_plist(&self, text: &str) -> Result<Option<String>> {
        if !self.no_file_sharing() && self.iphone_orientations.is_none() {
            return Ok(None);
        }
        let mut dict = parse(text)?;
        if self.no_file_sharing() {
            dict.retain(|key, _| !FILE_SHARING_INFO.contains(&key.as_str()));
        }
        if let Some(orientations) = &self.iphone_orientations {
            let template = dict
                .get(ORIENTATIONS)
                .with_context(|| format!("the template has no {ORIENTATIONS}"))?
                .clone();
            let iphone = orientations.iter().map(|orientation| Plist::from(orientation.key())).collect();
            dict.insert(ORIENTATIONS.to_string(), Plist::Array(iphone));
            dict.insert(IPAD_ORIENTATIONS.to_string(), template);
        }
        print(dict).map(Some)
    }

    /// The changed entitlements file, `None` when no setting touches it.
    fn entitlements(&self, text: &str) -> Result<Option<String>> {
        if !self.no_file_sharing() {
            return Ok(None);
        }
        let mut dict = parse(text)?;
        dict.retain(|key, _| !FILE_SHARING_ENTITLEMENTS.contains(&key.as_str()));
        print(dict).map(Some)
    }
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
    use plist::{Dictionary, Value as Plist};
    use toml::Value;

    use super::{
        FILE_SHARING_ENTITLEMENTS, FILE_SHARING_INFO, IPAD_ORIENTATIONS, IosOptions, ORIENTATIONS, parse,
    };

    // The real template files, so a change of the template is tested too.
    const INFO: &str =
        include_str!("../../mobile-template/iOS/HILEN_MOBILE_PROJECT_NAME_CAMEL_CASE/Info.plist");
    const ENTITLEMENTS: &str = include_str!(
        "../../mobile-template/iOS/HILEN_MOBILE_PROJECT_NAME_CAMEL_CASE/\
         HILEN_MOBILE_PROJECT_NAME_CAMEL_CASE.entitlements"
    );

    const ALL: [&str; 4] = [
        "UIInterfaceOrientationLandscapeLeft",
        "UIInterfaceOrientationLandscapeRight",
        "UIInterfaceOrientationPortrait",
        "UIInterfaceOrientationPortraitUpsideDown",
    ];

    fn options(toml: &str) -> Result<IosOptions> {
        IosOptions::read(&toml.parse::<Value>()?)
    }

    fn strings(dict: &Dictionary, key: &str) -> Vec<String> {
        let array = dict
            .get(key)
            .and_then(Plist::as_array)
            .unwrap_or_else(|| panic!("no array {key}"));
        array.iter().map(|value| value.as_string().unwrap().to_string()).collect()
    }

    fn keys(dict: &Dictionary) -> Vec<&str> {
        dict.keys().map(String::as_str).collect()
    }

    #[test]
    fn no_settings_change_no_file() -> Result<()> {
        for toml in ["project_name = \"app\"", "[ios]", "[ios]\nfile_sharing = true"] {
            let options = options(toml)?;
            assert_eq!(options.info_plist(INFO)?, None, "{toml}");
            assert_eq!(options.entitlements(ENTITLEMENTS)?, None, "{toml}");
        }
        Ok(())
    }

    #[test]
    fn the_template_has_what_the_settings_take_out() -> Result<()> {
        let info = parse(INFO)?;
        for key in FILE_SHARING_INFO {
            assert!(info.contains_key(key), "{key}");
        }
        let entitlements = parse(ENTITLEMENTS)?;
        for key in FILE_SHARING_ENTITLEMENTS {
            assert!(entitlements.contains_key(key), "{key}");
        }
        assert_eq!(strings(&info, ORIENTATIONS), ALL);
        assert!(!info.contains_key(IPAD_ORIENTATIONS));
        Ok(())
    }

    #[test]
    fn file_sharing_off_takes_the_keys_out() -> Result<()> {
        let options = options("[ios]\nfile_sharing = false")?;

        let before = parse(INFO)?;
        let info = parse(&options.info_plist(INFO)?.expect("Info.plist not changed"))?;
        for key in FILE_SHARING_INFO {
            assert!(!info.contains_key(key), "{key}");
        }
        let kept: Vec<&str> = keys(&before)
            .into_iter()
            .filter(|key| !FILE_SHARING_INFO.contains(key))
            .collect();
        assert_eq!(keys(&info), kept);
        for key in kept {
            assert_eq!(info.get(key), before.get(key), "{key}");
        }

        let entitlements = parse(&options.entitlements(ENTITLEMENTS)?.expect("entitlements not changed"))?;
        assert_eq!(
            keys(&entitlements),
            [
                "com.apple.security.app-sandbox",
                "com.apple.security.network.client"
            ]
        );
        Ok(())
    }

    #[test]
    fn file_sharing_off_leaves_no_trace_in_the_text() -> Result<()> {
        let options = options("[ios]\nfile_sharing = false")?;
        let text = options.info_plist(INFO)?.unwrap() + &options.entitlements(ENTITLEMENTS)?.unwrap();
        for word in [
            "iCloud",
            "icloud",
            "Ubiquit",
            "ubiquity",
            "UIFileSharingEnabled",
            "LSSupportsOpeningDocumentsInPlace",
        ] {
            assert!(!text.contains(word), "{word}");
        }
        Ok(())
    }

    #[test]
    fn iphone_orientations_leave_the_ipad_all() -> Result<()> {
        let options = options("[ios]\niphone_orientations = [\"portrait\"]")?;

        let info = parse(&options.info_plist(INFO)?.expect("Info.plist not changed"))?;
        assert_eq!(strings(&info, ORIENTATIONS), ["UIInterfaceOrientationPortrait"]);
        assert_eq!(strings(&info, IPAD_ORIENTATIONS), ALL);
        // The file sharing keys stay, that switch is not set.
        for key in FILE_SHARING_INFO {
            assert!(info.contains_key(key), "{key}");
        }
        assert_eq!(options.entitlements(ENTITLEMENTS)?, None);
        Ok(())
    }

    #[test]
    fn iphone_orientations_keep_their_order() -> Result<()> {
        let options = options(
            "[ios]\niphone_orientations = [\"landscape_right\", \"portrait_upside_down\", \
             \"landscape_left\"]",
        )?;
        let info = parse(&options.info_plist(INFO)?.unwrap())?;
        assert_eq!(
            strings(&info, ORIENTATIONS),
            [
                "UIInterfaceOrientationLandscapeRight",
                "UIInterfaceOrientationPortraitUpsideDown",
                "UIInterfaceOrientationLandscapeLeft",
            ]
        );
        Ok(())
    }

    #[test]
    fn both_settings_together() -> Result<()> {
        let options = options("[ios]\nfile_sharing = false\niphone_orientations = [\"portrait\"]")?;
        let info = parse(&options.info_plist(INFO)?.unwrap())?;
        for key in FILE_SHARING_INFO {
            assert!(!info.contains_key(key), "{key}");
        }
        assert_eq!(strings(&info, ORIENTATIONS), ["UIInterfaceOrientationPortrait"]);
        assert_eq!(strings(&info, IPAD_ORIENTATIONS), ALL);
        Ok(())
    }

    // A template of before these keys, or of after their removal, is not an
    // error.
    #[test]
    fn file_sharing_off_on_a_template_without_the_keys() -> Result<()> {
        let options = options("[ios]\nfile_sharing = false")?;
        let once = options.info_plist(INFO)?.unwrap();
        assert_eq!(options.info_plist(&once)?, Some(once.clone()));
        Ok(())
    }

    #[test]
    fn wrong_settings_are_refused() {
        for toml in [
            "[ios]\nfile_sharng = false",
            "[ios]\nfile_sharing = \"no\"",
            "[ios]\niphone_orientations = []",
            "[ios]\niphone_orientations = [\"sideways\"]",
            "[ios]\niphone_orientations = \"portrait\"",
            "[ios]\niphone_orientations = [\"portrait\", \"portrait\"]",
        ] {
            assert!(options(toml).is_err(), "{toml}");
        }
    }
}
