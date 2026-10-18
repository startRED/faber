//! What a check prints: the reading of its output (`output`) and the scan that keeps a secret out of it.
pub mod output;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SecretViolation {
    pub pattern: String,
    pub snippet: String,
}

/// Whether `content` holds something that looks like a secret (a private key, a cloud or hosting token).
pub fn scan_secrets(content: &str) -> Result<(), Vec<SecretViolation>> {
    let mut violations = vec![];

    if content.contains("-----BEGIN PRIVATE KEY-----")
        || content.contains("-----BEGIN RSA PRIVATE KEY-----")
        || content.contains("-----BEGIN OPENSSH PRIVATE KEY-----")
    {
        violations.push(SecretViolation { pattern: "private_key".into(), snippet: "private key header found".into() });
    }

    for line in content.lines() {
        if let Some(idx) = line.find("AKIA") {
            let rest = &line[idx..];
            if rest.as_bytes().get(..20).is_some_and(|s| s.iter().all(u8::is_ascii_alphanumeric)) {
                violations.push(SecretViolation { pattern: "aws_access_key".into(), snippet: "[redacted]".into() });
            }
        }
        if let Some(idx) = line.find("ghp_") {
            let rest = &line[idx..];
            if rest.len() >= 36 {
                violations.push(SecretViolation { pattern: "github_pat".into(), snippet: "[redacted]".into() });
            }
        }
        for (prefix, length) in [("ASIA", 20), ("github_pat_", 40), ("gho_", 36), ("ghs_", 36), ("sk-proj-", 32), ("glpat-", 26)] {
            if line.match_indices(prefix).any(|(at, _)| {
                line.as_bytes().get(at..at + length).is_some_and(|s| s.iter().all(|b| b.is_ascii_alphanumeric() || b"_-".contains(b)))
            }) {
                violations.push(SecretViolation { pattern: "token".into(), snippet: "[redacted]".into() });
            }
        }
    }

    if violations.is_empty() { Ok(()) } else { Err(violations) }
}
