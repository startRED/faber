//! The only information put into a problem report; the person sees it before opening the browser.
use serde::Serialize;
use std::path::Path;

const BASE: &str = "https://github.com/startRED/faber/issues/new";
const MAX_URL: usize = 2048;

#[derive(Serialize)]
pub(crate) struct Report {
    version: String,
    environment: String,
    url: String,
}

struct Details<'a> {
    version: &'a str,
    commit: &'a str,
    os: &'a str,
    os_version: &'a str,
    arch: &'a str,
    shell: &'a str,
    language: &'a str,
    crash: Option<&'a faber_engine::crash::Seen>,
}

fn safe(value: &str, max: usize) -> String {
    let out: String = value.chars().filter(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_')).take(max).collect();
    if out.is_empty() { "unknown".into() } else { out }
}

fn build(d: Details<'_>) -> Report {
    let version = safe(d.version, 64);
    let channel = if version.contains("-preview.") { "preview" } else { "stable" };
    let mut environment = format!(
        "Build commit: {}\nRelease channel: {}\nOperating system: {} {}\nCPU architecture: {}\nShell: {}\nInterface language: {}",
        safe(d.commit, 64),
        channel,
        safe(d.os, 32),
        safe(d.os_version, 64),
        safe(d.arch, 32),
        safe(d.shell, 32),
        safe(d.language, 16)
    );
    if let Some(crash) = d.crash {
        let kind = match crash.kind.as_str() {
            "panic" => "panic",
            "unclean-exit" => "unclean-exit",
            _ => "unknown",
        };
        environment.push_str(&format!("\nCrash kind: {kind}"));
        let mut parts = crash.location.rsplitn(3, ':');
        let _column = parts.next();
        let line = parts.next().and_then(|s| s.parse::<u32>().ok());
        let file = parts.next().and_then(|p| p.rsplit(['/', '\\']).next()).filter(|f| {
            f.ends_with(".rs") && f.len() <= 80 && f.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
        });
        if let (Some(file), Some(line)) = (file, line) {
            environment.push_str(&format!("\nCrash source file: {file}\nCrash line: {line}"));
        }
    }
    let mut url = url::Url::parse(BASE).expect("fixed report URL");
    url.query_pairs_mut()
        .append_pair("template", "bug_report.yml")
        .append_pair("version", &version)
        .append_pair("environment", &environment);
    let url: String = url.into();
    assert!(url.len() <= MAX_URL, "bounded report fields exceed URL limit");
    Report { version, environment, url }
}

pub(crate) fn prepare(dir: &Path, version: &str, shell: &str, language: &str, include_crash: bool) -> Report {
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_millis() as u64);
    let crash = include_crash
        .then(|| faber_engine::crash::since(&dir.join(faber_engine::crash::CRASHES), 0, now).unwrap_or_default().pop())
        .flatten();
    let os_version: String =
        sysinfo::System::os_version().unwrap_or_default().chars().filter(|c| c.is_ascii_digit() || *c == '.').collect();
    let language = match language {
        "en" | "pt-BR" | "es" | "fr" | "de" => language,
        _ => "unknown",
    };
    let arch = sysinfo::System::cpu_arch();
    let arch = match arch.as_str() {
        "x86_64" | "amd64" | "x86" | "aarch64" | "arm64" | "arm" => arch.as_str(),
        _ => "unknown",
    };
    build(Details {
        version,
        commit: env!("FABER_REPORT_COMMIT"),
        os: std::env::consts::OS,
        os_version: &os_version,
        arch,
        shell,
        language,
        crash: crash.as_ref(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn report_contains_only_bounded_allowed_fields_and_never_crash_message_or_path() {
        let crash = faber_engine::crash::Seen {
            at: 1,
            kind: "panic".into(),
            version: "secret".into(),
            commit: "secret".into(),
            thread: "secret".into(),
            message: "C:\\Users\\Alice\\token=secret".into(),
            location: "C:\\Users\\Alice\\src\\app.rs:23:5".into(),
        };
        let report = build(Details {
            version: "1.2.3-preview.1",
            commit: "abc123",
            os: "windows",
            os_version: "10.0",
            arch: "x86_64",
            shell: "Tauri",
            language: "pt-BR",
            crash: Some(&crash),
        });
        let url = url::Url::parse(&report.url).unwrap();
        let fields: Vec<_> = url.query_pairs().collect();
        assert_eq!(fields.iter().map(|(k, _)| k.as_ref()).collect::<Vec<_>>(), ["template", "version", "environment"]);
        assert!(report.url.len() <= MAX_URL && report.url.contains("%0A") && report.url.contains("%3A"));
        assert_eq!(
            report.environment,
            "Build commit: abc123\nRelease channel: preview\nOperating system: windows 10.0\nCPU architecture: x86_64\nShell: Tauri\nInterface language: pt-BR\nCrash kind: panic\nCrash source file: app.rs\nCrash line: 23"
        );
        assert!(!report.url.contains("Alice") && !report.url.contains("token") && !report.environment.contains("secret"));
        let long = "é&/".repeat(3000);
        let oversized = build(Details {
            version: &long,
            commit: &long,
            os: &long,
            os_version: &long,
            arch: &long,
            shell: &long,
            language: &long,
            crash: Some(&crash),
        });
        assert!(oversized.url.len() <= MAX_URL);
        assert_eq!(fields[1].1, report.version);
        assert_eq!(fields[2].1, report.environment);
    }
}
