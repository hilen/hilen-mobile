//! The Swift of the intents in the app target of `project.pbxproj`, put into
//! the text at fixed places of the template, like the widget target.

use anyhow::{Result, ensure};

use super::{HEADER_FILE, SWIFT_FILE};

const MAIN_SOURCE: &str = "\t\t\t\tD1392BEA28CE4325000D8CF5 /* main.m in Sources */,\n";
const HEADER_IN_GROUP: &str = "\t\t\t\tD1753F532BA3CEC10076158C /* hilen.h */,\n";

/// Ids of the added objects. The template uses ids that start with `D1` and
/// `E7A`, the widget `F1D6E7`, these `F1A7E0`.
const SWIFT_REF: &str = "F1A7E0000000000000000001";
const HEADER_REF: &str = "F1A7E0000000000000000002";
const SWIFT_BUILD: &str = "F1A7E0000000000000000003";

const SWIFT_VERSION: &str = "\t\t\t\tSWIFT_VERSION = 5.0;\n";

pub fn add(text: &str, camel: &str, bundle: &str) -> Result<String> {
    let mut text = text.to_string();
    text = before(
        text,
        "/* End PBXBuildFile section */",
        &format!(
            "\t\t{SWIFT_BUILD} /* {SWIFT_FILE} in Sources */ = {{isa = PBXBuildFile; fileRef = {SWIFT_REF} \
             /* {SWIFT_FILE} */; }};\n"
        ),
    )?;
    text = before(
        text,
        "/* End PBXFileReference section */",
        &format!(
            r#"		{SWIFT_REF} /* {SWIFT_FILE} */ = {{isa = PBXFileReference; lastKnownFileType = sourcecode.swift; path = {SWIFT_FILE}; sourceTree = "<group>"; }};
		{HEADER_REF} /* {HEADER_FILE} */ = {{isa = PBXFileReference; lastKnownFileType = sourcecode.c.h; path = {HEADER_FILE}; sourceTree = "<group>"; }};
"#
        ),
    )?;
    text = after(
        text,
        HEADER_IN_GROUP,
        &format!("\t\t\t\t{SWIFT_REF} /* {SWIFT_FILE} */,\n\t\t\t\t{HEADER_REF} /* {HEADER_FILE} */,\n"),
    )?;
    text = after(
        text,
        MAIN_SOURCE,
        &format!("\t\t\t\t{SWIFT_BUILD} /* {SWIFT_FILE} in Sources */,\n"),
    )?;

    // The widget puts the Swift version in already when it has Swift of the
    // app, it must be there once.
    let line = format!("\t\t\t\tPRODUCT_BUNDLE_IDENTIFIER = {bundle};\n");
    ensure!(
        text.matches(&line).count() == 2,
        "the project of the template has another shape, the app target has no 2 configurations with {line:?}"
    );
    let header = format!("\t\t\t\tSWIFT_OBJC_BRIDGING_HEADER = \"{camel}/{HEADER_FILE}\";\n");
    let with_version = format!("{line}{SWIFT_VERSION}");
    text = if text.matches(&with_version).count() == 2 {
        text.replace(&with_version, &format!("{with_version}{header}"))
    } else {
        text.replace(&line, &format!("{line}{SWIFT_VERSION}{header}"))
    };
    Ok(text)
}

fn once(text: &str, anchor: &str) -> Result<()> {
    ensure!(
        text.matches(anchor).count() == 1,
        "the project of the template has another shape, {anchor:?} is not in it once"
    );
    Ok(())
}

fn before(text: String, anchor: &str, lines: &str) -> Result<String> {
    once(&text, anchor)?;
    Ok(text.replacen(anchor, &format!("{lines}{anchor}"), 1))
}

fn after(text: String, anchor: &str, lines: &str) -> Result<String> {
    once(&text, anchor)?;
    Ok(text.replacen(anchor, &format!("{anchor}{lines}"), 1))
}
