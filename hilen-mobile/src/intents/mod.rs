//! The `[[intent]]` tables of `hilen.toml`, App Intents on iOS that Siri, the
//! Shortcuts app and Spotlight run, each one calling a Rust handler of the
//! app through the `intents` feature of hilen:
//!
//! ```toml
//! [[intent]]
//! name    = "add_item"
//! title   = "Add an item"
//! locked  = true
//! phrases = ["Add to {app}", "Add to {list} in {app}"]
//!
//! [[intent.parameter]]
//! name   = "item"
//! title  = "Item"
//! kind   = "text"
//! prompt = "What do you want to add?"
//!
//! [[intent.parameter]]
//! name     = "list"
//! title    = "List"
//! kind     = "choice"
//! optional = true
//! ```
//!
//! Apple reads the intents from the Swift of the app when it is built, so the
//! Swift is made here, `HilenIntents.swift` in the app target, with
//! `HilenIntents.h` as its bridging header to the C functions of hilen.

mod project;
mod swift;

use std::{
    collections::HashSet,
    fs::{read_to_string, write},
    path::Path,
};

use anyhow::{Context, Result, bail, ensure};
use convert_case::{Case, Casing};
use serde::Deserialize;
use toml::Value;

/// Apple takes no more App Shortcuts from 1 app.
const MAX_SHORTCUTS: usize = 10;

