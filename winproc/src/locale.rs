//! The language of the signed-in user ("pt-BR"), for the command line's words when Settings follows the system.

use windows_sys::Win32::Globalization::GetUserDefaultLocaleName;

/// The user's locale name, or None when Windows does not say.
pub fn user_language() -> Option<String> {
    // LOCALE_NAME_MAX_LENGTH.
    let mut name = [0u16; 85];
    // SAFETY: the buffer and its length in characters are passed together; Windows writes at most that many.
    let written = unsafe { GetUserDefaultLocaleName(name.as_mut_ptr(), name.len() as i32) };
    let len = usize::try_from(written).ok()?.checked_sub(1)?;
    Some(String::from_utf16_lossy(&name[..len])).filter(|n| !n.is_empty())
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_user_language_is_a_locale_name() {
        let name = super::user_language().expect("Windows names the user's locale");
        assert!(name.chars().all(|c| c.is_ascii_alphanumeric() || c == '-'), "{name}");
    }
}
