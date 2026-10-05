use super::*;
use pretty_assertions::assert_eq;

#[test]
fn rewrites_only_current_context_and_preserves_paths_and_history() {
    let time = TimeContext {
        date: "2026-10-05".into(),
        timezone: "Etc/UTC".into(),
    };
    let historical = json!({"type":"message", "role":"user", "content":[{"type":"input_text", "text":"<environment_context><current_date>2025-01-01</current_date><timezone>Asia/Tokyo</timezone></environment_context>"}]});
    let mut input = vec![
        historical.clone(),
        json!({"role":"assistant", "content":"past answer"}),
        json!({"type":"message", "role":"user", "content":[{"type":"input_text", "text":"<environment_context>\n<cwd>C:\\work</cwd>\n<os>Windows</os>\n<current_date>2026-10-04</current_date>\n<timezone>Asia/Singapore</timezone>\n</environment_context>"}]}),
    ];
    assert!(time.apply(&mut input));
    assert_eq!(input[0], historical);
    assert_eq!(
        input[2]["content"][0]["text"],
        json!(
            "<environment_context>\n<cwd>C:\\work</cwd>\n<os>Windows</os>\n<current_date>2026-10-05</current_date>\n<timezone>Etc/UTC</timezone>\n</environment_context>"
        )
    );
}

#[test]
fn quoted_context_is_unchanged_and_missing_fields_are_added() {
    let time = TimeContext {
        date: "2026-10-05".into(),
        timezone: "Asia/Singapore".into(),
    };
    assert_eq!(
        time.rewrite_block(
            "Explain this: <environment_context><timezone>UTC</timezone></environment_context>"
        ),
        None
    );
    assert_eq!(
        time.rewrite_block(
            "<environment_context>```\n<timezone>UTC</timezone>\n```</environment_context>"
        ),
        None
    );
    assert_eq!(time.rewrite_block("<environment_context><cwd>/app</cwd></environment_context>"), Some("<environment_context><cwd>/app</cwd>\n  <current_date>2026-10-05</current_date>\n\n  <timezone>Asia/Singapore</timezone>\n</environment_context>".into()));
}

#[test]
fn tool_continuation_does_not_rewrite_prior_user_context() {
    let time = TimeContext {
        date: "2026-10-05".into(),
        timezone: "Etc/UTC".into(),
    };
    let mut input = vec![
        json!({"type":"message", "role":"user", "content":[{"type":"input_text", "text":"<environment_context><current_date>2020-01-01</current_date><timezone>old-zone</timezone></environment_context>"}]}),
        json!({"type":"function_call", "call_id":"call", "name":"run", "arguments":"{}"}),
        json!({"type":"function_call_output", "call_id":"call", "output":"result"}),
    ];
    let history = input.clone();
    assert!(!time.apply(&mut input));
    assert_eq!(input, history);
}
