// SPDX-License-Identifier: MIT

//! normalize-pi on small hand-made Pi event logs.

mod support;

use serde_json::{Value, json};
use std::path::Path;
use support::{cmd, ok, write};

fn utf8(p: &Path) -> &str {
    p.to_str().expect("UTF-8 path")
}

#[test]
fn normalize_pi_builds_one_cell() {
    let tmp = tempfile::tempdir().expect("temp dir");
    let events = tmp.path().join("events.jsonl");
    let lines = [
        json!({"type": "session", "version": 3}).to_string(),
        json!({"type": "message_end", "message": {"role": "assistant", "content": [{"type": "text", "text": "answer"}, {"type": "tool", "text": "x"}, {"type": "text", "text": 5}, {"type": "text"}], "usage": {"output": 42}, "stopReason": "stop"}}).to_string(),
        "not json".into(),
        json!({"type": "message_end", "message": {"role": "user", "content": "ignored"}}).to_string(),
        json!({"type": "message_end", "message": null}).to_string(),
        json!({"type": "message_end", "message": {"role": "assistant", "content": {"a": 1}, "usage": {"output": "9"}}}).to_string(),
        String::new(),
        json!({"type": "message_end", "message": {"role": "assistant", "content": "final", "usage": {"output": 8}}}).to_string(),
    ];
    write(&events, &lines.join("\n"));
    let out = cmd(tmp.path(), &[], &["normalize-pi", utf8(&events)]);
    assert_eq!(
        ok(&out),
        "{\n  \"harness\": \"pi\",\n  \"is_error\": false,\n  \"result\": \"final\",\n  \"usage\": {\n    \"output_tokens\": 50\n  }\n}\n"
    );
}

#[test]
fn normalize_pi_marks_errors_and_usage() {
    let tmp = tempfile::tempdir().expect("temp dir");
    let events = tmp.path().join("events.jsonl");
    let path = utf8(&events);
    let normalised = || -> Value {
        serde_json::from_str(ok(&cmd(tmp.path(), &[], &["normalize-pi", path]))).expect("JSON")
    };
    write(&events, &json!({"type": "message_end", "message": {"role": "assistant", "content": [], "stopReason": "error"}}).to_string());
    assert_eq!(
        normalised(),
        json!({"harness": "pi", "is_error": true, "result": "", "usage": {"output_tokens": 0}})
    );
    write(&events, &json!({"type": "message_end", "message": {"role": "assistant", "content": null, "stopReason": "stop"}}).to_string());
    assert_eq!(
        normalised()["is_error"],
        json!(true),
        "an empty answer is an error"
    );

    let bs = char::from(0x5C_u8);
    let event = |content: &str| {
        format!(
            r#"{{"type":"message_end","message":{{"role":"assistant","content":"{content}","usage":{{"output":3}}}}}}"#
        )
    };
    write(&events, &event(&format!("a{bs}ud800")));
    let lone = ok(&cmd(tmp.path(), &[], &["normalize-pi", path])).to_owned();
    assert!(
        lone.contains(&format!(r#""result": "a{bs}ud800""#))
            && lone.contains(r#""output_tokens": 3"#),
        "an escaped lone surrogate is kept, as JSON.parse keeps it: {lone}"
    );
    for reserved in [
        char::from_u32(0x10_F800).expect("plane 16").to_string(),
        format!("{bs}udbfe{bs}udc00"),
    ] {
        write(&events, &event(&reserved));
        assert_eq!(
            cmd(tmp.path(), &[], &["normalize-pi", path]).code,
            1,
            "a stand-in code point is refused, raw or escaped"
        );
    }

    let usage = cmd(tmp.path(), &[], &["normalize-pi"]);
    assert_eq!(
        (usage.code, usage.stderr.as_str()),
        (2, "usage: puritanical-tools normalize-pi <events.jsonl>\n")
    );
    assert_eq!(
        cmd(tmp.path(), &[], &["normalize-pi", "missing.jsonl"]).code,
        1
    );
}
