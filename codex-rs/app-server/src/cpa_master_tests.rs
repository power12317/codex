use super::*;
use pretty_assertions::assert_eq;

#[test]
fn scan_uses_relative_credential_ids_and_effective_flags() -> anyhow::Result<()> {
    let directory = tempfile::TempDir::new()?;
    let root = directory.path();
    std::fs::create_dir(root.join("nested"))?;
    for (name, value) in [
        (
            "Original Account.JSON",
            json!({"type":" Codex ","codex_cli":{"enabled":true}}),
        ),
        (
            "nested/account.json",
            json!({"type":"codex","codex_cli":{"enabled":true},"access_token":"updated"}),
        ),
        (
            "off.json",
            json!({"type":"codex","codex_cli":{"enabled":false}}),
        ),
        (
            "disabled.json",
            json!({"type":"codex","disabled":true,"codex_cli":{"enabled":true}}),
        ),
        (
            "other.json",
            json!({"type":"other","codex_cli":{"enabled":true}}),
        ),
    ] {
        std::fs::write(root.join(name), serde_json::to_vec(&value)?)?;
    }
    assert_eq!(
        scan(root)?,
        HashMap::from([
            (
                "Original Account.JSON".into(),
                root.join("Original Account.JSON")
            ),
            (
                "nested/account.json".into(),
                root.join("nested/account.json")
            ),
        ])
    );
    Ok(())
}
