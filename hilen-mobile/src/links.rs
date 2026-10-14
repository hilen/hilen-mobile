//! The `[links]` table of `hilen.toml`, the links that open the app:
//!
//! ```toml
//! [links]
//! domains = ["shopping.vladas.xyz"]
//! scheme = "shopping"
//! ```
//!
//! An https link of one of the `domains` opens the app on a phone, a
//! universal link on iOS and a verified app link on Android. The domain
//! proves it with 2 files, `app_links_mount` of `hilen-server` serves them.
//! A link of the `scheme`, like `shopping://invite/x7k2`, opens it too. Not
//! set, the scheme is the `project_name` in lower case with `-` for `_`,
//! the same rule as `link_scheme!` of the engine.

use std::{
    fs::{read_to_string, write},
    path::Path,
};

use anyhow::{Context, Result, ensure};
use plist::{Dictionary, Value as Plist};
use serde::Deserialize;
use toml::Value;

const ASSOCIATED_DOMAINS: &str = "com.apple.developer.associated-domains";
const URL_TYPES: &str = "CFBundleURLTypes";

/// The activity line of the template that gets the launch mode.
const ACTIVITY_END: &str = "android:exported=\"true\">";
/// The line of the template the intent filters go before.
const LIB_NAME: &str = "            <meta-data android:name=\"android.app.lib_name\"";

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Table {
    #[serde(default)]
    domains: Vec<String>,
    scheme:  Option<String>,
}

#[derive(Debug, PartialEq, Eq)]
pub struct LinkOptions {
    domains: Vec<String>,
    scheme:  String,
}

impl LinkOptions {
    /// `None` for an app with no `[links]` table.
    pub fn read(project_info: &Value, project_name: &str) -> Result<Option<Self>> {
        let Some(table) = project_info.get("links") else {
            return Ok(None);
        };
        let table: Table = table.clone().try_into().context("[links] table in hilen.toml")?;
        let scheme = table.scheme.unwrap_or_else(|| project_name.to_lowercase().replace('_', "-"));
        let mut chars = scheme.chars();
        ensure!(
            chars.next().is_some_and(|c| c.is_ascii_lowercase())
                && chars
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, '+' | '-' | '.')),
            "the link scheme {scheme:?} in hilen.toml must start with a small letter and have only small \
             letters, digits, +, - and ."
        );
        ensure!(
            !matches!(scheme.as_str(), "http" | "https" | "file"),
            "the link scheme {scheme:?} in hilen.toml belongs to the system"
        );
        for domain in &table.domains {
            let host = domain.strip_prefix("*.").unwrap_or(domain);
            ensure!(
                host.contains('.')
                    && host
                        .chars()
                        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, '.' | '-')),
                "{domain:?} in domains of [links] in hilen.toml is no domain, write it like \
                 shopping.vladas.xyz, with no https:// and no path"
            );
        }
        Ok(Some(Self {
            domains: table.domains,
            scheme,
        }))
    }

    /// Writes the domains and the scheme into the generated iOS app folder,
    /// `mobile/iOS/<name>`.
    pub fn apply_ios(&self, app_dir: &Path, name: &str) -> Result<()> {
        let info = app_dir.join("Info.plist");
        change(&info, |text| self.info_plist(text))?;
        if !self.domains.is_empty() {
            let entitlements = app_dir.join(format!("{name}.entitlements"));
            change(&entitlements, |text| self.entitlements(text))?;
        }
        Ok(())
    }

    /// Writes the intent filters into the generated Android project,
    /// `mobile/android`.
    pub fn apply_android(&self, android_dir: &Path) -> Result<()> {
        let manifest = android_dir.join("app/src/main/AndroidManifest.xml");
        change(&manifest, |text| self.manifest(text))
    }

    fn info_plist(&self, text: &str) -> Result<String> {
        let mut dict = parse(text)?;
        let mut url_type = Dictionary::new();
        url_type.insert(
            "CFBundleURLName".to_string(),
            Plist::from("$(PRODUCT_BUNDLE_IDENTIFIER)"),
        );
        url_type.insert(
            "CFBundleURLSchemes".to_string(),
            Plist::Array(vec![Plist::from(self.scheme.as_str())]),
        );
        dict.insert(
            URL_TYPES.to_string(),
            Plist::Array(vec![Plist::Dictionary(url_type)]),
        );
        print(dict)
    }

    fn entitlements(&self, text: &str) -> Result<String> {
        let mut dict = parse(text)?;
        let domains = self
            .domains
            .iter()
            .map(|domain| Plist::from(format!("applinks:{domain}")))
            .collect();
        dict.insert(ASSOCIATED_DOMAINS.to_string(), Plist::Array(domains));
        print(dict)
    }

    /// The activity runs as `singleTask`, so a link while the app runs goes
    /// to the activity that runs and not to a second one.
    fn manifest(&self, text: &str) -> Result<String> {
        ensure!(
            text.matches(ACTIVITY_END).count() == 1 && text.matches(LIB_NAME).count() == 1,
            "the AndroidManifest.xml of the template has another shape, the links cannot go in"
        );
        let text = text.replacen(
            ACTIVITY_END,
            "android:exported=\"true\"\n                android:launchMode=\"singleTask\">",
            1,
        );

        let filter = |data: &str, verify: bool| {
            let open = if verify {
                "<intent-filter android:autoVerify=\"true\">"
            } else {
                "<intent-filter>"
            };
            format!(
                "            {open}\n                <action \
                 android:name=\"android.intent.action.VIEW\"/>\n                <category \
                 android:name=\"android.intent.category.DEFAULT\"/>\n                <category \
                 android:name=\"android.intent.category.BROWSABLE\"/>\n{data}            </intent-filter>\n"
            )
        };
        let mut filters = String::new();
        if !self.domains.is_empty() {
            let mut data = "                <data android:scheme=\"https\"/>\n".to_string();
            for domain in &self.domains {
                data.push_str(&format!("                <data android:host=\"{domain}\"/>\n"));
            }
            filters.push_str(&filter(&data, true));
        }
        filters.push_str(&filter(
            &format!("                <data android:scheme=\"{}\"/>\n", self.scheme),
            false,
        ));
        Ok(text.replacen(LIB_NAME, &format!("{filters}{LIB_NAME}"), 1))
    }
}