const SWIFT_FILE: &str = "HilenIntents.swift";
const HEADER_FILE: &str = "HilenIntents.h";

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Intent {
    name:       String,
    title:      String,
    /// Runs on a locked phone, with no Face ID first.
    #[serde(default)]
    locked:     bool,
    phrases:    Vec<String>,
    #[serde(default, rename = "parameter")]
    parameters: Vec<Parameter>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    /// Any text, Siri asks for it after the phrase.
    Text,
    /// 1 of the values the app gives with `Intents::options`, it can be part
    /// of a phrase.
    Choice,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Parameter {
    name:     String,
    title:    String,
    kind:     Kind,
    /// What Siri asks when the value is missing.
    prompt:   Option<String>,
    #[serde(default)]
    optional: bool,
}

/// A piece of a phrase.
#[derive(Debug, PartialEq, Eq)]
enum Piece<'a> {
    Text(&'a str),
    App,
    Parameter(&'a str),
}

#[derive(Debug, PartialEq, Eq)]
pub struct IntentOptions {
    intents: Vec<Intent>,
}

/// Words Swift keeps for itself, a parameter can not be named so.
const SWIFT_WORDS: &[&str] = &[
    "as",
    "break",
    "case",
    "catch",
    "class",
    "continue",
    "default",
    "defer",
    "deinit",
    "do",
    "else",
    "enum",
    "extension",
    "fallthrough",
    "false",
    "for",
    "func",
    "guard",
    "if",
    "import",
    "in",
    "init",
    "inout",
    "is",
    "let",
    "nil",
    "operator",
    "protocol",
    "repeat",
    "return",
    "self",
    "static",
    "struct",
    "subscript",
    "super",
    "switch",
    "throw",
    "throws",
    "true",
    "try",
    "var",
    "where",
    "while",
];

impl IntentOptions {
    /// `None` for an app with no `[[intent]]` table.
    pub fn read(project_info: &Value) -> Result<Option<Self>> {
        let Some(tables) = project_info.get("intent") else {
            return Ok(None);
        };
        let intents: Vec<Intent> = tables.clone().try_into().context("[[intent]] tables in hilen.toml")?;
        ensure!(!intents.is_empty(), "[[intent]] in hilen.toml has no intent");
        ensure!(
            intents.len() <= MAX_SHORTCUTS,
            "hilen.toml has {} [[intent]] tables, Apple takes at most {MAX_SHORTCUTS} from 1 app",
            intents.len()
        );
        let mut names = HashSet::new();
        for intent in &intents {
            intent.check()?;
            ensure!(
                names.insert(intent.type_name()),
                "2 [[intent]] tables in hilen.toml have the name {}",
                intent.name
            );
        }
        let options = Self { intents };
        options.choices()?;
        Ok(Some(options))
    }

    /// The choice parameters by name, each once. 2 intents that both have a
    /// choice `list` share its values, so the kind must match.
    fn choices(&self) -> Result<Vec<&Parameter>> {
        let mut choices: Vec<&Parameter> = Vec::new();
        for parameter in self.intents.iter().flat_map(|intent| &intent.parameters) {
            let same = self
                .intents
                .iter()
                .flat_map(|intent| &intent.parameters)
                .find(|p| p.name == parameter.name);
            if let Some(same) = same {
                ensure!(
                    same.kind == parameter.kind,
                    "parameter {} of [[intent]] in hilen.toml is a text in 1 intent and a choice in another",
                    parameter.name
                );
            }
            if parameter.kind == Kind::Choice && !choices.iter().any(|c| c.name == parameter.name) {
                choices.push(parameter);
            }
        }
        Ok(choices)
    }

    /// Writes the Swift and its header into the app folder, `mobile/iOS/<App>`,
    /// and puts both into the project.
    pub fn apply_ios(&self, ios_dir: &Path, camel: &str, bundle: &str) -> Result<()> {
        let app_dir = ios_dir.join(camel);
        write(app_dir.join(SWIFT_FILE), swift::source(self)?)?;
        write(app_dir.join(HEADER_FILE), swift::HEADER)?;
        let path = ios_dir.join(format!("{camel}.xcodeproj/project.pbxproj"));
        let text = read_to_string(&path).with_context(|| format!("{} not found", path.display()))?;
        write(
            &path,
            project::add(&text, camel, bundle).with_context(|| path.display().to_string())?,
        )?;
        println!(
            "Applied {} [[intent]] tables of hilen.toml to {}",
            self.intents.len(),
            app_dir.display()
        );
        Ok(())
    }
}

impl Intent {
    fn type_name(&self) -> String {
        format!("{}Intent", self.name.to_case(Case::UpperCamel))
    }

    fn check(&self) -> Result<()> {
        check_name(&self.name, "an [[intent]]")?;
        ensure!(
            !self.title.trim().is_empty(),
            "[[intent]] {} in hilen.toml has no title",
            self.name
        );
        let mut names = HashSet::new();
        for parameter in &self.parameters {
            check_name(
                &parameter.name,
                &format!("a parameter of [[intent]] {}", self.name),
            )?;
            ensure!(
                names.insert(&parameter.name),
                "[[intent]] {} in hilen.toml has 2 parameters named {}",
                self.name,
                parameter.name
            );
            ensure!(
                !parameter.title.trim().is_empty(),
                "parameter {} of [[intent]] {} in hilen.toml has no title",
                parameter.name,
                self.name
            );
        }
        ensure!(
            !self.phrases.is_empty(),
            "[[intent]] {} in hilen.toml has no phrases, Siri would not know it",
            self.name
        );
        for phrase in &self.phrases {
            self.check_phrase(phrase)?;
        }
        Ok(())
    }

    fn check_phrase(&self, phrase: &str) -> Result<()> {
        let pieces =
            pieces(phrase).with_context(|| format!("phrase {phrase:?} of [[intent]] {}", self.name))?;
        ensure!(
            pieces.contains(&Piece::App),
            "phrase {phrase:?} of [[intent]] {} has no {{app}}, Apple wants the name of the app in each \
             phrase",
            self.name
        );
        let used: Vec<&str> = pieces
            .iter()
            .filter_map(|piece| match piece {
                Piece::Parameter(name) => Some(*name),
                _ => None,
            })
            .collect();
        ensure!(
            used.len() <= 1,
            "phrase {phrase:?} of [[intent]] {} names {} parameters, Apple takes at most 1 in a phrase",
            self.name,
            used.len()
        );
        for name in used {
            let Some(parameter) = self.parameters.iter().find(|p| p.name == name) else {
                bail!(
                    "phrase {phrase:?} of [[intent]] {} names {{{name}}}, which is no parameter of it",
                    self.name
                );
            };
            ensure!(
                parameter.kind == Kind::Choice,
                "phrase {phrase:?} of [[intent]] {} names the text {{{name}}}, Apple takes only a choice in \
                 a phrase, Siri asks for a text after it",
                self.name
            );
        }
        Ok(())
    }
}

fn check_name(name: &str, what: &str) -> Result<()> {
    let mut chars = name.chars();
    ensure!(
        chars.next().is_some_and(|c| c.is_ascii_lowercase())
            && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_'),
        "{what} in hilen.toml is named {name:?}, a name is lower case letters, digits and _, like add_item"
    );
    ensure!(
        !SWIFT_WORDS.contains(&name),
        "{what} in hilen.toml is named {name:?}, a word Swift keeps for itself"
    );
    Ok(())
}

/// Splits a phrase at its `{...}` parts.
fn pieces(phrase: &str) -> Result<Vec<Piece<'_>>> {
    let mut pieces = Vec::new();
    let mut rest = phrase;
    while let Some(open) = rest.find(['{', '}']) {
        ensure!(rest[open..].starts_with('{'), "a }} with no {{ before it");
        if open > 0 {
            pieces.push(Piece::Text(&rest[..open]));
        }
        let Some(close) = rest[open..].find('}') else {
            bail!("a {{ with no }} after it");
        };
        let name = &rest[open + 1..open + close];
        ensure!(!name.contains('{'), "a {{ inside {{...}}");
        pieces.push(if name == "app" {
            Piece::App
        } else {
            Piece::Parameter(name)
        });
        rest = &rest[open + close + 1..];
    }
    if !rest.is_empty() {
        pieces.push(Piece::Text(rest));
    }
    Ok(pieces)
}

#[cfg(test)]
mod test;
