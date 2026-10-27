#![windows_subsystem = "windows"]
use std::{cell::RefCell, env, fs, path::PathBuf, rc::Rc, time::{Duration, Instant}};
use faber_engine::model::Conversation;
use gpui::{prelude::*, div, list, px, rgb, App, Context, Entity, ListAlignment, ListState, Window, WindowOptions};
use gpui_component::{input::{Textarea, TextareaState}, text::TextView};

struct Chat {
    messages: Rc<RefCell<Vec<String>>>,
    list: ListState,
    input: Entity<TextareaState>,
}
impl Render for Chat {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let messages = self.messages.clone();
        div().size_full().flex().bg(rgb(0x15171b)).text_color(rgb(0xe8e8e8))
            .child(div().w(px(220.)).h_full().p_4().bg(rgb(0x20232a)).child("Faber\n\nConversations"))
            .child(div().flex_1().h_full().flex().flex_col().p_4()
                .child(list(self.list.clone(), move |ix, _, _| {
                    div().w_full().p_4().child(TextView::markdown(ix, messages.borrow()[ix].clone())).into_any_element()
                }).flex_1())
                .child(Textarea::new(&self.input).h(px(85.))))
    }
}
fn stream_text(path: &str) -> Result<String, String> {
    let content = fs::read_to_string(path).map_err(|e| e.to_string())?;
    let text = content.lines().filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
        .filter_map(|line| line.pointer("/msg/event/delta/text").and_then(|value| value.as_str()).map(str::to_owned))
        .collect::<String>();
    if text.trim().is_empty() { return Err("Fixture has no streamed assistant text".into()); }
    let mut result = String::from("**Streamed reply**\n\n");
    while result.split_whitespace().count() < 4000 { result.push_str(&text); result.push_str("\n\n"); }
    result.push_str("- one\n- two\n\n```js\nconsole.log('stream');\n```\n");
    Ok(result)
}
fn main() {
    let start = Instant::now();
    if !on_hidden_desktop() { eprintln!("Refusing to open a window outside the hidden test desktop"); return; }
    let mut args = env::args().skip(1);
    let Some(path) = args.next() else { eprintln!("Conversation file required"); return };
    let mut ready: Option<PathBuf> = None;
    let mut top = false;
    let mut stream = None;
    while let Some(arg) = args.next() {
        match arg.as_str() { "--ready" => ready = args.next().map(PathBuf::from), "--scroll-top" => top = true, "--stream" => stream = args.next(), "--webview" => { eprintln!("WebView is unavailable in gpui-component 0.8.1"); return; }, _ => { eprintln!("Unknown argument: {arg}"); return; } }
    }
    let conversation: Conversation = match fs::read(&path).map_err(|e| e.to_string()).and_then(|data| serde_json::from_slice(&data).map_err(|e| e.to_string())) {
        Ok(c) => c, Err(e) => { eprintln!("Cannot read conversation: {e}"); return }
    };
    let stream = match stream.map(|path| stream_text(&path)).transpose() { Ok(value) => value, Err(e) => { eprintln!("{e}"); return } };
    let mut messages = conversation.messages.into_iter().map(|m| m.text).collect::<Vec<_>>();
    if stream.is_some() { messages.push(String::new()); }
    let messages = Rc::new(RefCell::new(messages));
    gpui_kit::application().run(move |cx: &mut App| {
        gpui_kit::init(cx);
        gpui_component::Theme::change(gpui_component::ThemeMode::Dark, None, cx);
        gpui_kit::open_window(WindowOptions { window_bounds: Some(gpui::WindowBounds::Windowed(gpui::Bounds::new(gpui::point(px(100.), px(100.)), gpui::size(px(1100.), px(760.))))), focus: false, ..Default::default() }, cx, |window, cx| {
            let list_state = ListState::new(messages.borrow().len(), ListAlignment::Bottom, px(300.));
            if let Some(path) = ready.clone() {
                let state = list_state.clone();
                window.on_next_frame(move |window, _| {
                    let _ = fs::write(&path, format!("first_frame_ms={}\n", start.elapsed().as_millis()));
                    if top {
                        state.scroll_to(gpui::ListOffset { item_ix: 0, offset_in_item: px(0.) });
                        window.on_next_frame(move |_, _| append_marker(&path, "top_frame_ms", start));
                        window.refresh();
                    }
                });
            }
            let view = cx.new(|cx| Chat { messages, list: list_state, input: cx.new(|cx| TextareaState::new(window, cx).auto_grow(1, 6)) });
            if let Some(text) = stream {
                let chars: Vec<char> = text.chars().collect();
                view.update(cx, |_, cx| {
                    cx.spawn(async move |view, cx| {
                        for tick in 1..=600 {
                            cx.background_executor().timer(Duration::from_millis(33)).await;
                            let end = chars.len() * tick / 600;
                            let text: String = chars[..end].iter().collect();
                            if view.update(cx, |chat, cx| {
                                let mut messages = chat.messages.borrow_mut();
                                let index = messages.len() - 1;
                                messages[index] = text;
                                chat.list.remeasure_items(index..index + 1);
                                chat.list.scroll_to_end();
                                cx.notify();
                            }).is_err() { return; }
                        }
                        if let Some(path) = ready { append_marker(&path, "stream_done_ms", start); }
                    }).detach();
                });
            }
            view
        }).expect("Cannot open hidden desktop window");
    });
}
fn append_marker(path: &PathBuf, name: &str, start: Instant) {
    use std::io::Write;
    if let Ok(mut file) = fs::OpenOptions::new().append(true).open(path) { let _ = writeln!(file, "{name}={}", start.elapsed().as_millis()); }
}


fn on_hidden_desktop() -> bool {
    use windows::Win32::{Foundation::HANDLE, System::{StationsAndDesktops::{GetThreadDesktop, GetUserObjectInformationW, UOI_NAME}, Threading::GetCurrentThreadId}};
    unsafe {
        let Ok(desktop) = GetThreadDesktop(GetCurrentThreadId()) else { return false };
        let mut name = [0u16; 256];
        if GetUserObjectInformationW(HANDLE(desktop.0), UOI_NAME, Some(name.as_mut_ptr().cast()), std::mem::size_of_val(&name) as u32, None).is_err() { return false; }
        let length = name.iter().position(|&c| c == 0).unwrap_or(name.len());
        String::from_utf16_lossy(&name[..length]).starts_with("faber-test-")
    }
}



