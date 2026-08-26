//! Wire-protocol tests for the test-server command parser and replies.

use ox_app::test_server::{Command, reply_ok};
use serde_json::json;

#[test]
fn parses_all_commands() {
    let cases = [
        (r#"{"cmd":"press","key":"W"}"#),
        (r#"{"cmd":"hold","key":"Space"}"#),
        (r#"{"cmd":"release","key":"W"}"#),
        (r#"{"cmd":"mouse","button":"left"}"#),
        (r#"{"cmd":"mouse_release","button":"left"}"#),
        (r#"{"cmd":"look","yaw":1.0,"pitch":-0.5}"#),
        (r#"{"cmd":"teleport","x":100.5,"y":60.0,"z":-40.5}"#),
        (r#"{"cmd":"tick","n":120}"#),
        (r#"{"cmd":"run","seconds":1.5}"#),
        (r#"{"cmd":"target"}"#),
        (r#"{"cmd":"state"}"#),
        (r#"{"cmd":"block","x":1,"y":2,"z":3}"#),
        (r#"{"cmd":"digest","cells":[[8,30,8],[9,33,9],[12,28,4]]}"#),
        (r#"{"cmd":"screenshot","path":"/tmp/x.png"}"#),
        (r#"{"cmd":"quit"}"#),
    ];
    for case in cases {
        assert!(serde_json::from_str::<Command>(case).is_ok(), "{case}");
    }
}

#[test]
fn fixture_array_parses_as_commands() {
    let fixture = r#"[
        {"cmd":"run","seconds":1.0},
        {"cmd":"look","yaw":0.0,"pitch":-1.5},
        {"cmd":"teleport","x":100.5,"y":60.0,"z":-40.5}
    ]"#;
    let commands: Vec<Command> = serde_json::from_str(fixture).expect("fixture array");
    assert_eq!(commands.len(), 3);
}

#[test]
fn rejects_malformed_digest_cells() {
    assert!(serde_json::from_str::<Command>(r#"{"cmd":"digest"}"#).is_err());
    assert!(serde_json::from_str::<Command>(r#"{"cmd":"digest","cells":[[8,30]]}"#).is_err());
}

#[test]
fn rejects_unknown_command() {
    assert!(serde_json::from_str::<Command>(r#"{"cmd":"explode"}"#).is_err());
}

#[test]
fn replies_are_json_objects() {
    let reply = reply_ok(json!({"pos": [1.0, 2.0, 3.0]}));
    let v: serde_json::Value = serde_json::from_str(&reply).expect("valid json");
    assert_eq!(v["ok"], serde_json::Value::Bool(true));
}
