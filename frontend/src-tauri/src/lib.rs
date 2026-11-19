//! OMyPIc 데스크톱 앱(Tauri). 백엔드를 이 프로세스 안에서 띄우고, 웹뷰에는 백엔드 주소, API 키,
//! 백업 파일 명령만 연다. 명령은 `keys`, `backup`의 순수 함수를 감싼다.

pub mod backend;
pub mod backup;
pub mod keys;
pub mod paths;

use std::fs::{self, OpenOptions};
use std::io::IsTerminal;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use tauri::{AppHandle, Manager, RunEvent, State};
use tauri_plugin_dialog::{DialogExt, FilePath, MessageDialogKind};
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::{fmt, EnvFilter, Layer};

use crate::backend::Backend;
use crate::backup::PickedFile;
use crate::keys::KeyringStore;

const MAIN_WINDOW: &str = "main";

#[tauri::command]
fn get_backend_url(backend: State<'_, Backend>) -> String {
    backend.url().to_string()
}

#[tauri::command]
async fn get_api_key(provider: Option<String>) -> Option<String> {
    tauri::async_runtime::spawn_blocking(move || {
        keys::get_api_key(&KeyringStore, provider.as_deref())
    })
    .await
    .ok()
    .flatten()
}

#[tauri::command]
async fn set_api_key(key: Option<String>, provider: Option<String>) -> Result<bool, String> {
    tauri::async_runtime::spawn_blocking(move || {
        keys::set_api_key(&KeyringStore, key.as_deref(), provider.as_deref())
    })
    .await
    .map_err(|err| err.to_string())??;
    Ok(true)
}

#[tauri::command]
async fn save_backup_file(
    app: AppHandle,
    default_name: Option<String>,
    bytes: Vec<u8>,
) -> Result<bool, String> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    zip_dialog(&app)
        .set_file_name(backup::default_name(default_name))
        .save_file(move |path| {
            let _ = tx.send(path);
        });
    let path = chosen_path(rx.await.ok().flatten())?;
    tauri::async_runtime::spawn_blocking(move || backup::write_backup(path.as_deref(), &bytes))
        .await
        .map_err(|err| err.to_string())?
}

#[tauri::command]
async fn pick_backup_file(app: AppHandle) -> Result<Option<PickedFile>, String> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    zip_dialog(&app).pick_file(move |path| {
        let _ = tx.send(path);
    });
    let path = chosen_path(rx.await.ok().flatten())?;
    tauri::async_runtime::spawn_blocking(move || backup::read_backup(path.as_deref()))
        .await
        .map_err(|err| err.to_string())?
}

/// zip 필터를 건, 주 창에 딸린 파일 대화상자. 플러그인이 대화상자를 주 스레드에서 띄운다.
fn zip_dialog(app: &AppHandle) -> tauri_plugin_dialog::FileDialogBuilder<tauri::Wry> {
    let dialog = app
        .dialog()
        .file()
        .add_filter(backup::FILTER_NAME, backup::FILTER_EXTENSIONS);
    match app.get_webview_window(MAIN_WINDOW) {
        Some(window) => dialog.set_parent(&window),
        None => dialog,
    }
}

fn chosen_path(path: Option<FilePath>) -> Result<Option<PathBuf>, String> {
    path.map(|path| path.into_path().map_err(|err| err.to_string()))
        .transpose()
}

/// 앱을 실행한다. 돌아오지 않는다.
pub fn run() {
    let data_dir =
        paths::resolve_data_dir(std::env::var("OMYPIC_DATA_DIR").ok(), dirs::config_dir());
    init_logging(&data_dir);

    let app = tauri::Builder::default()
        // 두 번째 실행은 같은 데이터 폴더로 백엔드를 또 띄우지 않고 떠 있는 창을 앞으로 가져온다.
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            if let Some(window) = app.get_webview_window(MAIN_WINDOW) {
                let _ = window.unminimize();
                let _ = window.show();
                let _ = window.set_focus();
            }
        }))
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            get_backend_url,
            get_api_key,
            set_api_key,
            save_backup_file,
            pick_backup_file
        ])
        .setup(|app| {
            match start_backend(app.handle()) {
                Ok(backend) => {
                    tracing::info!("Backend URL: {}", backend.url());
                    app.manage(backend);
                    if let Some(window) = app.get_webview_window(MAIN_WINDOW) {
                        window.show()?;
                    }
                }
                Err(err) => {
                    tracing::error!("Startup failed: {err:#}");
                    let handle = app.handle().clone();
                    app.dialog()
                        .message(format!("{err:#}\n\n앱을 종료합니다."))
                        .title("OMyPIc startup error")
                        .kind(MessageDialogKind::Error)
                        .show(move |_| handle.exit(1));
                }
            }
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("failed to build the OMyPIc app");

    let code = app.run_return(|app, event| match event {
        // 마지막 창이 닫혔거나 앱을 끝내라고 했다. 서버를 끄는 동안 창이 남아 보이지 않게 한다.
        RunEvent::ExitRequested { .. } => {
            for window in app.webview_windows().values() {
                let _ = window.hide();
            }
        }
        RunEvent::Exit => {
            if let Some(backend) = app.try_state::<Backend>() {
                backend.shutdown();
            }
        }
        _ => {}
    });
    std::process::exit(code);
}

/// `OMYPIC_BACKEND_URL`이 있으면 그 서버를 쓰고, 없으면 내장 서버를 띄운다.
fn start_backend(app: &AppHandle) -> anyhow::Result<Backend> {
    if let Some(url) = std::env::var("OMYPIC_BACKEND_URL")
        .ok()
        .filter(|url| !url.trim().is_empty())
    {
        return Ok(Backend::external(url));
    }
    let resource_dir = app.path().resource_dir().ok();
    let config = backend::build_config(
        |name| std::env::var(name).ok(),
        dirs::config_dir(),
        resource_dir.as_deref(),
    )?;
    tracing::info!("Data directory: {}", config.data_dir);
    Backend::start(config)
}

/// 표준 오류와 `<data_dir>/logs/omypic.log`(이어 쓰기, 색 없음)에 로그를 남긴다. 로그 파일을 열 수
/// 없으면 표준 오류에만 쓴다.
fn init_logging(data_dir: &Path) {
    let filter = || EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    let stderr = fmt::layer()
        .with_writer(std::io::stderr)
        .with_ansi(std::io::stderr().is_terminal())
        .with_filter(filter());
    let log_dir = data_dir.join("logs");
    let file = fs::create_dir_all(&log_dir).and_then(|()| {
        OpenOptions::new()
            .create(true)
            .append(true)
            .open(log_dir.join("omypic.log"))
    });
    let (file_layer, file_error) = match file {
        Ok(file) => (
            Some(
                fmt::layer()
                    .with_writer(Arc::new(file))
                    .with_ansi(false)
                    .with_filter(filter()),
            ),
            None,
        ),
        Err(err) => (None, Some(err)),
    };
    let _ = tracing_subscriber::registry()
        .with(stderr)
        .with(file_layer)
        .try_init();
    if let Some(err) = file_error {
        tracing::warn!("Cannot write the log file in {}: {err}", log_dir.display());
    }
}
