use tauri::AppHandle;
use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Modifiers, Shortcut, ShortcutState};

pub(crate) fn parse_shortcut(shortcut_str: &str) -> Result<Shortcut, String> {
    let parts: Vec<&str> = shortcut_str.split('+').map(|s| s.trim()).collect();
    let mut mods = Modifiers::empty();
    let mut key_code: Option<Code> = None;
    for part in &parts {
        match part.to_lowercase().as_str() {
            "alt" => mods |= Modifiers::ALT,
            "ctrl" | "control" => mods |= Modifiers::CONTROL,
            "shift" => mods |= Modifiers::SHIFT,
            "super" | "win" | "meta" => mods |= Modifiers::SUPER,
            k => {
                key_code = Some(match k.to_uppercase().as_str() {
                    "SPACE" => Code::Space, "TAB" => Code::Tab,
                    "ENTER" | "RETURN" => Code::Enter, "ESCAPE" | "ESC" => Code::Escape,
                    "BACKSPACE" => Code::Backspace, "DELETE" | "DEL" => Code::Delete,
                    "INSERT" => Code::Insert, "HOME" => Code::Home, "END" => Code::End,
                    "PAGEUP" => Code::PageUp, "PAGEDOWN" => Code::PageDown,
                    "ARROWUP" | "UP" => Code::ArrowUp, "ARROWDOWN" | "DOWN" => Code::ArrowDown,
                    "ARROWLEFT" | "LEFT" => Code::ArrowLeft, "ARROWRIGHT" | "RIGHT" => Code::ArrowRight,
                    "F1" => Code::F1, "F2" => Code::F2, "F3" => Code::F3, "F4" => Code::F4,
                    "F5" => Code::F5, "F6" => Code::F6, "F7" => Code::F7, "F8" => Code::F8,
                    "F9" => Code::F9, "F10" => Code::F10, "F11" => Code::F11, "F12" => Code::F12,
                    "A" => Code::KeyA, "B" => Code::KeyB, "C" => Code::KeyC, "D" => Code::KeyD,
                    "E" => Code::KeyE, "F" => Code::KeyF, "G" => Code::KeyG, "H" => Code::KeyH,
                    "I" => Code::KeyI, "J" => Code::KeyJ, "K" => Code::KeyK, "L" => Code::KeyL,
                    "M" => Code::KeyM, "N" => Code::KeyN, "O" => Code::KeyO, "P" => Code::KeyP,
                    "Q" => Code::KeyQ, "R" => Code::KeyR, "S" => Code::KeyS, "T" => Code::KeyT,
                    "U" => Code::KeyU, "V" => Code::KeyV, "W" => Code::KeyW, "X" => Code::KeyX,
                    "Y" => Code::KeyY, "Z" => Code::KeyZ,
                    "0" => Code::Digit0, "1" => Code::Digit1, "2" => Code::Digit2, "3" => Code::Digit3,
                    "4" => Code::Digit4, "5" => Code::Digit5, "6" => Code::Digit6, "7" => Code::Digit7,
                    "8" => Code::Digit8, "9" => Code::Digit9,
                    _ => return Err(format!("unknown key: {}", k)),
                });
            }
        }
    }
    let code = key_code.ok_or("No valid key code found")?;
    Ok(Shortcut::new(Some(mods), code))
}

pub(crate) fn register_shortcut_internal(app: &AppHandle, shortcut_str: &str) -> Result<(), String> {
    let shortcut = parse_shortcut(shortcut_str)?;
    let gs = app.global_shortcut();
    let handle = app.clone();
    gs.on_shortcut(shortcut, move |_, _, event| {
        if event.state == ShortcutState::Pressed {
            crate::toggle_window(&handle);
        }
    }).map_err(|e| format!("register shortcut failed: {}", e))?;
    Ok(())
}
