use cortex;
use std::time::{Instant, UNIX_EPOCH};
use std::path::PathBuf;
use std::fs;
use serde::{Serialize, Deserialize};
use tauri::{AppHandle, Emitter};

#[derive(Serialize, Deserialize)]
struct FileItem {
    name: String,
    size: u64,
    #[serde(rename = "type")]
    item_type: String,
    modified_ts: u64,
    path: String,
    is_dir: bool,
}

#[derive(Clone, Serialize)]
struct ProgressPayload {
    processed: usize,
    total: usize,
    is_compressing: bool,
}

#[tauri::command]
fn list_directory(path: String) -> Result<Vec<FileItem>, String> {
    let target_path = if path.is_empty() || path == "CORTEX Workspace /" {
        std::env::current_dir().unwrap_or_else(|_| PathBuf::from("/"))
    } else {
        PathBuf::from(&path)
    };
    
    // Virtual File System for .crx files
    if path.ends_with(".crx") && target_path.is_file() {
        if let Ok(Some(meta_bytes)) = cortex::read_metadata(&path) {
            if let Ok(items) = serde_json::from_slice::<Vec<FileItem>>(&meta_bytes) {
                return Ok(items);
            }
        }
        return Err("Cannot read archive contents or archive is in older format.".to_string());
    }

    let mut items = Vec::new();
    let entries = fs::read_dir(&target_path).map_err(|e| e.to_string())?;

    for entry in entries.filter_map(Result::ok) {
        let meta = match entry.metadata() {
            Ok(m) => m,
            Err(_) => continue,
        };
        let name = entry.file_name().to_string_lossy().to_string();
        
        if name.starts_with('.') {
            continue;
        }

        let is_dir = meta.is_dir();
        let item_type = if is_dir {
            "Folder".to_string()
        } else if name.ends_with(".crx") {
            "CORTEX Archive".to_string()
        } else {
            "File".to_string()
        };

        let modified_ts = meta.modified()
            .unwrap_or(UNIX_EPOCH)
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        items.push(FileItem {
            name,
            size: if is_dir { 0 } else { meta.len() },
            item_type,
            modified_ts,
            path: entry.path().to_string_lossy().to_string(),
            is_dir,
        });
    }
    
    items.sort_by(|a, b| {
        b.is_dir.cmp(&a.is_dir).then(a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });

    Ok(items)
}

#[tauri::command]
fn get_parent_directory(path: String) -> String {
    let p = PathBuf::from(path);
    if let Some(parent) = p.parent() {
        parent.to_string_lossy().to_string()
    } else {
        p.to_string_lossy().to_string()
    }
}

#[tauri::command]
fn get_cli_args() -> Vec<String> {
    std::env::args().collect()
}

#[tauri::command]
fn exit_app() {
    std::process::exit(0);
}

#[tauri::command]
async fn compress_cmd(app: AppHandle, input_paths: Vec<String>, output_path: String, password: Option<String>, level: u8, split_size: usize) -> Result<String, String> {
    let start = Instant::now();
    let temp_tar_path = format!("{}.tmp.tar", output_path);
    
    let mut meta_items = Vec::new();
    
    // Create Tar bundle & build metadata
    {
        let tar_file = fs::File::create(&temp_tar_path).map_err(|e| e.to_string())?;
        let mut builder = tar::Builder::new(tar_file);
        for path in &input_paths {
            let p = PathBuf::from(path);
            let name = p.file_name().unwrap_or_default().to_string_lossy().to_string();
            
            let meta = fs::metadata(&p).map_err(|e| e.to_string())?;
            let is_dir = p.is_dir();
            
            let item_type = if is_dir {
                "Folder".to_string()
            } else {
                "File".to_string()
            };
            
            let modified_ts = meta.modified()
                .unwrap_or(UNIX_EPOCH)
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs();

            meta_items.push(FileItem {
                name: name.clone(),
                size: if is_dir { 0 } else { meta.len() },
                item_type,
                modified_ts,
                path: p.to_string_lossy().to_string(), // In VFS this won't be a real disk path, but UI needs something
                is_dir,
            });

            if is_dir {
                builder.append_dir_all(&name, &p).map_err(|e| e.to_string())?;
            } else {
                let mut f = fs::File::open(&p).map_err(|e| e.to_string())?;
                builder.append_file(&name, &mut f).map_err(|e| e.to_string())?;
            }
        }
        builder.finish().map_err(|e| e.to_string())?;
    }
    
    let meta_json = serde_json::to_vec(&meta_items).map_err(|e| e.to_string())?;
    
    let temp_tar_clone = temp_tar_path.clone();
    let out_clone = output_path.clone();
    let pwd_clone = password.clone();
    
    let res = std::thread::spawn(move || {
        let pwd_ref = pwd_clone.as_deref();
        cortex::compress_file_with_progress(&temp_tar_clone, &out_clone, Some(&meta_json), pwd_ref, level, split_size, 16 * 1024 * 1024, |processed, total| {
            let _ = app.emit("progress", ProgressPayload {
                processed,
                total,
                is_compressing: true,
            });
        })
    }).join().map_err(|_| "Thread panic".to_string())?;
    
    let _ = fs::remove_file(&temp_tar_path);
    
    match res {
        Ok(stats) => {
            let duration = start.elapsed().as_secs_f64();
            let in_mb = stats.input_size as f64 / 1_048_576.0;
            let out_mb = stats.output_size as f64 / 1_048_576.0;
            Ok(format!("Compressed {:.2} MB -> {:.2} MB in {:.2}s", in_mb, out_mb, duration))
        }
        Err(e) => Err(format!("Compression failed: {}", e)),
    }
}

#[tauri::command]
async fn decompress_cmd(app: AppHandle, input_path: String, output_path: String, password: Option<String>) -> Result<String, String> {
    let start = Instant::now();
    let temp_tar_path = format!("{}.tmp.tar", input_path);
    
    let temp_tar_clone = temp_tar_path.clone();
    let in_clone = input_path.clone();
    let pwd_clone = password.clone();
    
    let res = std::thread::spawn(move || {
        let pwd_ref = pwd_clone.as_deref();
        cortex::decompress_file_with_progress(&in_clone, &temp_tar_clone, pwd_ref, |processed, total| {
            let _ = app.emit("progress", ProgressPayload {
                processed,
                total,
                is_compressing: false,
            });
        })
    }).join().map_err(|_| "Thread panic".to_string())?;

    match res {
        Ok(stats) => {
            // Unpack Tar bundle to the destination directory
            {
                let tar_file = fs::File::open(&temp_tar_path).map_err(|e| format!("Tar open error: {}", e))?;
                let mut archive = tar::Archive::new(tar_file);
                archive.unpack(&output_path).map_err(|e| format!("Tar unpack error: {}", e))?;
            }
            
            let _ = fs::remove_file(&temp_tar_path);

            let duration = start.elapsed().as_secs_f64();
            let in_mb = stats.input_size as f64 / 1_048_576.0;
            let out_mb = stats.output_size as f64 / 1_048_576.0;
            Ok(format!("Decompressed {:.2} MB -> {:.2} MB in {:.2}s", in_mb, out_mb, duration))
        }
        Err(e) => {
            let _ = fs::remove_file(&temp_tar_path);
            Err(format!("Decompression failed: {}", e))
        }
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
  tauri::Builder::default()
    .setup(|app| {
      if cfg!(debug_assertions) {
        app.handle().plugin(
          tauri_plugin_log::Builder::default()
            .level(log::LevelFilter::Info)
            .build(),
        )?;
      }
      Ok(())
    })
    .plugin(tauri_plugin_dialog::init())
    .invoke_handler(tauri::generate_handler![
        compress_cmd, 
        decompress_cmd, 
        list_directory, 
        get_parent_directory,
        get_cli_args,
        exit_app
    ])
    .run(tauri::generate_context!())
    .expect("error while running tauri application");
}
