//! OMyPIc 데스크톱 앱(Tauri). 백엔드를 이 프로세스 안에서 띄우고, 웹뷰에는 백엔드 주소, API 키,
//! 백업 파일 명령만 연다. 명령은 `keys`, `backup`의 순수 함수를 감싼다.

pub mod app_origin;
pub mod backend;
pub mod backup;
pub mod keys;
pub mod paths;

use std::fs::{self, OpenOptions};
use std::io::IsTerminal;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::Context;
use tauri::ipc::{Request, Response};
use tauri::{AppHandle, Manager, RunEvent, Runtime, State, Url, Webview};
use tauri_plugin_dialog::{DialogExt, FilePath, MessageDialogKind};
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::{fmt, EnvFilter, Layer};

use crate::backend::Backend;
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
    .unwrap_or_else(|err| {
        tracing::warn!("Reading the API key failed: {err}");
        None
    })
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

/// 요청 본문(백업 바이트 그대로)을 고른 경로에 쓴다. 본문은 복사하지 않는다.
#[tauri::command]
async fn save_backup_file(app: AppHandle, request: Request<'_>) -> Result<bool, String> {
    let name = request
        .headers()
        .get(backup::BACKUP_NAME_HEADER)
        .and_then(|value| value.to_str().ok());
    let default_name = backup::default_name(backup::decode_header_name(name));
    let bytes = backup::body_bytes(request.body())?;
    let (tx, rx) = tokio::sync::oneshot::channel();
    zip_dialog(&app)
        .set_file_name(default_name)
        .save_file(move |path| {
            let _ = tx.send(path);
        });
    let path = chosen_path(rx.await.ok().flatten())?;
    backup::write_backup(path.as_deref(), &bytes).await
}

/// 고른 파일의 이름과 내용을 바이트 그대로 돌려준다(`backup::read_backup`).
#[tauri::command]
async fn pick_backup_file(app: AppHandle) -> Result<Response, String> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    zip_dialog(&app).pick_file(move |path| {
        let _ = tx.send(path);
    });
    let path = chosen_path(rx.await.ok().flatten())?;
    let reply = tauri::async_runtime::spawn_blocking(move || backup::read_backup(path.as_deref()))
        .await
        .map_err(|err| err.to_string())??;
    Ok(Response::new(reply))
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

/// `tauri dev`에서 웹뷰가 여는 vite 주소. 설치본에는 없다.
fn dev_url<R: Runtime>(webview: &Webview<R>) -> Option<Url> {
    if cfg!(dev) {
        webview.config().build.dev_url.clone()
    } else {
        None
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
        // 어느 웹뷰도(설정 파일이 만드는 주 창 포함) 앱 페이지 밖으로 이동하지 않는다.
        .plugin(
            tauri::plugin::Builder::<tauri::Wry>::new("omypic-navigation")
                .on_navigation(|webview, url| {
                    let allowed = app_origin::allow_navigation(url, dev_url(webview).as_ref());
                    if !allowed {
                        tracing::warn!("Blocked navigation away from the app: {url}");
                    }
                    allowed
                })
                .build(),
        )
        .on_permission_request(|webview, kind| {
            app_origin::permission_for(
                webview.url().ok().as_ref(),
                dev_url(&webview).as_ref(),
                kind,
            )
        })
        .invoke_handler(tauri::generate_handler![
            get_backend_url,
            get_api_key,
            set_api_key,
            save_backup_file,
            pick_backup_file
        ])
        .setup(|app| {
            let started = start_backend(app.handle()).and_then(|backend| {
                tracing::info!("Backend URL: {}", backend.url());
                backend.finish_startup(|| match app.get_webview_window(MAIN_WINDOW) {
                    Some(window) => window.show().context("Failed to show the main window"),
                    None => Ok(()),
                })?;
                Ok(backend)
            });
            match started {
                Ok(backend) => {
                    app.manage(backend);
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

/// 로그 파일이 이 크기를 넘으면 시작할 때 `omypic.log.1`로 옮기고 새로 쓴다.
const LOG_ROTATE_BYTES: u64 = 5 * 1024 * 1024;

/// `log_file`이 `limit` 바이트보다 크면 `<log_file>.1`로 옮긴다(이전 `.1`은 덮어쓴다).
fn rotate_log(log_file: &Path, limit: u64) -> std::io::Result<()> {
    match fs::metadata(log_file) {
        Ok(meta) if meta.len() > limit => {
            let mut rotated = log_file.as_os_str().to_owned();
            rotated.push(".1");
            let rotated = PathBuf::from(rotated);
            // Windows의 rename은 대상이 있으면 실패한다.
            match fs::remove_file(&rotated) {
                Ok(()) => {}
                Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
                Err(err) => return Err(err),
            }
            fs::rename(log_file, rotated)
        }
        Ok(_) => Ok(()),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(err) => Err(err),
    }
}

/// 표준 오류와 `<data_dir>/logs/omypic.log`(이어 쓰기, 색 없음, 5 MB를 넘으면 시작할 때
/// `omypic.log.1`로 옮김)에 로그를 남긴다. 로그 파일을 열 수 없으면 표준 오류에만 쓴다.
fn init_logging(data_dir: &Path) {
    let filter = || EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    let stderr = fmt::layer()
        .with_writer(std::io::stderr)
        .with_ansi(std::io::stderr().is_terminal())
        .with_filter(filter());
    let log_dir = data_dir.join("logs");
    let log_file = log_dir.join("omypic.log");
    let rotate_error = rotate_log(&log_file, LOG_ROTATE_BYTES).err();
    let file = fs::create_dir_all(&log_dir)
        .and_then(|()| OpenOptions::new().create(true).append(true).open(&log_file));
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
    if let Some(err) = rotate_error {
        tracing::warn!("Cannot rotate the log file {}: {err}", log_file.display());
    }
    if let Some(err) = file_error {
        tracing::warn!("Cannot write the log file in {}: {err}", log_dir.display());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rotates_a_log_larger_than_the_limit() {
        let dir = tempfile::tempdir().unwrap();
        let log = dir.path().join("omypic.log");
        fs::write(&log, "0123456789").unwrap();
        fs::write(dir.path().join("omypic.log.1"), "older").unwrap();

        rotate_log(&log, 9).unwrap();

        assert!(!log.exists());
        assert_eq!(
            "0123456789",
            fs::read_to_string(dir.path().join("omypic.log.1")).unwrap()
        );
    }

    #[test]
    fn keeps_a_log_within_the_limit() {
        let dir = tempfile::tempdir().unwrap();
        let log = dir.path().join("omypic.log");
        fs::write(&log, "0123456789").unwrap();

        rotate_log(&log, 10).unwrap();

        assert_eq!("0123456789", fs::read_to_string(&log).unwrap());
        assert!(!dir.path().join("omypic.log.1").exists());
    }

    #[test]
    fn missing_log_is_not_an_error() {
        let dir = tempfile::tempdir().unwrap();
        rotate_log(&dir.path().join("logs/omypic.log"), 10).unwrap();
    }
}
