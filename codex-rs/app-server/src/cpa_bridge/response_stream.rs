//! SSE framing for identity translation; unchanged events retain their exact bytes.
use super::identity_response::IdentityResponse;
use serde_json::Value;

pub(super) struct ResponseStream {
    identities: IdentityResponse,
    json: bool,
    pending: Vec<u8>,
    scan: usize,
    line_start: usize,
}

impl ResponseStream {
    pub(super) fn new(identities: IdentityResponse, content_type: &str) -> Self {
        Self {
            identities,
            json: content_type.to_ascii_lowercase().contains("json"),
            pending: Vec::new(),
            scan: 0,
            line_start: 0,
        }
    }

    pub(super) fn push(&mut self, bytes: &[u8]) -> Vec<u8> {
        let mut output = Vec::new();
        for part in bytes.chunks(64 * 1024) {
            self.pending.extend_from_slice(part);
            if !self.json {
                while self.scan < self.pending.len() {
                    let byte = self.pending[self.scan];
                    if !matches!(byte, b'\n' | b'\r') {
                        self.scan += 1;
                        continue;
                    }
                    if byte == b'\r' && self.scan + 1 == self.pending.len() {
                        break;
                    }
                    let end = self.scan
                        + if byte == b'\r' && self.pending.get(self.scan + 1) == Some(&b'\n') {
                            2
                        } else {
                            1
                        };
                    if self.line_start == self.scan {
                        output.extend(self.event(&self.pending[..end]));
                        self.pending.drain(..end);
                        self.scan = 0;
                        self.line_start = 0;
                    } else if self.line_start == 0
                        && data_line(&self.pending[..self.scan]).is_none()
                    {
                        // Comments (including heartbeat-only lines), event names and event IDs
                        // need no identity translation and must not wait for a data record.
                        output.extend_from_slice(&self.pending[..end]);
                        self.pending.drain(..end);
                        self.scan = 0;
                        self.line_start = 0;
                    } else {
                        self.scan = end;
                        self.line_start = end;
                    }
                }
            }
        }
        output
    }

    pub(super) fn finish(self) -> Vec<u8> {
        if self.json {
            if let Ok(mut value) = serde_json::from_slice::<Value>(&self.pending)
                && self.identities.restore(&mut value)
            {
                return value.to_string().into_bytes();
            }
            self.pending
        } else {
            self.event(&self.pending)
        }
    }

    fn event(&self, bytes: &[u8]) -> Vec<u8> {
        let lines = lines(bytes);
        let mut data = Vec::new();
        for (line, _) in &lines {
            if let Some(value) = data_line(line) {
                if !data.is_empty() {
                    data.push(b'\n');
                }
                data.extend_from_slice(value);
            }
        }
        let Ok(mut value) = serde_json::from_slice::<Value>(&data) else {
            return bytes.to_vec();
        };
        if !self.identities.restore(&mut value) {
            return bytes.to_vec();
        }
        let mut output = Vec::new();
        let mut written = false;
        for (line, ending) in lines {
            if data_line(line).is_some() {
                if written {
                    continue;
                }
                output.extend_from_slice(b"data: ");
                output.extend_from_slice(value.to_string().as_bytes());
                written = true;
            } else {
                output.extend_from_slice(line);
            }
            output.extend_from_slice(ending);
        }
        output
    }
}

fn data_line(line: &[u8]) -> Option<&[u8]> {
    if line == b"data" {
        return Some(b"");
    }
    line.strip_prefix(b"data:")
        .map(|value| value.strip_prefix(b" ").unwrap_or(value))
}

fn lines(bytes: &[u8]) -> Vec<(&[u8], &[u8])> {
    let mut result = Vec::new();
    let mut start = 0;
    let mut scan = 0;
    while scan < bytes.len() {
        if matches!(bytes[scan], b'\r' | b'\n') {
            let end = scan
                + if bytes[scan] == b'\r' && bytes.get(scan + 1) == Some(&b'\n') {
                    2
                } else {
                    1
                };
            result.push((&bytes[start..scan], &bytes[scan..end]));
            start = end;
            scan = end;
        } else {
            scan += 1;
        }
    }
    if start < bytes.len() {
        result.push((&bytes[start..], &[]));
    }
    result
}

#[cfg(test)]
#[path = "response_stream_tests.rs"]
mod tests;
