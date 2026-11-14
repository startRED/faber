pub(super) fn get(name: &str) -> Option<&'static str> {
    match name {
        "audiobox-aesthetics-linux.txt" => Some(include_str!("audiobox-aesthetics-linux.txt")),
        "audiobox-aesthetics-windows.txt" => Some(include_str!("audiobox-aesthetics-windows.txt")),
        "chatterbox-multilingual-linux.txt" => Some(include_str!("chatterbox-multilingual-linux.txt")),
        "chatterbox-multilingual-windows.txt" => Some(include_str!("chatterbox-multilingual-windows.txt")),
        "faster-whisper-linux.txt" => Some(include_str!("faster-whisper-linux.txt")),
        "faster-whisper-macos.txt" => Some(include_str!("faster-whisper-macos.txt")),
        "faster-whisper-windows.txt" => Some(include_str!("faster-whisper-windows.txt")),
        "speechmos-linux.txt" => Some(include_str!("speechmos-linux.txt")),
        "speechmos-macos.txt" => Some(include_str!("speechmos-macos.txt")),
        "speechmos-windows.txt" => Some(include_str!("speechmos-windows.txt")),
        "utmosv2-linux.txt" => Some(include_str!("utmosv2-linux.txt")),
        "utmosv2-windows.txt" => Some(include_str!("utmosv2-windows.txt")),
        _ => None,
    }
}
