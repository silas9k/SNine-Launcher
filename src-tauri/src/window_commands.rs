fn command_error(error: impl std::fmt::Display) -> String {
    error.to_string()
}

#[tauri::command]
pub fn window_minimize(window: tauri::WebviewWindow) -> Result<(), String> {
    window.minimize().map_err(command_error)
}

#[tauri::command]
pub fn window_toggle_maximize(window: tauri::WebviewWindow) -> Result<(), String> {
    if window.is_maximized().map_err(command_error)? {
        window.unmaximize().map_err(command_error)
    } else {
        window.maximize().map_err(command_error)
    }
}

#[tauri::command]
pub fn window_close(window: tauri::WebviewWindow) -> Result<(), String> {
    window.close().map_err(command_error)
}

#[tauri::command]
pub fn window_start_dragging(window: tauri::WebviewWindow) -> Result<(), String> {
    window.start_dragging().map_err(command_error)
}
