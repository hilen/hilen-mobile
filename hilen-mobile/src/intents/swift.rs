//! The Swift of the intents. Every type is for iOS 16 and later, App Intents
//! came then, and the app itself may start on an older iOS. The Swift calls
//! Rust on the main actor: UIKit drains the main queue also when iOS started
//! the app in the background for an intent, with no window and no frame.

use std::fmt::Write;

use anyhow::Result;
use convert_case::{Case, Casing};

use super::{Intent, IntentOptions, Kind, Parameter, Piece, pieces};

/// The bridging header of the app target, the C functions of
/// `hilen/src/intents/ios.rs`.
pub const HEADER: &str = r#"// Made by hilen-mobile from the [[intent]] tables of hilen.toml.
// The C functions of the intents feature of hilen, for HilenIntents.swift.

#include <stdbool.h>

typedef void (*hilen_intent_done)(void *context, bool ok, const char *text);

void hilen_intent_run(const char *name, const char *args, void *context, hilen_intent_done done);
char *hilen_intent_options(const char *parameter);
void hilen_intent_free(char *text);
"#;

/// The part that is the same in every app.
const BRIDGE: &str = r#"// Made by hilen-mobile from the [[intent]] tables of hilen.toml.

import AppIntents
import Foundation

@available(iOS 16.0, *)
struct HilenIntentOption: Decodable {
    let id: String
    let title: String
}

/// Siri says the text of a failed run.
@available(iOS 16.0, *)
struct HilenIntentError: Error, CustomLocalizedStringResourceConvertible {
    let text: String

    var localizedStringResource: LocalizedStringResource {
        LocalizedStringResource(stringLiteral: text)
    }
}

@available(iOS 16.0, *)
enum HilenIntentBridge {
    private final class Waiter {
        let continuation: CheckedContinuation<(Bool, String), Never>

        init(_ continuation: CheckedContinuation<(Bool, String), Never>) {
            self.continuation = continuation
        }
    }

    @MainActor
    static func run(_ name: String, _ args: [String: String]) async -> (Bool, String) {
        let json: String
        do {
            json = String(decoding: try JSONEncoder().encode(args), as: UTF8.self)
        } catch {
            NSLog("hilen: the parameters of intent %@ make no JSON: %@", name, "\(error)")
            return (false, "\(error)")
        }
        return await withCheckedContinuation { continuation in
            let waiter = Unmanaged.passRetained(Waiter(continuation)).toOpaque()
            hilen_intent_run(name, json, waiter) { context, ok, text in
                guard let context else { return }
                let waiter = Unmanaged<Waiter>.fromOpaque(context).takeRetainedValue()
                waiter.continuation.resume(returning: (ok, text.map { String(cString: $0) } ?? ""))
            }
        }
    }

    @MainActor
    static func options(_ parameter: String) -> [HilenIntentOption] {
        guard let raw = hilen_intent_options(parameter) else { return [] }
        defer { hilen_intent_free(raw) }
        do {
            return try JSONDecoder().decode([HilenIntentOption].self, from: Data(String(cString: raw).utf8))
        } catch {
            NSLog("hilen: the values of choice %@ are no list: %@", parameter, "\(error)")
            return []
        }
    }
}

/// `Intents::options_changed` of hilen calls this, so Siri learns the new
/// values of the choices in the phrases.
@objc(HilenIntents)
final class HilenIntents: NSObject {
    @objc static func updateParameters() {
        if #available(iOS 16.0, *) {
            HilenShortcuts.updateAppShortcutParameters()
        }
    }
}
"#;

