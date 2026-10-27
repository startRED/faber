#![windows_subsystem = "windows"]
use gpui_kit::*;
use gpui_webview::WebView;
use std::{env, fs, time::Instant};

struct Demo { webview: Entity<WebView> }
impl Render for Demo {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().size_full().relative().bg(rgb(0x15171b)).text_color(rgb(0xffffff))
            .child(div().absolute().left(px(100.)).top(px(100.)).w(px(700.)).h(px(400.)).child(self.webview.clone()))
            .child(div().absolute().left(px(700.)).top(px(200.)).w(px(260.)).h(px(100.))
                .bg(rgb(0xe63366)).text_color(rgb(0xffffff)).p_4().child("GPUI overlay over WebView"))
    }
}
fn main() {
    std::panic::set_hook(Box::new(|info| { let _ = fs::write("out/webview-panic.txt", info.to_string()); }));
    if !on_hidden_desktop() { return; }
    unsafe {
        env::set_var("GPUI_DISABLE_DIRECT_COMPOSITION", "true");
        env::set_var("WEBVIEW2_USER_DATA_FOLDER", env::current_dir().unwrap().join("out/webview-profile"));
    }
    let args: Vec<String> = env::args().collect();
    let marker = args.windows(2).find(|pair| pair[0] == "--ready").map(|pair| pair[1].clone());
    let started = Instant::now();
    gpui_kit::application().run(move |cx| {
        gpui_kit::init(cx);
        gpui_kit::open_window(WindowOptions { window_bounds: Some(WindowBounds::Windowed(Bounds::new(point(px(100.),px(100.)),size(px(1100.),px(760.))))), focus: false, ..Default::default() }, cx, |window,cx| {
            let webview = cx.new(|cx| {
                let builder = wry::WebViewBuilder::new().with_html("<!doctype html><html><body style='margin:0;background:#66bba4;color:#102c26;font:28px sans-serif;padding:40px'><h1>Local WebView page</h1><p>WebView region</p></body></html>");
                WebView::build(builder, window, cx).expect("WebView creation failed")
            });
            if let Some(path) = marker.clone() { window.on_next_frame(move |_,_| { let _ = fs::write(path, format!("first_frame_ms={}\n",started.elapsed().as_millis())); }); }
            cx.new(|_| Demo { webview })
        }).expect("Cannot open hidden window");
    });
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





