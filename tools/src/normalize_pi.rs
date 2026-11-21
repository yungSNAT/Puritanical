// SPDX-License-Identifier: MIT

//! Converts Pi's raw JSONL event stream to the benchmark cell schema that
//! `aggregate` reads.

use crate::Out;
use crate::js::{self, json_num};
use serde_json::{Value, json};
use std::path::Path;

/// Prints the cell for one `events.jsonl`.
#[must_use]
pub fn run(args: &[String]) -> Out {
    let Some(file) = args.first() else {
        return Out {
            stderr: "usage: puritanical-tools normalize-pi <events.jsonl>\n".into(),
            code: 2,
            ..Out::default()
        };
    };
    let text = match js::read(Path::new(file)) {
        Ok(text) => text,
        Err(e) => return Out::fail(format!("{file}: {e}")),
    };
    let (mut output_tokens, mut result, mut is_error) = (0.0, String::new(), false);
    for message in text.split('\n').filter_map(assistant_end) {
        let output = message
            .get("usage")
            .and_then(|u| u.get("output"))
            .and_then(Value::as_f64);
        output_tokens += output.filter(|x| x.is_finite()).unwrap_or(0.0);
        result = text_of(message.get("content"));
        is_error |= message.get("stopReason").and_then(Value::as_str) == Some("error");
    }
    let cell = json!({
        "harness": "pi",
        "is_error": is_error || result.is_empty(),
        "result": result,
        "usage": {"output_tokens": json_num(output_tokens)},
    });
    Out {
        stdout: js::stringify(&cell, 2) + "\n",
        ..Out::default()
    }
}

/// The message of an assistant `message_end` event; any other line is skipped.
fn assistant_end(line: &str) -> Option<Value> {
    let mut event = serde_json::from_str::<Value>(line).ok()?;
    let ended = event.get("type").and_then(Value::as_str) == Some("message_end");
    let message = event.get_mut("message").filter(|m| js::truthy(Some(m)))?;
    (ended && message.get("role").and_then(Value::as_str) == Some("assistant"))
        .then(|| message.take())
}

/// A message's text: its text blocks joined, or the content itself.
fn text_of(content: Option<&Value>) -> String {
    match content {
        Some(Value::Array(blocks)) => blocks
            .iter()
            .filter(|b| b.get("type").and_then(Value::as_str) == Some("text"))
            .map(|b| {
                if js::truthy(b.get("text")) {
                    js::string(b.get("text"))
                } else {
                    String::new()
                }
            })
            .collect::<Vec<_>>()
            .join("\n"),
        c if js::truthy(c) => js::string(c),
        _ => String::new(),
    }
}
