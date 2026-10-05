//! Current runtime time context for CPA requests; history and caller paths stay intact.
use chrono::Local;
use serde_json::Map;
use serde_json::Value;
use serde_json::json;

pub(super) struct TimeContext {
    pub(super) date: String,
    pub(super) timezone: String,
}

impl TimeContext {
    pub(super) fn now() -> Self {
        let now = Local::now();
        let utc_offset = now.format("%:z").to_string();
        #[cfg(unix)]
        let configured = std::env::var("TZ").ok().filter(|value| !value.is_empty());
        // Windows local time follows OS settings, not the POSIX TZ variable.
        #[cfg(not(unix))]
        let configured: Option<String> = None;
        let candidate = configured
            .clone()
            .or_else(|| iana_time_zone::get_timezone().ok());
        let timezone = candidate
            .map(|value| value.strip_prefix(':').unwrap_or(&value).to_owned())
            .filter(|value| {
                !value.is_empty()
                    && value.len() <= 128
                    && value
                        .split('/')
                        .all(|part| !matches!(part, "" | "." | ".."))
                    && value
                        .bytes()
                        .all(|byte| byte.is_ascii_alphanumeric() || b"/_+-".contains(&byte))
            })
            .filter(|value| {
                #[cfg(unix)]
                {
                    let zone =
                        std::fs::read(std::path::Path::new("/usr/share/zoneinfo").join(value));
                    if configured.is_some() {
                        zone.is_ok()
                    } else {
                        // /etc/timezone can be stale when only /etc/localtime is mounted.
                        zone.ok()
                            .zip(std::fs::read("/etc/localtime").ok())
                            .is_some_and(|(zone, local)| zone == local)
                    }
                }
                #[cfg(not(unix))]
                {
                    let _ = value;
                    true
                }
            })
            // Preserve the actual system offset when its IANA name is unavailable.
            .unwrap_or_else(|| format!("UTC{utc_offset}"));
        Self {
            date: now.format("%Y-%m-%d").to_string(),
            timezone,
        }
    }

    pub(super) fn apply(&self, input: &mut [Value]) -> bool {
        let current_start = input
            .iter()
            .rposition(|item| item["role"] == "assistant" || item["type"] != "message")
            .map_or(0, |index| index + 1);
        let mut found = false;
        for item in &mut input[current_start..] {
            if !matches!(item["role"].as_str(), Some("user" | "developer" | "system")) {
                continue;
            }
            if let Some(content) = item["content"].as_array_mut() {
                for part in content {
                    if part["type"] == "input_text"
                        && let Some(text) = part["text"].as_str()
                        && let Some(updated) = self.rewrite_block(text)
                    {
                        part["text"] = Value::String(updated);
                        found = true;
                    }
                }
            }
        }
        found
    }

    fn rewrite_block(&self, text: &str) -> Option<String> {
        let trimmed = text.trim();
        let tag = ["environment_context", "codex_apps_client_time_context"]
            .into_iter()
            .find(|tag| {
                trimmed.starts_with(&format!("<{tag}>")) && trimmed.ends_with(&format!("</{tag}>"))
            })?;
        let opening = format!("<{tag}>");
        let closing = format!("</{tag}>");
        let inner = trimmed.strip_prefix(&opening)?.strip_suffix(&closing)?;
        // Only standalone context blocks are owned by this adapter. Never rewrite quoted
        // examples, code fences, or an arbitrary user prompt containing matching text.
        if inner.contains("```") || inner.contains("~~~") || inner.contains(&opening) {
            return None;
        }
        let mut inner = inner.to_owned();
        for (field, value) in [("current_date", &self.date), ("timezone", &self.timezone)] {
            let open = format!("<{field}>");
            let close = format!("</{field}>");
            if let Some(start) = inner.find(&open) {
                let end = start + open.len() + inner[start + open.len()..].find(&close)?;
                // Do not change nested paths, descriptions or repeated ambiguous declarations.
                if inner[end + close.len()..].contains(&open) {
                    return None;
                }
                inner.replace_range(start + open.len()..end, value);
            } else {
                inner.push_str(&format!("\n  <{field}>{value}</{field}>\n"));
            }
        }
        Some(format!("{opening}{inner}{closing}"))
    }

    pub(super) fn message(&self) -> Value {
        json!({"type":"message", "role":"user", "content":[{"type":"input_text", "text":format!(
            "<environment_context>\n  <current_date>{}</current_date>\n  <timezone>{}</timezone>\n</environment_context>",
            self.date, self.timezone
        )}]})
    }

    pub(super) fn metadata(&self, body: &mut Value, request: &Map<String, Value>) {
        // Keep the flat fields consistent when a caller supplies them, without adding
        // non-Responses top-level timezone/date parameters to the upstream API.
        for (key, value) in [("timezone", &self.timezone), ("current_date", &self.date)] {
            if request
                .get("client_metadata")
                .and_then(|metadata| metadata.get(key))
                .is_some()
            {
                body["client_metadata"][key] = json!(value);
            }
        }
    }
}

#[cfg(test)]
#[path = "inference_context_tests.rs"]
mod tests;