/// A Swift string literal of `text`.
fn literal(text: &str) -> String {
    let mut out = String::from("\"");
    for c in text.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

fn choice_type(parameter: &Parameter) -> String {
    format!("{}Choice", parameter.name.to_case(Case::UpperCamel))
}

pub fn source(options: &IntentOptions) -> Result<String> {
    let mut out = String::from(BRIDGE);
    for choice in options.choices()? {
        write_choice(&mut out, choice)?;
    }
    for intent in &options.intents {
        write_intent(&mut out, intent)?;
    }
    write_shortcuts(&mut out, options)?;
    Ok(out)
}

fn write_choice(out: &mut String, parameter: &Parameter) -> Result<()> {
    let ty = choice_type(parameter);
    let name = literal(&parameter.name);
    let title = literal(&parameter.title);
    write!(
        out,
        r#"
@available(iOS 16.0, *)
struct {ty}: AppEntity {{
    static let typeDisplayRepresentation: TypeDisplayRepresentation = {title}
    static let defaultQuery = {ty}Query()

    let id: String
    let title: String

    var displayRepresentation: DisplayRepresentation {{
        DisplayRepresentation(title: LocalizedStringResource(stringLiteral: title))
    }}
}}

@available(iOS 16.0, *)
struct {ty}Query: EntityStringQuery {{
    private func all() async -> [{ty}] {{
        await HilenIntentBridge.options({name}).map {{ {ty}(id: $0.id, title: $0.title) }}
    }}

    func entities(for identifiers: [String]) async throws -> [{ty}] {{
        await all().filter {{ identifiers.contains($0.id) }}
    }}

    func entities(matching string: String) async throws -> [{ty}] {{
        await all().filter {{ $0.title.localizedCaseInsensitiveContains(string) }}
    }}

    func suggestedEntities() async throws -> [{ty}] {{
        await all()
    }}
}}
"#
    )?;
    Ok(())
}

fn write_intent(out: &mut String, intent: &Intent) -> Result<()> {
    let ty = intent.type_name();
    let title = literal(&intent.title);
    let policy = if intent.locked {
        ".alwaysAllowed"
    } else {
        ".requiresAuthentication"
    };
    write!(
        out,
        r#"
@available(iOS 16.0, *)
struct {ty}: AppIntent {{
    static let title: LocalizedStringResource = {title}
    static let openAppWhenRun = false
    static let authenticationPolicy: IntentAuthenticationPolicy = {policy}
"#
    )?;
    let mut args = String::new();
    for parameter in &intent.parameters {
        let name = &parameter.name;
        let key = literal(name);
        let base = match parameter.kind {
            Kind::Text => "String".to_string(),
            Kind::Choice => choice_type(parameter),
        };
        let ty = if parameter.optional {
            format!("{base}?")
        } else {
            base
        };
        let prompt = parameter.prompt.as_deref().map(literal);
        let attributes = match &prompt {
            Some(prompt) => format!(
                "title: {}, requestValueDialog: IntentDialog({prompt})",
                literal(&parameter.title)
            ),
            None => format!("title: {}", literal(&parameter.title)),
        };
        writeln!(out, "\n    @Parameter({attributes})\n    var {name}: {ty}")?;
        let value = match parameter.kind {
            Kind::Text => name.clone(),
            Kind::Choice => format!("{name}{}.id", if parameter.optional { "?" } else { "" }),
        };
        if parameter.optional {
            writeln!(args, "        if let value = {value} {{ args[{key}] = value }}")?;
        } else {
            writeln!(args, "        args[{key}] = {value}")?;
        }
    }
    let declare = if args.is_empty() { "let" } else { "var" };
    write!(
        out,
        r#"
    @MainActor
    func perform() async throws -> some IntentResult & ProvidesDialog {{
        {declare} args: [String: String] = [:]
{args}        let (ok, text) = await HilenIntentBridge.run({name}, args)
        guard ok else {{ throw HilenIntentError(text: text) }}
        return .result(dialog: IntentDialog(stringLiteral: text))
    }}
}}
"#,
        name = literal(&intent.name)
    )?;
    Ok(())
}

/// A phrase as Swift, `{app}` is the name of the app and `{list}` the value
/// of a choice.
fn phrase(text: &str) -> Result<String> {
    let mut out = String::from("\"");
    for piece in pieces(text)? {
        match piece {
            Piece::Text(text) => {
                let quoted = literal(text);
                out.push_str(&quoted[1..quoted.len() - 1]);
            }
            Piece::App => out.push_str("\\(.applicationName)"),
            Piece::Parameter(name) => write!(out, "\\(\\.${name})")?,
        }
    }
    out.push('"');
    Ok(out)
}

fn write_shortcuts(out: &mut String, options: &IntentOptions) -> Result<()> {
    out.push_str(
        r#"
@available(iOS 16.0, *)
struct HilenShortcuts: AppShortcutsProvider {
    @AppShortcutsBuilder
    static var appShortcuts: [AppShortcut] {
"#,
    );
    for intent in &options.intents {
        let phrases = intent.phrases.iter().map(|text| phrase(text)).collect::<Result<Vec<_>>>()?;
        write!(
            out,
            "        AppShortcut(intent: {}(), phrases: [\n            {}\n        ])\n",
            intent.type_name(),
            phrases.join(",\n            ")
        )?;
    }
    out.push_str("    }\n}\n");
    Ok(())
}
