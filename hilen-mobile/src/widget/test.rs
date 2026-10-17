use std::{
    collections::BTreeSet,
    env::temp_dir,
    fs::{create_dir_all, remove_dir_all, write},
    process::id,
};

use anyhow::Result;
use plist::Value as Plist;
use toml::Value;

use super::{
    APS_ENVIRONMENT, FREQUENT_UPDATES, KEYCHAIN_GROUP_KEY, KEYCHAIN_GROUPS, LIVE_ACTIVITIES, SwiftFiles,
    WidgetNames, WidgetOptions, main_m, parse, project, swift_files,
};

// The real template files, so a change of the template is tested too.
const PROJECT: &str = include_str!(
    "../../../mobile-template/iOS/HILEN_MOBILE_PROJECT_NAME_CAMEL_CASE.xcodeproj/project.pbxproj"
);
const INFO: &str =
    include_str!("../../../mobile-template/iOS/HILEN_MOBILE_PROJECT_NAME_CAMEL_CASE/Info.plist");
const ENTITLEMENTS: &str = include_str!(
    "../../../mobile-template/iOS/HILEN_MOBILE_PROJECT_NAME_CAMEL_CASE/HILEN_MOBILE_PROJECT_NAME_CAMEL_CASE.\
     entitlements"
);
const MAIN: &str = include_str!("../../../mobile-template/iOS/HILEN_MOBILE_PROJECT_NAME_CAMEL_CASE/main.m");

const NAMES: WidgetNames = WidgetNames {
    camel:   "HILEN_MOBILE_PROJECT_NAME_CAMEL_CASE",
    title:   "Banda",
    bundle:  "HILEN_MOBILE_BINDLE_IDENTIFIER",
    version: "0.1.38",
};

fn options(toml: &str) -> Result<Option<WidgetOptions>> {
    WidgetOptions::read(&toml.parse::<Value>()?)
}

fn banda() -> WidgetOptions {
    options(
        "[widget]\nextension = \"mobile-src/widget\"\nminimum = \"17.2\"\nlive_activities = true\npush = \
         true",
    )
    .unwrap()
    .unwrap()
}

fn files(app: bool) -> SwiftFiles {
    SwiftFiles {
        extension: vec![
            "mobile-src/widget/Widget.swift".into(),
            "mobile-src/widget/Island.swift".into(),
        ],
        app:       if app {
            vec!["mobile-src/app/Start.swift".into()]
        } else {
            vec![]
        },
        shared:    vec!["mobile-src/shared/Attributes.swift".into()],
    }
}

/// Every id the change added, `F1D6E7` and 18 hex digits.
fn added_ids(text: &str) -> BTreeSet<&str> {
    text.match_indices("F1D6E7").map(|(at, _)| &text[at..at + 24]).collect()
}

#[test]
fn no_table_no_widget() -> Result<()> {
    assert_eq!(options("project_name = \"banda\"")?, None);
    Ok(())
}

#[test]
fn the_minimum_is_17_unless_the_table_names_one() -> Result<()> {
    let options = options("[widget]\nextension = \"w\"")?.unwrap();
    assert_eq!(options.minimum, "17.0");
    assert!(!options.live_activities);
    assert!(!options.push);
    assert_eq!(banda().minimum, "17.2");
    Ok(())
}

#[test]
fn wrong_tables_are_refused() {
    for toml in [
        "[widget]",
        "[widget]\nextension = \"\"",
        "[widget]\nextension = \"/abs/widget\"",
        "[widget]\nextension = \"../other/widget\"",
        "[widget]\nextension = \"w\"\nminimum = \"seventeen\"",
        "[widget]\nextension = \"w\"\nminimum = \"17.\"",
        "[widget]\nextension = \"w\"\npush = \"yes\"",
        "[widget]\nextension = \"w\"\nlive_activity = true",
    ] {
        assert!(options(toml).is_err(), "{toml}");
    }
}

