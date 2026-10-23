//! One-shot foreground text observation; callers must use a dedicated blocking MTA worker.
use std::io;

#[derive(Debug)]
pub struct Observation {
    pub title: String,
    pub text: String,
    pub truncated: bool,
}

trait Provider {
    type Node: Clone;
    fn expired(&self) -> bool;
    fn protected(&mut self, node: &Self::Node) -> io::Result<bool>;
    fn offscreen(&mut self, node: &Self::Node) -> io::Result<bool>;
    fn first(&mut self, node: &Self::Node) -> io::Result<Option<Self::Node>>;
    fn next(&mut self, node: &Self::Node) -> io::Result<Option<Self::Node>>;
    fn text(&mut self, node: &Self::Node) -> io::Result<String>;
}

fn collect<P: Provider>(provider: &mut P, root: P::Node) -> (String, bool) {
    let mut out = String::new();
    let partial = walk(provider, root, 0, &mut 0, &mut out).unwrap_or(true);
    (out, partial)
}

use std::time::{Duration, Instant};
use windows::Win32::{
    Foundation::HWND,
    System::Com::{CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx, CoUninitialize},
    UI::{Accessibility::*, WindowsAndMessaging::*},
};

const MAX_TEXT: usize = 16_000;
const MAX_NODES: usize = 256;
const MAX_DEPTH: usize = 16;

fn append_bounded(output: &mut String, value: &str, cap: usize) -> bool {
    let room = cap.saturating_sub(output.len());
    let mut end = value.len().min(room);
    while !value.is_char_boundary(end) {
        end -= 1;
    }
    output.push_str(&value[..end]);
    end < value.len()
}

fn walk<P: Provider>(p: &mut P, node: P::Node, depth: usize, count: &mut usize, out: &mut String) -> io::Result<bool> {
    if p.expired() || *count >= MAX_NODES || depth > MAX_DEPTH || out.len() >= MAX_TEXT {
        return Ok(true);
    }
    *count += 1;
    if p.protected(&node)? || p.expired() || p.offscreen(&node)? {
        return Ok(p.expired());
    }
    if p.expired() {
        return Ok(true);
    }
    let child = p.first(&node)?;
    if let Some(mut child) = child {
        loop {
            if walk(p, child.clone(), depth + 1, count, out)? {
                return Ok(true);
            }
            if p.expired() {
                return Ok(true);
            }
            match p.next(&child)? {
                Some(next) => child = next,
                None => break,
            }
        }
    } else {
        if p.expired() {
            return Ok(true);
        }
        let text = p.text(&node)?;
        if !text.is_empty() {
            if !out.is_empty() && append_bounded(out, "\n", MAX_TEXT) {
                return Ok(true);
            }
            if append_bounded(out, &text, MAX_TEXT) {
                return Ok(true);
            }
        }
    }
    Ok(false)
}

