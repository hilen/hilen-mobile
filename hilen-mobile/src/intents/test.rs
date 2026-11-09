use anyhow::Result;
use toml::Value;

use super::{IntentOptions, project, swift};

// The real template, so a change of it is tested too.
const PROJECT: &str = include_str!(
    "../../../mobile-template/iOS/HILEN_MOBILE_PROJECT_NAME_CAMEL_CASE.xcodeproj/project.pbxproj"
);
const CAMEL: &str = "HILEN_MOBILE_PROJECT_NAME_CAMEL_CASE";
const BUNDLE: &str = "HILEN_MOBILE_BINDLE_IDENTIFIER";

const SHOPPING: &str = r#"
[[intent]]
name    = "add_item"
title   = "Add an item"
locked  = true
phrases = ["Add to {app}", "Add to {list} in {app}"]

[[intent.parameter]]
name   = "item"
title  = "Item"
kind   = "text"
prompt = "What do you want to add?"

[[intent.parameter]]
name     = "list"
title    = "List"
kind     = "choice"
optional = true

[[intent]]
name    = "clear"
title   = "Clear a \"list\""
phrases = ["Clear {list} in {app}"]

[[intent.parameter]]
name  = "list"
title = "List"
kind  = "choice"
"#;

fn read(toml: &str) -> Result<Option<IntentOptions>> {
    IntentOptions::read(&toml.parse::<Value>()?)
}

fn error(toml: &str) -> String {
    format!("{:#}", read(toml).unwrap_err())
}

const ONE: &str = "[[intent]]\nname = \"add\"\ntitle = \"Add\"\n";

#[test]
fn an_app_with_no_table_has_no_intents() {
    assert_eq!(read("project_name = \"x\"").unwrap(), None);
}

#[test]
fn the_swift_has_the_intents_the_choice_and_the_phrases() {
    let options = read(SHOPPING).unwrap().unwrap();
    let swift = swift::source(&options).unwrap();

    assert!(swift.contains("struct AddItemIntent: AppIntent {"));
    assert!(swift.contains("static let authenticationPolicy: IntentAuthenticationPolicy = .alwaysAllowed"));
    assert!(swift.contains(
        "@Parameter(title: \"Item\", requestValueDialog: IntentDialog(\"What do you want to add?\"))\n    \
         var item: String"
    ));
    assert!(swift.contains("@Parameter(title: \"List\")\n    var list: ListChoice?"));
    assert!(swift.contains("        args[\"item\"] = item\n"));
    assert!(swift.contains("        if let value = list?.id { args[\"list\"] = value }\n"));
    assert!(swift.contains("await HilenIntentBridge.run(\"add_item\", args)"));

    // Not locked is the default, and a title keeps its quotes.
    assert!(swift.contains("struct ClearIntent: AppIntent {"));
    assert!(swift.contains("static let title: LocalizedStringResource = \"Clear a \\\"list\\\"\""));
    assert!(swift.contains("= .requiresAuthentication"));
    assert!(swift.contains("        args[\"list\"] = list.id\n"));

    // 2 intents share the choice list, it is 1 entity.
    assert_eq!(swift.matches("struct ListChoice: AppEntity {").count(), 1);
    assert!(swift.contains("HilenIntentBridge.options(\"list\")"));

    assert!(
        swift.contains(
            "AppShortcut(intent: AddItemIntent(), phrases: [\n            \"Add to \
             \\(.applicationName)\",\n            \"Add to \\(\\.$list) in \\(.applicationName)\"\n        \
             ])"
        )
    );
    assert!(swift.contains("@objc(HilenIntents)"));
    // Every type is for the iOS that has App Intents.
    assert_eq!(
        swift.matches("\nstruct ").count() + swift.matches("\nenum ").count(),
        swift.matches("@available(iOS 16.0, *)\nstruct ").count()
            + swift.matches("@available(iOS 16.0, *)\nenum ").count()
    );
}

#[test]
fn an_intent_with_no_parameters_declares_no_var() {
    let options = read(&format!("{ONE}phrases = [\"Go {{app}}\"]")).unwrap().unwrap();
    let swift = swift::source(&options).unwrap();
    assert!(swift.contains("let args: [String: String] = [:]"));
}

