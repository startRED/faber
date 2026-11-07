//! Bounded photographs of one real hidden-app interaction. Geometry is still judged by the sentinel's frame probe.

use crate::cdp::Cdp;
use base64::Engine as _;
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Sender};
use std::thread::JoinHandle;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const STATE: &str = "(() => { const a = document.querySelector('article.ai'); return { at: performance.now(), look: document.documentElement.dataset.base, reply: { chars: a?.querySelector('.md')?.textContent.length ?? 0, complete: !!a?.querySelector('.actions') } }; })()";

pub struct Recording {
    folder: PathBuf,
    stop: Sender<()>,
    worker: JoinHandle<(Vec<Value>, Option<String>)>,
}

fn photo(cdp: &mut Cdp, folder: &Path, began: f64, phase: &str, n: usize, size: &Value) -> Result<Value, String> {
    cdp.set_deadline(Some(Instant::now() + Duration::from_secs(3)));
    let state = cdp.eval(STATE)?;
    let from = state["at"].as_f64().ok_or("no photograph clock")? - began;
    let shot = cdp.call(
        "Page.captureScreenshot",
        json!({ "format": "jpeg", "quality": 60,
        "clip": { "x": 0, "y": 0, "width": size[0], "height": size[1], "scale": 0.5 } }),
    )?;
    let end = cdp.eval(STATE)?;
    let to = end["at"].as_f64().ok_or("no photograph clock")? - began;
    let bytes =
        base64::engine::general_purpose::STANDARD.decode(shot["data"].as_str().ok_or("no photograph data")?).map_err(|e| e.to_string())?;
    if !bytes.starts_with(&[0xff, 0xd8]) || !bytes.ends_with(&[0xff, 0xd9]) {
        return Err("the photograph is not a complete JPEG".into());
    }
    let file = format!("{n:03}-{phase}.jpg");
    std::fs::write(folder.join(&file), bytes).map_err(|e| e.to_string())?;
    Ok(
        json!({ "file": file, "phase": phase, "from": from, "to": to, "reply": state["reply"], "replyAfter": end["reply"], "look": state["look"], "lookAfter": end["look"] }),
    )
}

impl Recording {
    pub fn start(mut cdp: Cdp, root: &Path, label: &str, began: f64) -> Result<Recording, String> {
        let slug: String = label.chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '-' }).collect();
        let unique = SystemTime::now().duration_since(UNIX_EPOCH).map_err(|e| e.to_string())?.as_micros();
        let folder = root.join(format!("{slug}-{unique}"));
        std::fs::create_dir_all(root).map_err(|e| e.to_string())?;
        std::fs::create_dir(&folder).map_err(|e| e.to_string())?;
        let size = cdp.eval("[innerWidth, innerHeight]")?;
        let before = photo(&mut cdp, &folder, began, "before", 0, &size)?;
        let (stop, rx) = mpsc::channel();
        let output = folder.clone();
        let worker = std::thread::spawn(move || {
            let mut frames = vec![before];
            let result = (|| {
                loop {
                    match rx.recv_timeout(Duration::from_millis(80)) {
                        Ok(()) => break,
                        Err(mpsc::RecvTimeoutError::Disconnected) => {
                            return Err("the interaction ended without closing its photograph observer".into());
                        }
                        Err(mpsc::RecvTimeoutError::Timeout) => {}
                    }
                    if frames.len() >= 95 {
                        return Err("photograph limit reached before the interaction ended".into());
                    }
                    frames.push(photo(&mut cdp, &output, began, "during", frames.len(), &size)?);
                }
                frames.push(photo(&mut cdp, &output, began, "after", frames.len(), &size)?);
                Ok(())
            })();
            (frames, result.err())
        });
        Ok(Recording { folder, stop, worker })
    }

    /// Sampling completeness is mechanical; a contact sheet always still needs a human or independent visual review.
    pub fn finish(self, label: &str, trace: &Value, problem: Option<&str>) -> Result<PathBuf, String> {
        let _ = self.stop.send(());
        let (frames, capture_error) = self.worker.join().map_err(|_| "the photograph observer panicked")?;
        let issue = problem
            .map(str::to_owned)
            .or(capture_error)
            .or_else(|| photo_problem(&frames))
            .or_else(|| (label == "reply streams").then(|| streaming_problem(&frames)).flatten());
        let status = if issue.is_some() { "incomplete" } else { "ready-for-review" };
        let report = json!({ "interaction": label, "status": status, "visualReview": "required", "problem": issue,
            "frames": frames, "trace": trace });
        std::fs::write(self.folder.join("rehearsal.json"), serde_json::to_string_pretty(&report).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
        let html = contact_sheet(label, status, issue.as_deref(), &frames);
        std::fs::write(self.folder.join("index.html"), html).map_err(|e| e.to_string())?;
        if let Some(issue) = issue {
            return Err(format!("incomplete visual rehearsal: {issue}; {}", self.folder.join("index.html").display()));
        }
        Ok(self.folder.join("index.html"))
    }
}

