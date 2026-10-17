//! The widget extension target in `project.pbxproj`. The template has only the
//! app and the `SystemInput` targets, so the extension and the steps that build
//! and embed it are put into the text at fixed places of the template.

use anyhow::{Result, ensure};

use super::{SwiftFiles, WidgetNames, WidgetOptions};

const PROJECT: &str = "D1411E722617166700F3F18B";
const MAIN_SOURCE: &str = "\t\t\t\tD1392BEA28CE4325000D8CF5 /* main.m in Sources */,\n";
const SYSTEM_INPUT_GROUP: &str = "\t\t\t\tE7A000000000000000000005 /* SystemInput */,\n";
const SYSTEM_INPUT_TARGET: &str = "\t\t\t\tE7A000000000000000000006 /* SystemInput */,\n";
const APP_RESOURCES: &str = "\t\t\t\tD1392BF528CE4325000D8CF5 /* Resources */,\n";
const NO_DEPENDENCIES: &str = "\t\t\t);\n\t\t\tbuildRules = (\n\t\t\t);\n\t\t\tdependencies = (\n\t\t\t);\n";
const APP_DEBUG: &str = "\t\tD1392BFA28CE4325000D8CF5 /* Debug */ = {\n\t\t\tisa = \
                         XCBuildConfiguration;\n\t\t\tbuildSettings = {\n";
const APP_LINKER_FLAGS: &str = "\t\t\t\tOTHER_LDFLAGS = (\n";

/// Ids of the added objects. The template uses ids that start with `D1` and
/// `E7A`, these start with `F1D6E7`.
struct Ids(u32);

impl Ids {
    fn next(&mut self) -> String {
        self.0 += 1;
        format!("F1D6E7{:018X}", self.0)
    }
}

struct Source {
    name:      String,
    path:      String,
    file:      String,
    in_app:    Option<String>,
    in_widget: Option<String>,
}

pub fn add(text: &str, options: &WidgetOptions, names: &WidgetNames, files: &SwiftFiles) -> Result<String> {
    let mut ids = Ids(0);
    let target = names.target();
    let camel = names.camel;
    let bundle = names.bundle;

    let mut sources = Vec::new();
    for (paths, app, widget) in [
        (&files.extension, false, true),
        (&files.app, true, false),
        (&files.shared, true, true),
    ] {
        for path in paths {
            sources.push(Source {
                name:      path.rsplit('/').next().unwrap_or(path).to_string(),
                path:      path.clone(),
                file:      ids.next(),
                in_app:    app.then(|| ids.next()),
                in_widget: widget.then(|| ids.next()),
            });
        }
    }

    let appex = ids.next();
    let info = ids.next();
    let entitlements = ids.next();
    let embed = ids.next();
    let group = ids.next();
    let frameworks = ids.next();
    let widget_sources = ids.next();
    let copy = ids.next();
    let proxy = ids.next();
    let dependency = ids.next();
    let target_id = ids.next();
    let debug = ids.next();
    let release = ids.next();
    let configurations = ids.next();

    let mut build_files = String::new();
    let mut references = String::new();
    let mut group_children =
        format!("\t\t\t\t{info} /* Info.plist */,\n\t\t\t\t{entitlements} /* {target}.entitlements */,\n");
    let mut app_source_lines = String::new();
    let mut widget_source_lines = String::new();
    for source in &sources {
        let Source { name, path, file, .. } = source;
        references.push_str(&format!(
            "\t\t{file} /* {name} */ = {{isa = PBXFileReference; lastKnownFileType = sourcecode.swift; name \
             = \"{name}\"; path = \"../../{path}\"; sourceTree = SOURCE_ROOT; }};\n"
        ));
        group_children.push_str(&format!("\t\t\t\t{file} /* {name} */,\n"));
        for (id, lines) in [
            (&source.in_app, &mut app_source_lines),
            (&source.in_widget, &mut widget_source_lines),
        ] {
            if let Some(id) = id {
                build_files.push_str(&format!(
                    "\t\t{id} /* {name} in Sources */ = {{isa = PBXBuildFile; fileRef = {file} /* {name} \
                     */; }};\n"
                ));
                lines.push_str(&format!("\t\t\t\t{id} /* {name} in Sources */,\n"));
            }
        }
    }
    build_files.push_str(&format!(
        "\t\t{embed} /* {target}.appex in Embed Foundation Extensions */ = {{isa = PBXBuildFile; fileRef = \
         {appex} /* {target}.appex */; settings = {{ATTRIBUTES = (RemoveHeadersOnCopy, ); }}; }};\n"
    ));
    references.push_str(&format!(
        r#"		{appex} /* {target}.appex */ = {{isa = PBXFileReference; explicitFileType = "wrapper.app-extension"; includeInIndex = 0; path = {target}.appex; sourceTree = BUILT_PRODUCTS_DIR; }};
		{info} /* Info.plist */ = {{isa = PBXFileReference; lastKnownFileType = text.plist.xml; path = Info.plist; sourceTree = "<group>"; }};
		{entitlements} /* {target}.entitlements */ = {{isa = PBXFileReference; lastKnownFileType = text.plist.entitlements; path = {target}.entitlements; sourceTree = "<group>"; }};
"#
    ));

    let mut text = text.to_string();
    text = before(text, "/* End PBXBuildFile section */", &build_files)?;
    text = before(text, "/* End PBXFileReference section */", &references)?;
    text = before(
        text,
        "/* End PBXFrameworksBuildPhase section */",
        &format!(
            r#"		{frameworks} /* Frameworks */ = {{
			isa = PBXFrameworksBuildPhase;
			buildActionMask = 2147483647;
			files = (
			);
			runOnlyForDeploymentPostprocessing = 0;
		}};
"#
        ),
    )?;
    text = before(
        text,
        "/* End PBXGroup section */",
        &format!(
            r#"		{group} /* {target} */ = {{
			isa = PBXGroup;
			children = (
{group_children}			);
			path = {target};
			sourceTree = "<group>";
		}};