struct Apartment;
impl Drop for Apartment {
    fn drop(&mut self) {
        unsafe { CoUninitialize() };
    }
}
fn native_error(error: windows::core::Error) -> io::Error {
    io::Error::other(error)
}
struct Native {
    walker: IUIAutomationTreeWalker,
    start: Instant,
}
impl Native {
    fn deadline(&self) -> io::Result<()> {
        if self.expired() { Err(io::Error::new(io::ErrorKind::TimedOut, "desktop observation deadline")) } else { Ok(()) }
    }
}
impl Provider for Native {
    type Node = IUIAutomationElement;
    fn expired(&self) -> bool {
        self.start.elapsed() >= Duration::from_millis(800)
    }
    fn protected(&mut self, n: &Self::Node) -> io::Result<bool> {
        self.deadline()?;
        unsafe { n.CurrentIsPassword().map(|v| v.as_bool()).map_err(native_error) }
    }
    fn offscreen(&mut self, n: &Self::Node) -> io::Result<bool> {
        self.deadline()?;
        unsafe { n.CurrentIsOffscreen().map(|v| v.as_bool()).map_err(native_error) }
    }
    fn first(&mut self, n: &Self::Node) -> io::Result<Option<Self::Node>> {
        self.deadline()?;
        unsafe {
            self.walker
                .GetFirstChildElement(n)
                .map(Some)
                .or_else(|e| if e.code() == windows::core::HRESULT(0x80004003u32 as i32) { Ok(None) } else { Err(e) })
                .map_err(native_error)
        }
    }
    fn next(&mut self, n: &Self::Node) -> io::Result<Option<Self::Node>> {
        self.deadline()?;
        unsafe {
            self.walker
                .GetNextSiblingElement(n)
                .map(Some)
                .or_else(|e| if e.code() == windows::core::HRESULT(0x80004003u32 as i32) { Ok(None) } else { Err(e) })
                .map_err(native_error)
        }
    }
    fn text(&mut self, n: &Self::Node) -> io::Result<String> {
        self.deadline()?;
        if self.protected(n)? || self.offscreen(n)? {
            return Ok(String::new());
        }
        // Only leaves reach here: container names/text may aggregate protected descendants.
        unsafe {
            if let Ok(pattern) = n.GetCurrentPatternAs::<IUIAutomationTextPattern>(UIA_TextPatternId) {
                self.deadline()?;
                let ranges = pattern.GetVisibleRanges().map_err(native_error)?;
                self.deadline()?;
                let len = ranges.Length().map_err(native_error)?;
                if len > 256 {
                    return Err(io::Error::other("too many visible text ranges"));
                }
                let mut text = String::new();
                for index in 0..len.min(256) {
                    self.deadline()?;
                    let range = ranges.GetElement(index).map_err(native_error)?;
                    self.deadline()?;
                    let value = range.GetText((MAX_TEXT + 1) as i32).map_err(native_error)?.to_string();
                    if append_bounded(&mut text, &value, MAX_TEXT + 1) {
                        break;
                    }
                }
                return Ok(text);
            }
            // A visible edit may contain scrolled-out characters; raw values cannot prove visibility.
            self.deadline()?;
            n.CurrentName().map(|v| v.to_string()).map_err(native_error)
        }
    }
}

fn foreground_valid(hwnd: HWND) -> bool {
    unsafe { !hwnd.is_invalid() && GetForegroundWindow() == hwnd && IsWindowVisible(hwnd).as_bool() && !IsIconic(hwnd).as_bool() }
}

/// Reads only the current foreground window. No handles or COM objects survive this call.
/// The caller must serialize calls on dedicated blocking workers; provider timeouts are not a hard latency guarantee.
/// Password exclusion trusts provider metadata. Aggregate containers are omitted, and a failed tree read returns partial text.
pub fn read_foreground() -> io::Result<Observation> {
    read_foreground_inner(None)
}

/// Synthetic hidden-desktop proof only. The caller must retain the fixture Child handle until this returns.
/// A mismatched sampled foreground PID is refused before COM, UI Automation or window-title access.
#[cfg(feature = "test-hooks")]
pub fn read_foreground_for_test(expected_pid: u32) -> io::Result<Observation> {
    read_foreground_inner(Some(expected_pid))
}

fn matches_expected_pid(actual: u32, expected: Option<u32>) -> bool {
    expected.is_none_or(|pid| pid != 0 && pid == actual)
}

fn foreground_matches(hwnd: HWND, expected_pid: Option<u32>) -> bool {
    if !foreground_valid(hwnd) {
        return false;
    }
    // Shipping calls keep the existing foreground checks and do not add a PID query.
    if expected_pid.is_none() {
        return true;
    }
    let mut pid = 0;
    unsafe {
        GetWindowThreadProcessId(hwnd, Some(&mut pid));
    }
    matches_expected_pid(pid, expected_pid)
}