fn photo_problem(frames: &[Value]) -> Option<String> {
    if frames.len() < 3
        || frames[0]["phase"] != "before"
        || frames.last().is_none_or(|f| f["phase"] != "after")
        || !frames.iter().any(|f| f["phase"] == "during")
    {
        return Some("before, during and after photographs are required".into());
    }
    if frames.windows(2).any(|w| {
        let Some((a, b)) = w[0]["from"].as_f64().zip(w[1]["from"].as_f64()) else { return true };
        b <= a || b - a > 500.0
    }) || frames.iter().any(|f| {
        let Some((from, to)) = f["from"].as_f64().zip(f["to"].as_f64()) else { return true };
        !from.is_finite() || !to.is_finite() || from < 0.0 || to < from || to - from > 500.0
    }) {
        return Some("photograph timestamps have a gap or capture interval over 500 ms".into());
    }
    None
}

fn streaming_problem(frames: &[Value]) -> Option<String> {
    let final_chars = frames.last().and_then(|f| f["reply"]["chars"].as_u64()).unwrap_or(0);
    if frames.last().is_none_or(|f| ["reply", "replyAfter"].iter().any(|key| f[*key]["complete"] != json!(true)))
        || !frames.iter().any(|f| {
            f["phase"] == "during"
                && ["reply", "replyAfter"].iter().all(|key| {
                    f[*key]["complete"] == json!(false) && f[*key]["chars"].as_u64().is_some_and(|chars| chars > 0 && chars < final_chars)
                })
        })
    {
        return Some("photographs did not observe both partial streaming words and the completed reply".into());
    }
    None
}

fn escape(text: &str) -> String {
    text.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;").replace('\'', "&#39;")
}

fn contact_sheet(label: &str, status: &str, problem: Option<&str>, frames: &[Value]) -> String {
    let mut html = format!(
        "<!doctype html><html lang=\"en\"><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width\"><title>{}</title><style>body{{font:16px system-ui;background:#17171b;color:#eee;margin:24px}}main{{display:grid;grid-template-columns:repeat(auto-fit,minmax(min(100%,360px),1fr));gap:16px}}figure{{margin:0}}img{{width:100%;height:auto}}figcaption{{padding:8px 0}}a{{color:#aaf}}</style><h1>{}</h1><p>{}. Visual review required. Photographs sample the interaction; the linked trace measures every frame.</p><p>{}</p><p><a href=\"rehearsal.json\">Measurements and coverage</a></p><main>",
        escape(label),
        escape(label),
        escape(status),
        escape(problem.unwrap_or("Sampling completed; no automatic visual pass is claimed."))
    );
    for frame in frames {
        let file = escape(frame["file"].as_str().unwrap_or(""));
        html.push_str(&format!("<figure><a href=\"{file}\"><img src=\"{file}\" loading=\"lazy\" alt=\"{} photograph\"></a><figcaption>{}: {:.0}–{:.0} ms</figcaption></figure>", escape(frame["phase"].as_str().unwrap_or("unknown")), escape(frame["phase"].as_str().unwrap_or("unknown")), frame["from"].as_f64().unwrap_or(0.0), frame["to"].as_f64().unwrap_or(0.0)));
    }
    html.push_str("</main></html>");
    html
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frames() -> Vec<Value> {
        ["before", "during", "after"]
            .iter()
            .enumerate()
            .map(|(i, phase)| json!({ "file": format!("{i}.jpg"), "phase": phase, "from": i * 100, "to": i * 100 + 30 }))
            .collect()
    }

    #[test]
    fn empty_gapped_and_cut_short_photographs_never_qualify() {
        assert!(photo_problem(&frames()).is_none());
        assert!(photo_problem(&[]).is_some());
        assert!(photo_problem(&frames()[..2]).is_some());
        let mut gap = frames();
        gap[2]["from"] = json!(1000);
        assert!(photo_problem(&gap).is_some());
        let mut stalled = frames();
        stalled[1]["to"] = json!(800);
        assert!(photo_problem(&stalled).is_some());
    }

    #[test]
    fn the_contact_sheet_escapes_content_and_never_claims_a_visual_pass() {
        let html = contact_sheet("<test>&\"", "ready-for-review", None, &frames());
        assert!(html.contains("&lt;test&gt;&amp;&quot;"));
        assert!(!html.contains("<test>"));
        assert!(html.contains("Visual review required"));
        assert!(html.contains("0.jpg") && html.contains("2.jpg"));
    }

    #[test]
    fn a_final_reply_alone_is_not_streaming_evidence() {
        let mut rows = frames();
        rows[2]["reply"] = json!({ "chars": 100, "complete": true });
        rows[2]["replyAfter"] = json!({ "chars": 100, "complete": true });
        assert!(streaming_problem(&rows).is_some());
        rows[1]["reply"] = json!({ "chars": 30, "complete": false });
        rows[1]["replyAfter"] = json!({ "chars": 35, "complete": false });
        assert!(streaming_problem(&rows).is_none());
        rows[1]["reply"]["chars"] = json!(100);
        assert!(streaming_problem(&rows).is_some());
    }
}
