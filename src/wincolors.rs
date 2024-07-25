use anyhow::Result;
use windows::Win32::{
    Foundation::HANDLE,
    Storage::FileSystem::{
        CreateFileW, FILE_FLAGS_AND_ATTRIBUTES, FILE_GENERIC_READ, FILE_GENERIC_WRITE,
        FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
    },
    System::Console::{
        GetConsoleMode, SetConsoleMode, CONSOLE_MODE, ENABLE_VIRTUAL_TERMINAL_PROCESSING,
    },
};
use windows_strings::w;

/// owo-colors doesn't handle this. I could just use another dep to do it, but it's not complex
/// code. See <https://docs.microsoft.com/en-us/windows/console/console-virtual-terminal-sequences#example-of-sgr-terminal-sequences>
/// but CONOUT$ always works when GetStdHandle apparently might not:
/// <https://stackoverflow.com/a/45823353>
pub(crate) fn enable_colors() -> Result<()> {
    let handle = unsafe {
        CreateFileW(
            w!("CONOUT$"),
            (FILE_GENERIC_READ | FILE_GENERIC_WRITE).0,
            FILE_SHARE_READ | FILE_SHARE_WRITE,
            None,
            OPEN_EXISTING,
            FILE_FLAGS_AND_ATTRIBUTES::default(),
            HANDLE::default(),
        )?
    };
    let mut mode = CONSOLE_MODE::default();
    unsafe { GetConsoleMode(handle, &mut mode)? };
    if (mode & ENABLE_VIRTUAL_TERMINAL_PROCESSING).0 == 0 {
        unsafe { SetConsoleMode(handle, mode | ENABLE_VIRTUAL_TERMINAL_PROCESSING)? };
    }
    Ok(())
}