"#
        ),
    )?;
    text = after(
        text,
        SYSTEM_INPUT_GROUP,
        &format!("\t\t\t\t{group} /* {target} */,\n"),
    )?;
    text = after(
        text,
        &format!("\t\t\t\tD1392BFC28CE4325000D8CF5 /* {camel}.app */,\n"),
        &format!("\t\t\t\t{appex} /* {target}.appex */,\n"),
    )?;
    text = before(
        text,
        "/* End PBXNativeTarget section */",
        &format!(
            r#"		{target_id} /* {target} */ = {{
			isa = PBXNativeTarget;
			buildConfigurationList = {configurations} /* Build configuration list for PBXNativeTarget "{target}" */;
			buildPhases = (
				{widget_sources} /* Sources */,
				{frameworks} /* Frameworks */,
			);
			buildRules = (
			);
			dependencies = (
			);
			name = {target};
			productName = {target};
			productReference = {appex} /* {target}.appex */;
			productType = "com.apple.product-type.app-extension";
		}};
"#
        ),
    )?;
    text = replace(
        text,
        &format!("{APP_RESOURCES}{NO_DEPENDENCIES}"),
        &format!(
            "{APP_RESOURCES}\t\t\t\t{copy} /* Embed Foundation Extensions */,\n\t\t\t);\n\t\t\tbuildRules = \
             (\n\t\t\t);\n\t\t\tdependencies = (\n\t\t\t\t{dependency} /* PBXTargetDependency \
             */,\n\t\t\t);\n"
        ),
    )?;
    text = after(
        text,
        SYSTEM_INPUT_TARGET,
        &format!("\t\t\t\t{target_id} /* {target} */,\n"),
    )?;
    text = after(text, MAIN_SOURCE, &app_source_lines)?;
    text = before(
        text,
        "/* End PBXSourcesBuildPhase section */",
        &format!(
            r#"		{widget_sources} /* Sources */ = {{
			isa = PBXSourcesBuildPhase;
			buildActionMask = 2147483647;
			files = (
{widget_source_lines}			);
			runOnlyForDeploymentPostprocessing = 0;
		}};
