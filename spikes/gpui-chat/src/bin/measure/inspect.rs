use std::{fs::File, io::BufWriter, path::Path};
use windows::Win32::{
    Foundation::{HWND, RECT},
    Graphics::Gdi::{BITMAPINFO, BITMAPINFOHEADER, BI_RGB, CreateCompatibleDC, CreateDIBSection, DIB_RGB_COLORS, DeleteDC, DeleteObject, HGDIOBJ, SelectObject},
    Storage::Xps::{PrintWindow, PRINT_WINDOW_FLAGS},
    System::Com::{CoCreateInstance, CoInitializeEx, CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED},
    UI::{Accessibility::{CUIAutomation, IUIAutomation, IUIAutomationElement, IUIAutomationTreeWalker, IUIAutomationTextPattern, IUIAutomationValuePattern, UIA_TextPatternId, UIA_ValuePatternId}, WindowsAndMessaging::{GetWindowRect, PW_RENDERFULLCONTENT}},
};

pub fn run(pid: u32, output: &Path) -> Result<(), String> {
    let windows = faber_testkit::desktop::visible_windows_of(&[pid])?;
    let hwnd = windows.into_iter().filter_map(|raw| {
        let hwnd = HWND(raw as *mut _);
        let mut rect = RECT::default();
        unsafe { GetWindowRect(hwnd, &mut rect).ok()?; }
        let width = rect.right - rect.left;
        let height = rect.bottom - rect.top;
        (width > 0 && height > 0).then_some((i64::from(width) * i64::from(height), hwnd))
    }).max_by_key(|(area, _)| *area).map(|(_, hwnd)| hwnd).ok_or("No nonempty window on hidden desktop")?;
    let screenshot = unsafe { capture(hwnd, &output.with_extension("png")) };
    let accessibility = unsafe { tree(hwnd) };
    std::fs::write(output.with_extension("uia.txt"), format!("screenshot: {screenshot:?}\n{}", accessibility.unwrap_or_else(|e| format!("UIA error: {e}")))).map_err(|e| e.to_string())?;
    Ok(())
}
unsafe fn capture(hwnd: HWND, path: &Path) -> Result<(), String> {
    unsafe {
        let mut rect = RECT::default();
        GetWindowRect(hwnd, &mut rect).map_err(|e| e.to_string())?;
        let width = rect.right - rect.left;
        let height = rect.bottom - rect.top;
        if width <= 0 || height <= 0 || width > 10000 || height > 10000 { return Err("Invalid window size".into()); }
        let dc = CreateCompatibleDC(None);
        if dc.is_invalid() { return Err("CreateCompatibleDC failed".into()); }
        let mut info = BITMAPINFO::default();
        info.bmiHeader = BITMAPINFOHEADER { biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32, biWidth: width, biHeight: -height, biPlanes: 1, biBitCount: 32, biCompression: BI_RGB.0, ..Default::default() };
        let mut bits = std::ptr::null_mut();
        let bitmap = CreateDIBSection(Some(dc), &info, DIB_RGB_COLORS, &mut bits, None, 0).map_err(|e| e.to_string())?;
        let old = SelectObject(dc, HGDIOBJ(bitmap.0));
        let printed = PrintWindow(hwnd, dc, PRINT_WINDOW_FLAGS(PW_RENDERFULLCONTENT)).as_bool();
        let source = std::slice::from_raw_parts(bits as *const u8, width as usize * height as usize * 4);
        let mut rgba = source.to_vec();
        for pixel in rgba.chunks_exact_mut(4) { pixel.swap(0, 2); pixel[3] = 255; }
        SelectObject(dc, old);
        let _ = DeleteObject(HGDIOBJ(bitmap.0));
        let _ = DeleteDC(dc);
        if !printed { return Err("PrintWindow failed".into()); }
        let file = File::create(path).map_err(|e| e.to_string())?;
        let mut encoder = png::Encoder::new(BufWriter::new(file), width as u32, height as u32);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().map_err(|e| e.to_string())?;
        writer.write_image_data(&rgba).map_err(|e| e.to_string())?;
        Ok(())
    }
}
unsafe fn tree(hwnd: HWND) -> Result<String, String> {
    unsafe {
        CoInitializeEx(None, COINIT_MULTITHREADED).ok().map_err(|e| e.to_string())?;
        let automation: IUIAutomation = CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER).map_err(|e| e.to_string())?;
        let root = automation.ElementFromHandle(hwnd).map_err(|e| e.to_string())?;
        let walker = automation.RawViewWalker().map_err(|e| e.to_string())?;
        let mut out = String::new();
        walk(&walker, &root, 0, &mut out);
        Ok(out)
    }
}
unsafe fn walk(walker: &IUIAutomationTreeWalker, element: &IUIAutomationElement, depth: usize, out: &mut String) {
    if depth > 30 || out.len() > 1_000_000 { return; }
    unsafe {
        let name = element.CurrentName().map(|s| s.to_string()).unwrap_or_default();
        let value = element.GetCurrentPatternAs::<IUIAutomationValuePattern>(UIA_ValuePatternId).and_then(|p| p.CurrentValue()).map(|s| s.to_string()).unwrap_or_default();
        let text = element.GetCurrentPatternAs::<IUIAutomationTextPattern>(UIA_TextPatternId).and_then(|p| p.DocumentRange()).and_then(|r| r.GetText(-1)).map(|s| s.to_string()).unwrap_or_default();
        out.push_str(&format!("{}name={name:?} value={value:?} text={text:?}\n", " ".repeat(depth)));
        if let Ok(mut child) = walker.GetFirstChildElement(element) {
            loop {
                walk(walker, &child, depth + 1, out);
                match walker.GetNextSiblingElement(&child) { Ok(next) => child = next, Err(_) => break }
            }
        }
    }
}