#[derive(Clone, Copy)]
enum WindowCheck {
    Foreground(Option<u32>),
    #[cfg(feature = "test-hooks")]
    Synthetic(u32),
}
impl WindowCheck {
    fn matches(self, hwnd: HWND) -> bool {
        match self {
            Self::Foreground(expected_pid) => foreground_matches(hwnd, expected_pid),
            #[cfg(feature = "test-hooks")]
            Self::Synthetic(expected_pid) => synthetic_window_matches(hwnd, expected_pid),
        }
    }
    fn changed_error(self) -> &'static str {
        match self {
            Self::Foreground(_) => "foreground window changed",
            #[cfg(feature = "test-hooks")]
            Self::Synthetic(_) => "synthetic fixture window changed",
        }
    }
}

/// Separate provider-only proof, never a foreground proof or an engine tool.
/// The caller must retain the fixed synthetic fixture Child handle throughout this read.
#[cfg(feature = "test-hooks")]
pub fn read_synthetic_window_for_test(window: usize, expected_pid: u32) -> io::Result<Observation> {
    let hwnd = HWND(window as *mut std::ffi::c_void);
    if !synthetic_window_matches(hwnd, expected_pid) {
        return Err(io::Error::other("synthetic fixture window unavailable"));
    }
    observe_window(hwnd, WindowCheck::Synthetic(expected_pid))
}

#[cfg(feature = "test-hooks")]
use synthetic::matches as synthetic_window_matches;

#[cfg(feature = "test-hooks")]
mod synthetic {
    use super::HWND;
    use windows::Win32::{
        Foundation::HANDLE,
        System::{JobObjects::*, StationsAndDesktops::*, Threading::GetCurrentThreadId},
        UI::WindowsAndMessaging::{GetWindowThreadProcessId, IsIconic, IsWindowVisible},
    };

    struct InputDesktop(HDESK);
    impl Drop for InputDesktop {
        fn drop(&mut self) {
            let _ = unsafe { CloseDesktop(self.0) };
        }
    }
    fn name(desktop: HDESK) -> Option<String> {
        let mut text = [0u16; 256];
        unsafe {
            GetUserObjectInformationW(
                HANDLE(desktop.0),
                UOI_NAME,
                Some(text.as_mut_ptr().cast()),
                std::mem::size_of_val(&text) as u32,
                None,
            )
            .ok()?;
        }
        let end = text.iter().position(|v| *v == 0)?;
        String::from_utf16(&text[..end]).ok()
    }
    fn desktops_match(target_thread: u32) -> Option<bool> {
        let own = name(unsafe { GetThreadDesktop(GetCurrentThreadId()) }.ok()?)?;
        if !own.starts_with("faber-test-") || own.len() > 128 || !own.bytes().all(|v| v.is_ascii_alphanumeric() || v == b'-') {
            return Some(false);
        }
        let target = name(unsafe { GetThreadDesktop(target_thread) }.ok()?)?;
        if !own.eq_ignore_ascii_case(&target) {
            return Some(false);
        }
        let input = InputDesktop(unsafe { OpenInputDesktop(DESKTOP_CONTROL_FLAGS(0), false, DESKTOP_READOBJECTS) }.ok()?);
        Some(!own.eq_ignore_ascii_case(&name(input.0)?))
    }
    fn owned_job_contains(expected_pid: u32) -> bool {
        // The native variable-length record is two u32 counts followed by pointer-sized process ids.
        #[repr(C)]
        struct Members {
            assigned: u32,
            count: u32,
            ids: [usize; 64],
        }
        let mut members = Members { assigned: 0, count: 0, ids: [0; 64] };
        let mut limits = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
        // NULL queries only this process's immediate job. Oversized/failed lists are refused, never truncated.
        unsafe {
            if QueryInformationJobObject(
                None,
                JobObjectExtendedLimitInformation,
                (&mut limits as *mut JOBOBJECT_EXTENDED_LIMIT_INFORMATION).cast(),
                std::mem::size_of_val(&limits) as u32,
                None,
            )
            .is_err()
                || !limits.BasicLimitInformation.LimitFlags.contains(JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE)
                || QueryInformationJobObject(
                    None,
                    JobObjectBasicProcessIdList,
                    (&mut members as *mut Members).cast(),
                    std::mem::size_of_val(&members) as u32,
                    None,
                )
                .is_err()
            {
                return false;
            }
        }
        if members.count > 64 || members.assigned != members.count {
            return false;
        }
        let ids = &members.ids[..members.count as usize];
        ids.contains(&(expected_pid as usize)) && ids.contains(&(std::process::id() as usize))
    }
    pub(super) fn matches(hwnd: HWND, expected_pid: u32) -> bool {
        let mut actual = 0;
        let thread = unsafe { GetWindowThreadProcessId(hwnd, Some(&mut actual)) };
        // Refuse any non-fixture PID before querying visibility, desktops, job membership or content.
        if !super::matches_expected_pid(actual, Some(expected_pid)) || actual == std::process::id() || thread == 0 {
            return false;
        }
        unsafe {
            if !IsWindowVisible(hwnd).as_bool() || IsIconic(hwnd).as_bool() {
                return false;
            }
        }
        desktops_match(thread) == Some(true) && owned_job_contains(expected_pid)
    }
}