#[test]
fn every_added_object_is_defined_once_and_used() -> Result<()> {
    let text = project::add(PROJECT, &banda(), &NAMES, &files(true))?;
    let ids = added_ids(&text);
    // 4 file references, 3 sources of the widget, 2 of the app, and 14
    // objects of the target.
    assert_eq!(ids.len(), 23);
    for id in ids {
        assert_eq!(
            text.matches(&format!("\n\t\t{id} /* ")).count(),
            1,
            "{id} is not defined once"
        );
        assert!(text.matches(id).count() >= 2, "{id} is defined and used nowhere");
    }
    assert_eq!(text.matches('{').count(), text.matches('}').count());
    assert_eq!(text.matches('(').count(), text.matches(')').count());
    Ok(())
}

#[test]
fn the_app_builds_and_embeds_the_extension() -> Result<()> {
    let text = project::add(PROJECT, &banda(), &NAMES, &files(false))?;
    let target = "HILEN_MOBILE_PROJECT_NAME_CAMEL_CASEWidget";
    assert!(text.contains(&format!(
        "path = {target}.appex; sourceTree = BUILT_PRODUCTS_DIR;"
    )));
    assert!(text.contains("productType = \"com.apple.product-type.app-extension\";"));
    assert!(text.contains("dstSubfolderSpec = 13;"));
    assert!(text.contains("PRODUCT_BUNDLE_IDENTIFIER = HILEN_MOBILE_BINDLE_IDENTIFIER.widget;"));
    assert!(text.contains("path = \"../../mobile-src/shared/Attributes.swift\"; sourceTree = SOURCE_ROOT;"));
    let app_target = &text[text
        .find("D1392BE628CE4325000D8CF5 /* HILEN_MOBILE_PROJECT_NAME_CAMEL_CASE */ = {")
        .unwrap()..];
    let app_target = &app_target[..app_target.find("};").unwrap()];
    assert!(app_target.contains("/* Embed Foundation Extensions */,"));
    assert!(app_target.contains("/* PBXTargetDependency */,"));
    Ok(())
}

#[test]
fn the_build_script_leaves_the_minimum_of_the_extension() -> Result<()> {
    let text = project::add(PROJECT, &banda(), &NAMES, &files(false))?;
    assert_eq!(text.matches("IPHONEOS_DEPLOYMENT_TARGET = \"17.2\";").count(), 2);
    // `build-project.rs` matches `IPHONEOS_DEPLOYMENT_TARGET = [0-9.]+;`, every
    // line it can match is one of the template.
    assert_eq!(
        text.matches("IPHONEOS_DEPLOYMENT_TARGET = 1").count(),
        PROJECT.matches("IPHONEOS_DEPLOYMENT_TARGET = 1").count()
    );
    Ok(())
}

#[test]
fn swift_in_the_app_turns_swift_on_for_the_app() -> Result<()> {
    let with = project::add(PROJECT, &banda(), &NAMES, &files(true))?;
    let app_lines =
        "PRODUCT_BUNDLE_IDENTIFIER = HILEN_MOBILE_BINDLE_IDENTIFIER;\n\t\t\t\tSWIFT_VERSION = 5.0;";
    assert_eq!(with.matches(app_lines).count(), 2);
    assert!(with.contains("/* main.m in Sources */,\n\t\t\t\tF1D6E7"));
    // The Debug configurations of the app and of the extension.
    assert_eq!(
        with.matches("SWIFT_ACTIVE_COMPILATION_CONDITIONS = DEBUG;").count(),
        2
    );
    assert_eq!(
        with.matches("\"-weak_framework\",\n\t\t\t\t\tActivityKit,").count(),
        2
    );

    let shared_only = SwiftFiles {
        extension: vec!["w/W.swift".into()],
        ..SwiftFiles::default()
    };
    let without = project::add(PROJECT, &banda(), &NAMES, &shared_only)?;
    assert_eq!(without.matches(app_lines).count(), 0);
    assert_eq!(
        without.matches("SWIFT_ACTIVE_COMPILATION_CONDITIONS = DEBUG;").count(),
        1
    );

    let no_live = options("[widget]\nextension = \"w\"")?.unwrap();
    let plain = project::add(PROJECT, &no_live, &NAMES, &shared_only)?;
    assert!(!plain.contains("ActivityKit"));
    Ok(())
}

