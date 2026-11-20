//! 웹뷰가 앱 자신의 페이지에 있을 때만 허용하는 것들: 페이지 이동과 마이크 권한. 판단은 순수
//! 함수로 두고 `lib.rs`가 Tauri 훅에 연결한다.

use tauri::webview::{PermissionKind, PermissionResponse};
use tauri::Url;

/// 앱 자신의 페이지인가. 설치본은 Linux와 macOS에서 `tauri://localhost`, Windows에서
/// `http(s)://tauri.localhost`(WebView2)에서 뜨고, `tauri dev`는 `dev_url`(vite)에서 뜬다. 포트나
/// 사용자 정보(`user@`)가 붙은 주소는 앱 페이지가 아니다. 같은 컴퓨터의 백엔드
/// (`http://127.0.0.1:<port>`)도 앱 페이지가 아니다.
pub fn is_app_origin(url: &Url, dev_url: Option<&Url>) -> bool {
    if !url.username().is_empty() || url.password().is_some() {
        return false;
    }
    let bundled_host = match url.scheme() {
        "tauri" => Some("localhost"),
        "http" | "https" if cfg!(windows) => Some("tauri.localhost"),
        _ => None,
    };
    let bundled =
        bundled_host.is_some_and(|host| url.host_str() == Some(host)) && url.port().is_none();
    bundled || dev_url.is_some_and(|dev| dev.origin() == url.origin())
}

/// 웹뷰(앱의 모든 웹뷰)의 페이지 이동을 허용할지. 앱 페이지 밖으로는 나가지 않는다.
pub fn allow_navigation(url: &Url, dev_url: Option<&Url>) -> bool {
    is_app_origin(url, dev_url)
}

/// 권한 요청에 대한 답. 앱 페이지가 요청한 마이크는 모든 데스크톱 OS에서 허용한다. 처리기를
/// 등록하면 `Default`는 Linux(WebKitGTK)에서 거부, macOS(WKWebView)에서 실행할 때마다 묻는
/// WebKit 창, Windows(WebView2)에서 답을 기억하는 웹뷰 창이 되기 때문이다. 시스템의 마이크
/// 권한(macOS TCC, Windows 개인 정보 설정)은 그대로 적용된다. 나머지는 `Default`다.
pub fn permission_for(
    url: Option<&Url>,
    dev_url: Option<&Url>,
    kind: PermissionKind,
) -> PermissionResponse {
    let app_page = url.is_some_and(|url| is_app_origin(url, dev_url));
    if app_page && matches!(kind, PermissionKind::Microphone) {
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
    fn app_page_gets_the_microphone() {
        let page = url("tauri://localhost/index.html");
        assert!(allowed(permission_for(
            Some(&page),
            None,
            PermissionKind::Microphone
        )));
    }

    #[test]
    fn foreign_page_does_not_get_the_microphone() {
        let page = url("https://evil.example/");
        assert!(is_default(permission_for(
            Some(&page),
            None,
            PermissionKind::Microphone
        )));
    }

    #[test]
    fn backend_origin_does_not_get_the_microphone() {
        let page = url("http://127.0.0.1:38869/api/meta/models");
        assert!(is_default(permission_for(
            Some(&page),
            None,
            PermissionKind::Microphone
        )));
    }

    #[test]
    fn app_page_does_not_get_the_camera() {
        let page = url("tauri://localhost/");
        assert!(is_default(permission_for(
            Some(&page),
            None,
            PermissionKind::Camera
        )));
    }

    #[test]
    fn the_windows_origin_is_an_app_page_only_on_windows() {
        // Windows WebView2 serves the app from http(s)://tauri.localhost. Elsewhere that host is
        // not the app (Tauri does not treat it as local there either).
        for page in [
            "http://tauri.localhost/",
            "https://tauri.localhost/index.html",
            "HTTP://TAURI.LOCALHOST/",
        ] {
            assert_eq!(
                cfg!(windows),
                allowed(permission_for(
                    Some(&url(page)),
                    None,
                    PermissionKind::Microphone
                )),
                "{page}"
            );
            assert_eq!(cfg!(windows), allow_navigation(&url(page), None), "{page}");
        }
    }

    #[test]
    fn userinfo_ports_and_other_hosts_are_not_app_pages() {
        for page in [
            "tauri://user@localhost/",
            "tauri://user:secret@localhost/index.html",
            "tauri://localhost:8080/",
            "TAURI://LOCALHOST/",
            "http://user@tauri.localhost/",
            "http://tauri.localhost@evil.example/",
            "https://tauri.localhost.evil.example/",
            "http://tauri.localhost:8080/",
        ] {
            assert!(!is_app_origin(&url(page), None), "{page}");
        }
        let dev = url("http://localhost:5173");
        assert!(!is_app_origin(
            &url("http://user@localhost:5173/"),
            Some(&dev)
        ));
    }

    #[test]
    fn missing_url_gets_nothing() {
        assert!(is_default(permission_for(
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
            Some(&page),
            Some(&dev),
            PermissionKind::Microphone
        )));
        assert!(is_default(permission_for(
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