fn read_foreground_inner(expected_pid: Option<u32>) -> io::Result<Observation> {
    unsafe {
        let hwnd = GetForegroundWindow();
        let mut pid = 0;
        GetWindowThreadProcessId(hwnd, Some(&mut pid));
        if !foreground_valid(hwnd) || pid == std::process::id() || pid == 0 || !matches_expected_pid(pid, expected_pid) {
            return Err(io::Error::other("foreground window unavailable"));
        }
        observe_window(hwnd, WindowCheck::Foreground(expected_pid))
    }
}

fn observe_window(hwnd: HWND, check: WindowCheck) -> io::Result<Observation> {
    unsafe {
        CoInitializeEx(None, COINIT_MULTITHREADED).ok().map_err(native_error)?;
        let _apartment = Apartment;
        let start = Instant::now();
        let automation: IUIAutomation2 = CoCreateInstance(&CUIAutomation8, None, CLSCTX_INPROC_SERVER).map_err(native_error)?;
        automation.SetConnectionTimeout(200).map_err(native_error)?;
        automation.SetTransactionTimeout(200).map_err(native_error)?;
        let root = automation.ElementFromHandle(hwnd).map_err(native_error)?;
        let mut provider = Native { walker: automation.RawViewWalker().map_err(native_error)?, start };
        if !check.matches(hwnd) {
            return Err(io::Error::other(check.changed_error()));
        }
        let mut title_buffer = [0u16; 257];
        let len = GetWindowTextW(hwnd, &mut title_buffer);
        let mut title = String::new();
        let title_partial = append_bounded(&mut title, &String::from_utf16_lossy(&title_buffer[..len.max(0) as usize]), 256);
        let (text, truncated) = collect(&mut provider, root);
        if !check.matches(hwnd) {
            return Err(io::Error::other(check.changed_error()));
        }
        Ok(Observation { title, text, truncated: truncated || title_partial || len == 256 })
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn native_desktop_uia_expected_pid_policy_refuses_zero_and_mismatch() {
        for actual in [0, 1, 42, u32::MAX] {
            assert!(matches_expected_pid(actual, None), "production has no expected-PID restriction");
            assert!(!matches_expected_pid(actual, Some(0)), "zero cannot identify an owned fixture");
        }
        assert!(matches_expected_pid(42, Some(42)));
        assert!(!matches_expected_pid(41, Some(42)));
        assert!(!matches_expected_pid(0, Some(42)));
    }
    #[test]
    fn native_desktop_uia_test_seam_checks_expected_pid_before_observation() {
        let source = include_str!("desktop.rs");
        let production = source.split("#[cfg(test)]").next().unwrap();
        let guard = production.find("!matches_expected_pid(pid, expected_pid)");
        let com = production.find("CoInitializeEx(None").unwrap();
        assert!(guard.is_some_and(|at| at < com), "the sampled HWND's expected PID must be checked before any COM, UIA or title read");
        assert_eq!(
            production.matches("!check.matches(hwnd)").count(),
            2,
            "the selected scope is rechecked before title and after collection"
        );
        assert!(production.contains("read_foreground_inner(None)"), "shipping entry point keeps its unrestricted foreground semantics");
        assert!(
            production.contains("#[cfg(feature = \"test-hooks\")]\npub fn read_foreground_for_test"),
            "the fixture seam must be absent from shipping builds"
        );
    }
    #[test]
    fn native_desktop_uia_never_reads_whole_edit_values() {
        let source = include_str!("desktop.rs");
        let production = source.split("#[cfg(test)]").next().unwrap();
        assert!(
            !production.contains(".CurrentValue("),
            "a visible Edit can contain offscreen characters; whole values have no visibility proof"
        );
    }
    use super::*;
    struct Fake {
        reads: Vec<usize>,
    }
    impl Provider for Fake {
        type Node = usize;
        fn expired(&self) -> bool {
            false
        }
        fn protected(&mut self, n: &usize) -> io::Result<bool> {
            Ok(*n == 1)
        }
        fn offscreen(&mut self, n: &usize) -> io::Result<bool> {
            Ok(*n == 3)
        }
        fn first(&mut self, n: &usize) -> io::Result<Option<usize>> {
            assert!(*n != 1 && *n != 3, "protected and offscreen subtrees must not be traversed");
            Ok(if *n == 0 { Some(1) } else { None })
        }
        fn next(&mut self, n: &usize) -> io::Result<Option<usize>> {
            Ok(if *n < 4 { Some(n + 1) } else { None })
        }
        fn text(&mut self, n: &usize) -> io::Result<String> {
            self.reads.push(*n);
            Ok(format!("text{n}"))
        }
    }
    struct Budget {
        calls: usize,
        deadline: bool,
        chain: bool,
        fail: bool,
    }
    impl Provider for Budget {
        type Node = usize;
        fn expired(&self) -> bool {
            self.deadline
        }
        fn protected(&mut self, _: &usize) -> io::Result<bool> {
            self.calls += 1;
            if self.fail { Err(io::Error::other("provider failed")) } else { Ok(false) }
        }
        fn offscreen(&mut self, _: &usize) -> io::Result<bool> {
            Ok(false)
        }
        fn first(&mut self, n: &usize) -> io::Result<Option<usize>> {
            Ok(if self.chain || *n == 0 { Some(n + 1) } else { None })
        }
        fn next(&mut self, n: &usize) -> io::Result<Option<usize>> {
            Ok(if self.chain { None } else { Some(n + 1) })
        }
        fn text(&mut self, _: &usize) -> io::Result<String> {
            Ok(String::new())
        }
    }
    #[test]
    fn native_desktop_uia_bounds_nodes_depth_deadline_and_errors() {
        for chain in [false, true] {
            let mut p = Budget { calls: 0, deadline: false, chain, fail: false };
            assert!(collect(&mut p, 0).1);
            assert_eq!(p.calls, if chain { MAX_DEPTH + 1 } else { MAX_NODES });
        }
        let mut p = Budget { calls: 0, deadline: true, chain: false, fail: false };
        assert!(collect(&mut p, 0).1);
        assert_eq!(p.calls, 0);
        p.deadline = false;
        p.fail = true;
        assert!(collect(&mut p, 0).1);
        assert_eq!(p.calls, 1);
    }
    #[test]
    fn native_desktop_uia_utf8_output_never_splits_characters() {
        let mut out = "a".repeat(MAX_TEXT - 1);
        assert!(append_bounded(&mut out, "é", MAX_TEXT));
        assert_eq!(out.len(), MAX_TEXT - 1);
        assert!(!append_bounded(&mut out, "x", MAX_TEXT));
        assert_eq!(out.len(), MAX_TEXT);
        assert!(append_bounded(&mut out, "z", MAX_TEXT));
    }
    #[test]
    fn native_desktop_uia_omits_password_offscreen_and_aggregate() {
        let mut p = Fake { reads: vec![] };
        let (text, partial) = collect(&mut p, 0);
        assert_eq!(text, "text2\ntext4");
        assert_eq!(p.reads, vec![2, 4]);
        assert!(!partial);
    }
}