"#
        ),
    )?;
    text = before(
        text,
        "/* Begin PBXFileReference section */",
        &format!(
            r#"/* Begin PBXContainerItemProxy section */
		{proxy} /* PBXContainerItemProxy */ = {{
			isa = PBXContainerItemProxy;
			containerPortal = {PROJECT} /* Project object */;
			proxyType = 1;
			remoteGlobalIDString = {target_id};
			remoteInfo = {target};
		}};
/* End PBXContainerItemProxy section */

/* Begin PBXCopyFilesBuildPhase section */
		{copy} /* Embed Foundation Extensions */ = {{
			isa = PBXCopyFilesBuildPhase;
			buildActionMask = 2147483647;
			dstPath = "";
			dstSubfolderSpec = 13;
			files = (
				{embed} /* {target}.appex in Embed Foundation Extensions */,
			);
			name = "Embed Foundation Extensions";
			runOnlyForDeploymentPostprocessing = 0;
		}};
/* End PBXCopyFilesBuildPhase section */

"#
        ),
    )?;
    text = before(
        text,
        "/* Begin XCBuildConfiguration section */",
        &format!(
            r#"/* Begin PBXTargetDependency section */
		{dependency} /* PBXTargetDependency */ = {{
			isa = PBXTargetDependency;
			target = {target_id} /* {target} */;
			targetProxy = {proxy} /* PBXContainerItemProxy */;
		}};
/* End PBXTargetDependency section */

"#
        ),
    )?;
    let settings = |name: &str, id: &str| {
        // The minimum is quoted. `build/ios/build-project.rs` of the app sets
        // every unquoted `IPHONEOS_DEPLOYMENT_TARGET` to the minimum of the
        // app, which is too old for WidgetKit.
        format!(
            r#"		{id} /* {name} */ = {{
			isa = XCBuildConfiguration;
			buildSettings = {{
				APPLICATION_EXTENSION_API_ONLY = YES;
				CODE_SIGN_ENTITLEMENTS = {target}/{target}.entitlements;
				CODE_SIGN_STYLE = Automatic;
				DEVELOPMENT_TEAM = 2545JX4S8U;
				"EXCLUDED_ARCHS[sdk=iphonesimulator*]" = arm64;
				INFOPLIST_FILE = {target}/Info.plist;
				IPHONEOS_DEPLOYMENT_TARGET = "{minimum}";
				LD_RUNPATH_SEARCH_PATHS = (
					"$(inherited)",
					"@executable_path/Frameworks",
					"@executable_path/../../Frameworks",
				);
				PRODUCT_BUNDLE_IDENTIFIER = {bundle}.widget;
				PRODUCT_NAME = "$(TARGET_NAME)";
				SKIP_INSTALL = YES;
{debug_flag}				SWIFT_VERSION = 5.0;
				TARGETED_DEVICE_FAMILY = "1,2";
			}};
			name = {name};
		}};
"#,
            minimum = options.minimum,
            debug_flag = if name == "Debug" { DEBUG_FLAG } else { "" }
        )
    };
    text = before(
        text,
        "/* End XCBuildConfiguration section */",
        &format!("{}{}", settings("Debug", &debug), settings("Release", &release)),
    )?;
    text = before(
        text,
        "/* End XCConfigurationList section */",
        &format!(
            r#"		{configurations} /* Build configuration list for PBXNativeTarget "{target}" */ = {{
			isa = XCConfigurationList;
			buildConfigurations = (
				{debug} /* Debug */,
				{release} /* Release */,
			);
			defaultConfigurationIsVisible = 0;
			defaultConfigurationName = Release;
		}};
"#
        ),
    )?;
    if files.in_app() {
        let line = format!("\t\t\t\tPRODUCT_BUNDLE_IDENTIFIER = {bundle};\n");
        ensure!(
            text.matches(&line).count() == 2,
            "the project of the template has another shape, the app target has no 2 configurations with \
             {line:?}"
        );
        text = text.replace(&line, &format!("{line}\t\t\t\tSWIFT_VERSION = 5.0;\n"));
        text = after(text, APP_DEBUG, DEBUG_FLAG)?;
    }
    if options.live_activities {
        // The app runs on systems older than ActivityKit, there the
        // framework is missing and a strong link would end the start.
        ensure!(
            text.matches(APP_LINKER_FLAGS).count() == 2,
            "the project of the template has another shape, the app target has no 2 OTHER_LDFLAGS"
        );
        text = text.replace(
            APP_LINKER_FLAGS,
            &format!("{APP_LINKER_FLAGS}\t\t\t\t\t\"-weak_framework\",\n\t\t\t\t\tActivityKit,\n"),
        );
    }
    Ok(text)
}

/// `#if DEBUG` in the Swift of a debug build, which gets its push tokens
/// from the sandbox of APNs.
const DEBUG_FLAG: &str = "\t\t\t\tSWIFT_ACTIVE_COMPILATION_CONDITIONS = DEBUG;\n";

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

fn replace(text: String, anchor: &str, with: &str) -> Result<String> {
    once(&text, anchor)?;
    Ok(text.replacen(anchor, with, 1))
}