fn change(path: &Path, edit: impl Fn(&str) -> Result<String>) -> Result<()> {
    let text = read_to_string(path).with_context(|| format!("{} not found", path.display()))?;
    let changed = edit(&text).with_context(|| path.display().to_string())?;
    write(path, changed)?;
    println!("Applied the [links] table of hilen.toml to {}", path.display());
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

    use super::{ASSOCIATED_DOMAINS, LinkOptions, URL_TYPES, parse};

    // The real template files, so a change of the template is tested too.
    const INFO: &str =
        include_str!("../../mobile-template/iOS/HILEN_MOBILE_PROJECT_NAME_CAMEL_CASE/Info.plist");
    const ENTITLEMENTS: &str = include_str!(
        "../../mobile-template/iOS/HILEN_MOBILE_PROJECT_NAME_CAMEL_CASE/\
         HILEN_MOBILE_PROJECT_NAME_CAMEL_CASE.entitlements"
    );
    const MANIFEST: &str = include_str!("../../mobile-template/android/app/src/main/AndroidManifest.xml");
    const ACTIVITY: &str = include_str!(
        "../../mobile-template/android/app/src/main/java/com/example/wgpuapplication/MainActivity.kt"
    );

    fn options(toml: &str) -> Result<Option<LinkOptions>> {
        LinkOptions::read(&toml.parse::<Value>()?, "shopping")
    }

    fn shopping() -> LinkOptions {
        options("[links]\ndomains = [\"shopping.vladas.xyz\"]").unwrap().unwrap()
    }

    #[test]
    fn no_table_no_links() -> Result<()> {
        assert_eq!(options("project_name = \"shopping\"")?, None);
        Ok(())
    }

    #[test]
    fn the_scheme_is_the_project_name_unless_the_table_names_one() -> Result<()> {
        assert_eq!(shopping().scheme, "shopping");
        assert_eq!(
            options("[links]\nscheme = \"shop-app\"")?.unwrap().scheme,
            "shop-app"
        );
        assert_eq!(
            LinkOptions::read(&"[links]".parse::<Value>()?, "My_Shop")?.unwrap().scheme,
            "my-shop"
        );
        Ok(())
    }

    #[test]
    fn wrong_tables_are_refused() {
        for toml in [
            "[links]\ndomains = [\"https://shopping.vladas.xyz\"]",
            "[links]\ndomains = [\"shopping.vladas.xyz/invite\"]",
            "[links]\ndomains = [\"localhost\"]",
            "[links]\ndomains = \"shopping.vladas.xyz\"",
            "[links]\nscheme = \"https\"",
            "[links]\nscheme = \"Shop\"",
            "[links]\ndomain = [\"shopping.vladas.xyz\"]",
        ] {
            assert!(options(toml).is_err(), "{toml}");
        }
    }

    #[test]
    fn ios_gets_the_scheme_and_the_associated_domains() -> Result<()> {
        let options = shopping();

        let info = parse(&options.info_plist(INFO)?)?;
        let before = parse(INFO)?;
        for (key, value) in &before {
            assert_eq!(info.get(key), Some(value), "{key}");
        }
        let types = info.get(URL_TYPES).and_then(Plist::as_array).expect("no url types");
        let url_type = types[0].as_dictionary().unwrap();
        assert_eq!(
            url_type.get("CFBundleURLSchemes").and_then(Plist::as_array).unwrap()[0].as_string(),
            Some("shopping")
        );

        let entitlements = parse(&options.entitlements(ENTITLEMENTS)?)?;
        let domains: Vec<&str> = entitlements
            .get(ASSOCIATED_DOMAINS)
            .and_then(Plist::as_array)
            .unwrap()
            .iter()
            .map(|value| value.as_string().unwrap())
            .collect();
        assert_eq!(domains, ["applinks:shopping.vladas.xyz"]);
        for key in parse(ENTITLEMENTS)?.keys() {
            assert!(entitlements.contains_key(key), "{key}");
        }
        Ok(())
    }

    #[test]
    fn android_gets_a_verified_filter_and_a_scheme_filter() -> Result<()> {
        let manifest = shopping().manifest(MANIFEST)?;

        assert!(manifest.contains("android:launchMode=\"singleTask\""));
        assert!(manifest.contains("<intent-filter android:autoVerify=\"true\">"));
        assert!(manifest.contains("<data android:scheme=\"https\"/>"));
        assert!(manifest.contains("<data android:host=\"shopping.vladas.xyz\"/>"));
        assert!(manifest.contains("<data android:scheme=\"shopping\"/>"));
        assert_eq!(manifest.matches("android.intent.action.VIEW").count(), 2);
        // The launcher filter stays, and the filters sit inside the activity.
        assert!(manifest.contains("android.intent.category.LAUNCHER"));
        let activity_end = manifest.find("</activity>").unwrap();
        assert!(manifest.rfind("</intent-filter>").unwrap() < activity_end);
        Ok(())
    }

    #[test]
    fn a_scheme_alone_gets_no_verified_filter() -> Result<()> {
        let manifest = options("[links]")?.unwrap().manifest(MANIFEST)?;
        assert!(!manifest.contains("autoVerify"));
        assert!(manifest.contains("<data android:scheme=\"shopping\"/>"));
        Ok(())
    }

    /// The engine reads the intent of the activity, so a link while the
    /// app runs must become that intent.
    #[test]
    fn the_activity_keeps_the_intent_of_a_link_while_it_runs() {
        assert!(ACTIVITY.contains("override fun onNewIntent(intent: Intent)"));
        assert!(ACTIVITY.contains("setIntent(intent)"));
    }
}
