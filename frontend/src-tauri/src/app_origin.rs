//! 웹뷰가 앱 자신의 페이지에 있을 때만 허용하는 것들: 페이지 이동과 마이크 권한. 판단은 순수
//! 함수로 두고 `lib.rs`가 Tauri 훅에 연결한다.

use tauri::webview::{PermissionKind, PermissionResponse};
use tauri::Url;

/// 앱 자신의 페이지인가. 설치본은 `tauri://localhost`(Linux, macOS)나 `http(s)://tauri.localhost`
/// (Windows)에서 뜨고, `tauri dev`는 `dev_url`(vite)에서 뜬다. 같은 컴퓨터의 백엔드
/// (`http://127.0.0.1:<port>`)도 앱 페이지가 아니다.
pub fn is_app_origin(url: &Url, dev_url: Option<&Url>) -> bool {
    let bundled = match url.scheme() {
        "tauri" => url.host_str() == Some("localhost"),
        "http" | "https" => url.host_str() == Some("tauri.localhost") && url.port().is_none(),
        _ => false,
    };
    bundled || dev_url.is_some_and(|dev| dev.origin() == url.origin())
}

/// 주 웹뷰의 페이지 이동을 허용할지. 앱 페이지 밖으로는 나가지 않는다.
pub fn allow_navigation(url: &Url, dev_url: Option<&Url>) -> bool {
    is_app_origin(url, dev_url)
}

/// 권한 요청에 대한 답. Linux의 WebKitGTK는 처리하는 쪽이 없으면 묻지 않고 거부하므로, 앱 페이지가
/// 요청한 마이크만 허용한다. 나머지는 플랫폼 기본 동작(`Default`)이다.
pub fn permission_for(
    is_linux: bool,
    url: Option<&Url>,
    dev_url: Option<&Url>,
    kind: PermissionKind,
) -> PermissionResponse {
    let app_page = url.is_some_and(|url| is_app_origin(url, dev_url));
    if is_linux && app_page && matches!(kind, PermissionKind::Microphone) {
        PermissionResponse::Allow
    } else {
        PermissionResponse::Default
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn url(value: &str) -> Url {
        value.parse().unwrap()
    }

    fn allowed(response: PermissionResponse) -> bool {
        matches!(response, PermissionResponse::Allow)
    }

    fn is_default(response: PermissionResponse) -> bool {
        matches!(response, PermissionResponse::Default)
    }

    #[test]
    fn app_page_gets_the_microphone_on_linux() {
        let page = url("tauri://localhost/index.html");
        assert!(allowed(permission_for(
            true,
            Some(&page),
            None,
            PermissionKind::Microphone
        )));
    }

    #[test]
    fn foreign_page_does_not_get_the_microphone() {
        let page = url("https://evil.example/");
        assert!(is_default(permission_for(
            true,
            Some(&page),
            None,
            PermissionKind::Microphone
        )));
    }

    #[test]
    fn backend_origin_does_not_get_the_microphone() {
        let page = url("http://127.0.0.1:38869/api/meta/models");
        assert!(is_default(permission_for(
            true,
            Some(&page),
            None,
            PermissionKind::Microphone
        )));
    }

    #[test]
    fn app_page_does_not_get_the_camera() {
        let page = url("tauri://localhost/");
        assert!(is_default(permission_for(
            true,
            Some(&page),
            None,
            PermissionKind::Camera
        )));
    }

    #[test]
    fn other_platforms_keep_their_prompt() {
        let page = url("tauri://localhost/");
        assert!(is_default(permission_for(
            false,
            Some(&page),
            None,
            PermissionKind::Microphone
        )));
    }

    #[test]
    fn missing_url_gets_nothing() {
        assert!(is_default(permission_for(
            true,
            None,
            None,
            PermissionKind::Microphone
        )));
    }

    #[test]
    fn dev_server_page_gets_the_microphone_only_when_it_is_the_dev_url() {
        let dev = url("http://localhost:5173");
        let page = url("http://localhost:5173/");
        assert!(allowed(permission_for(
            true,
            Some(&page),
            Some(&dev),
            PermissionKind::Microphone
        )));
        assert!(is_default(permission_for(
            true,
            Some(&page),
            None,
            PermissionKind::Microphone
        )));
    }

    #[test]
    fn navigation_stays_on_the_app_pages() {
        let dev = url("http://localhost:5173");
        for allowed_url in [
            "tauri://localhost/",
            "tauri://localhost/index.html#settings",
            "http://tauri.localhost/",
            "https://tauri.localhost/index.html",
        ] {
            assert!(allow_navigation(&url(allowed_url), None), "{allowed_url}");
        }
        assert!(allow_navigation(
            &url("http://localhost:5173/src/main.jsx"),
            Some(&dev)
        ));

        for blocked in [
            "https://evil.example/",
            "http://127.0.0.1:38869/",
            "http://localhost:5173/",
            "http://tauri.localhost:8080/",
            "tauri://evil.example/",
            "https://huggingface.co/",
            "file:///etc/passwd",
        ] {
            assert!(!allow_navigation(&url(blocked), None), "{blocked}");
        }
        assert!(!allow_navigation(
            &url("http://localhost:5174/"),
            Some(&dev)
        ));
    }
}
