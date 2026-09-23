//! Reads a parsed workflow as text, for the clauses that search values.
//!
//! Split from `rules.rs` under the repository's 400-line cap. The walk turns
//! every key and scalar into one line, and the computed-secret reader judges
//! that text; the rules decide what each finding means.

use serde_yaml::{Mapping, Value};

/// Returns every key and scalar in a parsed value, one per line.
///
/// Reading the parse rather than the file is deliberate: comments are gone by
/// this point, so prose explaining why the token is absent does not read as
/// the token being present, while a block scalar's content, which Actions
/// expands, is read in full. It also reaches every place a value can sit, at
/// workflow, job or step scope, in a `run` body, an action input, an `env`
/// value, an `if:`, a `defaults.run.shell` or a `secrets:` forwarding or
/// declaration, without a clause having to be told about each. Keys carry
/// their `:` so the computed-secret clause can tell a `secrets:` key from an
/// expression. The walk cannot fail, so no clause can pass by searching text
/// that was never produced.
pub(super) fn rendered(value: &Value) -> String {
    let mut text = String::new();
    collect_scalars(value, &mut text);
    text
}

/// Appends the keys and scalars of `value` to `text`, depth first.
fn collect_scalars(value: &Value, text: &mut String) {
    match value {
        Value::Null => {}
        Value::Bool(flag) => push_line(text, &flag.to_string()),
        Value::Number(number) => push_line(text, &number.to_string()),
        Value::String(string) => push_line(text, string),
        Value::Sequence(items) => items.iter().for_each(|item| collect_scalars(item, text)),
        Value::Mapping(mapping) => collect_mapping(mapping, text),
        Value::Tagged(tagged) => collect_scalars(&tagged.value, text),
    }
}

/// Appends each key of `mapping`, with its `:`, followed by its value.
fn collect_mapping(mapping: &Mapping, text: &mut String) {
    for (key, item) in mapping {
        let mut key_text = String::new();
        collect_scalars(key, &mut key_text);
        push_line(text, &format!("{}:", key_text.trim_end()));
        collect_scalars(item, text);
    }
}

/// Appends one line to `text`.
fn push_line(text: &mut String, line: &str) {
    text.push_str(line);
    text.push('\n');
}

/// Returns [`rendered`] case-folded, for the clauses that search for a name.
///
/// Secret names, context names and a DNS host are all case-insensitive, so
/// `secrets.cs_access_token` and `SECRETS[...]` reach the same token as the
/// spelling the clauses name. Folding the text, rather than the needle, is
/// what lets every clause read each spelling.
pub(super) fn folded(value: &Value) -> String { rendered(value).to_ascii_lowercase() }

/// Returns [`rendered_mapping`] case-folded, as [`folded`] does a value.
pub(super) fn folded_mapping(mapping: &Mapping) -> String {
    rendered_mapping(mapping).to_ascii_lowercase()
}

/// Walks one step or job mapping, as [`rendered`] does a whole value.
pub(super) fn rendered_mapping(mapping: &Mapping) -> String {
    let mut text = String::new();
    collect_mapping(mapping, &mut text);
    text
}

/// Returns whether a character can continue an identifier.
const fn is_identifier(character: char) -> bool {
    character.is_ascii_alphanumeric() || character == '_'
}

/// Returns whether `text` reaches the `secrets` context other than by name.
///
/// `secrets['CS_' + ...]`, `secrets[format('CS_{0}', 'ACCESS_TOKEN')]` and
/// `toJSON(secrets)` all hand a step the token without spelling it, so a
/// search for the name alone passes them. Every `secrets` word is therefore
/// judged by what follows it: `.` is a named reference, which the name search
/// already judges, and `:` is a YAML key (`secrets: inherit` or a named
/// forwarding), which the job clauses judge. Anything else is a computed or
/// whole-context access and is refused.
pub(super) fn computes_a_secret(text: &str) -> bool {
    text.match_indices("secrets").any(|(start, word)| {
        let (before, rest) = text.split_at(start);
        let after = rest.get(word.len()..).unwrap_or_default();
        let is_word_start = before
            .chars()
            .next_back()
            .is_none_or(|character| !(is_identifier(character) || character == '.'));
        let next = after.trim_start().chars().next();
        is_word_start && !matches!(next, Some('.' | ':') | None) && !next.is_some_and(is_identifier)
    })
}
