/// 앱 명령을 매니페스트에 적어 두면 capability에 적힌 명령만 웹뷰가 부를 수 있다.
const COMMANDS: &[&str] = &[
    "get_backend_url",
    "get_api_key",
    "set_api_key",
    "save_backup_file",
    "pick_backup_file",
];

fn main() {
    tauri_build::try_build(
        tauri_build::Attributes::new()
            .app_manifest(tauri_build::AppManifest::new().commands(COMMANDS)),
    )
    .expect("failed to run tauri-build");
}
