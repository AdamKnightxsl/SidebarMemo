/// 校验路径组件（memo_id / filename），防止路径遍历。
pub(crate) fn validate_path_component(s: &str) -> Result<(), String> {
    if s.is_empty() || s.len() > 255 {
        return Err("非法的路径参数".to_string());
    }
    if s.contains('/') || s.contains('\\') || s.contains("..") {
        return Err("非法的路径参数".to_string());
    }
    if !s.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.') {
        return Err("非法的路径参数".to_string());
    }
    Ok(())
}

#[tauri::command]
pub(crate) fn save_image(state: tauri::State<crate::AppState>, memo_id: String, filename: String, data_base64: String) -> Result<String, String> {
    validate_path_component(&memo_id)?;
    validate_path_component(&filename)?;
    use base64::Engine;
    let img_data = base64::engine::general_purpose::STANDARD
        .decode(&data_base64)
        .map_err(|e| format!("base64 decode failed: {}", e))?;

    let img_dir = dirs::data_dir()
        .ok_or("no data dir")?
        .join("sidebar-memo")
        .join("images")
        .join(&memo_id);
    std::fs::create_dir_all(&img_dir).map_err(|e| format!("create dir failed: {}", e))?;

    let file_path = img_dir.join(&filename);
    std::fs::write(&file_path, &img_data).map_err(|e| format!("write file failed: {}", e))?;

    let store = state.store.lock().map_err(|e| e.to_string())?;
    let memos = store.get_all().map_err(|e| e.to_string())?;
    let memo = memos.iter().find(|m| m.id == memo_id)
        .ok_or("memo not found")?;
    let mut images: Vec<String> = serde_json::from_str(&memo.images).unwrap_or_default();
    if !images.contains(&filename) {
        images.push(filename.clone());
    }
    let images_json = serde_json::to_string(&images).unwrap_or_else(|_| "[]".to_string());
    store.update_images(&memo_id, &images_json).map_err(|e| e.to_string())?;

    Ok(images_json)
}

#[tauri::command]
pub(crate) fn delete_image(state: tauri::State<crate::AppState>, memo_id: String, filename: String) -> Result<String, String> {
    validate_path_component(&memo_id)?;
    validate_path_component(&filename)?;
    let img_dir = dirs::data_dir()
        .ok_or("no data dir")?
        .join("sidebar-memo")
        .join("images")
        .join(&memo_id);
    let file_path = img_dir.join(&filename);
    if file_path.exists() {
        let _ = std::fs::remove_file(&file_path);
    }

    let store = state.store.lock().map_err(|e| e.to_string())?;
    let memos = store.get_all().map_err(|e| e.to_string())?;
    let memo = memos.iter().find(|m| m.id == memo_id)
        .ok_or("memo not found")?;
    let mut images: Vec<String> = serde_json::from_str(&memo.images).unwrap_or_default();
    images.retain(|f| f != &filename);
    let images_json = serde_json::to_string(&images).unwrap_or_else(|_| "[]".to_string());
    store.update_images(&memo_id, &images_json).map_err(|e| e.to_string())?;

    Ok(images_json)
}

#[tauri::command]
pub(crate) fn get_image_base64(memo_id: String, filename: String) -> Result<String, String> {
    validate_path_component(&memo_id)?;
    validate_path_component(&filename)?;
    let file_path = dirs::data_dir()
        .ok_or("no data dir")?
        .join("sidebar-memo")
        .join("images")
        .join(&memo_id)
        .join(&filename);

    if !file_path.exists() {
        return Err("image file not found".to_string());
    }

    let data = std::fs::read(&file_path).map_err(|e| format!("read file failed: {}", e))?;
    use base64::Engine;
    Ok(base64::engine::general_purpose::STANDARD.encode(&data))
}

/// 返回图片文件的绝对路径（供前端 convertFileSrc 使用，避免 base64 IPC 传输）
#[tauri::command]
pub(crate) fn get_image_path(memo_id: String, filename: String) -> Result<String, String> {
    validate_path_component(&memo_id)?;
    validate_path_component(&filename)?;
    let file_path = dirs::data_dir()
        .ok_or("no data dir")?
        .join("sidebar-memo")
        .join("images")
        .join(&memo_id)
        .join(&filename);

    if !file_path.exists() {
        return Err("image file not found".to_string());
    }

    Ok(file_path.to_string_lossy().to_string())
}