#[test]
fn a_phrase_needs_the_name_of_the_app() {
    assert!(error(&format!("{ONE}phrases = [\"Add\"]")).contains("has no {app}"));
    assert!(error(&format!("{ONE}phrases = []")).contains("has no phrases"));
}

#[test]
fn a_text_can_not_be_part_of_a_phrase() {
    let toml = format!(
        "{ONE}phrases = [\"Add {{item}} to {{app}}\"]\n[[intent.parameter]]\nname = \"item\"\ntitle = \
         \"Item\"\nkind = \"text\""
    );
    assert!(error(&toml).contains("Apple takes only a choice in a phrase"));
}

#[test]
fn a_phrase_names_1_known_parameter_at_most() {
    assert!(error(&format!("{ONE}phrases = [\"Add {{list}} to {{app}}\"]")).contains("no parameter of it"));
    let toml = format!(
        r#"{ONE}phrases = ["{{a}} {{b}} {{app}}"]
[[intent.parameter]]
name = "a"
title = "A"
kind = "choice"
[[intent.parameter]]
name = "b"
title = "B"
kind = "choice"
"#
    );
    assert!(error(&toml).contains("names 2 parameters"));
}

#[test]
fn a_broken_brace_is_an_error() {
    assert!(error(&format!("{ONE}phrases = [\"Add {{app\"]")).contains("with no }"));
    assert!(error(&format!("{ONE}phrases = [\"Add }} {{app}}\"]")).contains("with no { before it"));
}

#[test]
fn names_are_snake_case_and_no_swift_words() {
    let toml = "[[intent]]\nname = \"AddItem\"\ntitle = \"A\"\nphrases = [\"{app}\"]";
    assert!(error(toml).contains("lower case letters"));
    let toml = "[[intent]]\nname = \"add\"\ntitle = \"A\"\nphrases = \
                [\"{app}\"]\n[[intent.parameter]]\nname = \"in\"\ntitle = \"In\"\nkind = \"text\"";
    assert!(error(toml).contains("a word Swift keeps"));
}

#[test]
fn a_parameter_is_a_text_or_a_choice_everywhere() {
    let toml = r#"
[[intent]]
name = "a"
title = "A"
phrases = ["{app}"]
[[intent.parameter]]
name = "list"
title = "List"
kind = "text"
[[intent]]
name = "b"
title = "B"
phrases = ["{list} {app}"]
[[intent.parameter]]
name = "list"
title = "List"
kind = "choice"
"#;
    assert!(error(toml).contains("a text in 1 intent and a choice in another"));
}

#[test]
fn apple_takes_10_shortcuts_at_most() {
    let toml: String = (0..11)
        .map(|n| format!("[[intent]]\nname = \"i{n}\"\ntitle = \"I\"\nphrases = [\"{{app}}\"]\n"))
        .collect();
    assert!(error(&toml).contains("at most 10"));
}

#[test]
fn the_project_compiles_the_swift_with_its_header() {
    let text = project::add(PROJECT, CAMEL, BUNDLE).unwrap();
    assert_eq!(text.matches("HilenIntents.swift in Sources */,").count(), 1);
    assert_eq!(text.matches("/* HilenIntents.h */,").count(), 1);
    let header = format!("SWIFT_OBJC_BRIDGING_HEADER = \"{CAMEL}/HilenIntents.h\";");
    assert_eq!(text.matches(&header).count(), 2);
    // The 2 of the SystemInput target and the 2 of the app.
    assert_eq!(text.matches("SWIFT_VERSION = 5.0;").count(), 4);
}

#[test]
fn the_swift_version_of_the_widget_is_not_written_twice() {
    let line = format!("\t\t\t\tPRODUCT_BUNDLE_IDENTIFIER = {BUNDLE};\n");
    let widget = PROJECT.replace(&line, &format!("{line}\t\t\t\tSWIFT_VERSION = 5.0;\n"));
    let text = project::add(&widget, CAMEL, BUNDLE).unwrap();
    assert_eq!(text.matches("SWIFT_VERSION = 5.0;").count(), 4);
    assert_eq!(text.matches("SWIFT_OBJC_BRIDGING_HEADER").count(), 2);
}