#[test]
fn both_targets_get_the_keychain_group() -> Result<()> {
    let options = banda();
    let info = parse(&options.app_info(INFO, &NAMES)?)?;
    let group = "$(AppIdentifierPrefix)HILEN_MOBILE_BINDLE_IDENTIFIER.shared";
    assert_eq!(
        info.get(KEYCHAIN_GROUP_KEY).and_then(Plist::as_string),
        Some(group)
    );
    assert_eq!(info.get(LIVE_ACTIVITIES), Some(&Plist::Boolean(true)));
    assert_eq!(info.get(FREQUENT_UPDATES), Some(&Plist::Boolean(true)));
    for (key, value) in &parse(INFO)? {
        assert_eq!(info.get(key), Some(value), "{key}");
    }

    let entitlements = parse(&options.app_entitlements(ENTITLEMENTS, &NAMES)?)?;
    let groups: Vec<&str> = entitlements
        .get(KEYCHAIN_GROUPS)
        .and_then(Plist::as_array)
        .unwrap()
        .iter()
        .map(|value| value.as_string().unwrap())
        .collect();
    assert_eq!(
        groups,
        ["$(AppIdentifierPrefix)HILEN_MOBILE_BINDLE_IDENTIFIER", group]
    );
    assert_eq!(
        entitlements.get(APS_ENVIRONMENT).and_then(Plist::as_string),
        Some("development")
    );

    let widget = parse(&options.widget_entitlements(&NAMES)?)?;
    assert_eq!(
        widget.get(KEYCHAIN_GROUPS).and_then(Plist::as_array).unwrap(),
        &vec![Plist::from(group)]
    );
    assert!(widget.contains_key(APS_ENVIRONMENT));
    Ok(())
}

#[test]
fn no_push_and_no_live_activities_leave_their_keys_out() -> Result<()> {
    let options = options("[widget]\nextension = \"w\"")?.unwrap();
    let info = parse(&options.app_info(INFO, &NAMES)?)?;
    assert!(!info.contains_key(LIVE_ACTIVITIES));
    assert!(!parse(&options.app_entitlements(ENTITLEMENTS, &NAMES)?)?.contains_key(APS_ENVIRONMENT));
    assert!(!parse(&options.widget_entitlements(&NAMES)?)?.contains_key(APS_ENVIRONMENT));
    Ok(())
}

#[test]
fn the_extension_has_the_version_of_the_app() -> Result<()> {
    let info = parse(&banda().widget_info(&NAMES)?)?;
    assert_eq!(
        info.get("CFBundleShortVersionString").and_then(Plist::as_string),
        Some("0.1.38")
    );
    let extension = info.get("NSExtension").and_then(Plist::as_dictionary).unwrap();
    assert_eq!(
        extension.get("NSExtensionPointIdentifier").and_then(Plist::as_string),
        Some("com.apple.widgetkit-extension")
    );
    Ok(())
}

#[test]
fn main_starts_the_swift_of_the_app_first() -> Result<()> {
    let main = main_m(MAIN)?;
    let start = main.find("NSClassFromString(@\"HilenAppStart\")").unwrap();
    assert!(start < main.find("return hilen_start_app();").unwrap());
    assert!(main_m("int main() { return 0; }").is_err());
    Ok(())
}

#[test]
fn swift_files_are_found_in_sub_folders_and_sorted() -> Result<()> {
    let root = temp_dir().join(format!("hilen-mobile-widget-{}", id()));
    create_dir_all(root.join("b"))?;
    write(root.join("b/Z.swift"), "")?;
    write(root.join("A.swift"), "")?;
    write(root.join("notes.md"), "")?;
    let found = swift_files(&root.display().to_string());
    remove_dir_all(&root)?;
    let found = found?;
    assert_eq!(found.len(), 2);
    assert!(found[0].ends_with("/A.swift"), "{found:?}");
    assert!(found[1].ends_with("/b/Z.swift"), "{found:?}");
    Ok(())
}
