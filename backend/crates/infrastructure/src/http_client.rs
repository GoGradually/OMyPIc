//! 백엔드의 모든 HTTP 클라이언트(블로킹과 비동기)를 만드는 곳.
//!
//! `rustls-tls-native-roots` 기능을 켠 `reqwest`는 OS 인증서 저장소(또는 `SSL_CERT_FILE` /
//! `SSL_CERT_DIR`)에서 읽은 인증서가 있는데 그중 쓸 수 있는 것이 하나도 없으면 클라이언트를 만들지
//! 못한다("zero valid certificates found in native root store"). 그 저장소 하나 때문에 앱이 뜨지
//! 못하지 않도록, 만들기에 실패하면 경고를 한 번 남기고 OS 저장소를 끈 채(프로그램에 들어 있는
//! 루트 인증서는 그대로 쓴다) 다시 만든다. 한 번 실패한 뒤로는 OS 저장소를 다시 읽지 않는다.

use std::sync::atomic::{AtomicBool, Ordering};

/// OS 인증서 저장소로 클라이언트를 만들지 못한 적이 있다(이 프로세스에서).
static NATIVE_ROOTS_UNUSABLE: AtomicBool = AtomicBool::new(false);

/// `make`가 돌려주는 설정으로 블로킹 클라이언트를 만든다. 블로킹 클라이언트는 내부 스레드를 띄우고
/// 놓을 때 그 스레드를 기다리므로 비동기 작업자 스레드가 아닌 곳에서 만들고 놓는다.
pub fn build_blocking_client(
    make: impl Fn() -> reqwest::blocking::ClientBuilder,
) -> reqwest::Result<reqwest::blocking::Client> {
    with_native_roots_fallback(
        &NATIVE_ROOTS_UNUSABLE,
        || make().build(),
        || make().tls_built_in_native_certs(false).build(),
    )
}

/// `make`가 돌려주는 설정으로 비동기 클라이언트를 만든다.
pub fn build_async_client(
    make: impl Fn() -> reqwest::ClientBuilder,
) -> reqwest::Result<reqwest::Client> {
    with_native_roots_fallback(
        &NATIVE_ROOTS_UNUSABLE,
        || make().build(),
        || make().tls_built_in_native_certs(false).build(),
    )
}

/// `with_native`로 만들어 보고, 실패하면 `without_native`로 만든다. 두 번째가 성공하면 `unusable`을
/// 세우고(처음 세울 때만 경고를 남긴다) 다음부터는 바로 `without_native`로 만든다. 둘 다 실패하면
/// 두 번째 오류를 돌려준다(OS 저장소와 상관없는 설정 오류다).
fn with_native_roots_fallback<C>(
    unusable: &AtomicBool,
    with_native: impl FnOnce() -> reqwest::Result<C>,
    without_native: impl FnOnce() -> reqwest::Result<C>,
) -> reqwest::Result<C> {
    if unusable.load(Ordering::Relaxed) {
        return without_native();
    }
    let native_error = match with_native() {
        Ok(client) => return Ok(client),
        Err(err) => err,
    };
    let client = without_native()?;
    if !unusable.swap(true, Ordering::Relaxed) {
        tracing::warn!(
            "The OS certificate store is unusable ({}); using the built-in root certificates only",
            crate::openai::http::chain(&native_error)
        );
    }
    Ok(client)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    /// 실패하는 `reqwest` 만들기 오류(헤더 값으로 쓸 수 없는 User-Agent는 `build`에서 실패한다).
    fn builder_error() -> reqwest::Error {
        reqwest::Client::builder()
            .user_agent("\u{0}")
            .build()
            .expect_err("an invalid header value fails the build")
    }

    #[test]
    fn a_failed_native_build_falls_back_once_and_later_builds_skip_the_native_store() {
        let unusable = AtomicBool::new(false);
        let native_tries = Cell::new(0);

        let built = with_native_roots_fallback(
            &unusable,
            || {
                native_tries.set(native_tries.get() + 1);
                Err::<&str, _>(builder_error())
            },
            || Ok("built-in roots"),
        );
        assert_eq!("built-in roots", built.unwrap());
        assert!(unusable.load(Ordering::Relaxed));

        let again = with_native_roots_fallback(
            &unusable,
            || {
                native_tries.set(native_tries.get() + 1);
                Ok("native")
            },
            || Ok("built-in roots"),
        );
        assert_eq!("built-in roots", again.unwrap());
        assert_eq!(1, native_tries.get(), "the OS store is not read again");
    }

    #[test]
    fn a_working_native_build_is_used_and_a_failing_fallback_reports_its_error() {
        let unusable = AtomicBool::new(false);
        let built = with_native_roots_fallback(&unusable, || Ok("native"), || Ok("built-in"));
        assert_eq!("native", built.unwrap());
        assert!(!unusable.load(Ordering::Relaxed));

        let failed = with_native_roots_fallback::<()>(
            &unusable,
            || Err(builder_error()),
            || Err(builder_error()),
        );
        assert!(failed.unwrap_err().is_builder());
        assert!(!unusable.load(Ordering::Relaxed));
    }

    const CHILD_MARKER: &str = "OMYPIC_TLS_FALLBACK_CHILD";

    /// 환경 변수는 프로세스 전체에 걸리므로 이 시험 하나만 따로 띄운 자식 프로세스에서 돌린다.
    #[test]
    fn a_junk_ssl_cert_file_falls_back_to_the_built_in_roots() {
        if std::env::var_os(CHILD_MARKER).is_some() {
            // 자식: 보통 만들기는 실패하고 도우미로 만들면 성공한다.
            let plain = reqwest::blocking::Client::builder().build();
            assert!(
                plain
                    .as_ref()
                    .err()
                    .is_some_and(|err| crate::openai::http::chain(err).contains("native root")),
                "the junk store breaks a plain build: {:?}",
                plain.err()
            );
            build_blocking_client(reqwest::blocking::Client::builder).unwrap();
            build_async_client(reqwest::Client::builder).unwrap();
            assert!(NATIVE_ROOTS_UNUSABLE.load(Ordering::Relaxed));
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        let junk = dir.path().join("junk.pem");
        std::fs::write(
            &junk,
            "-----BEGIN CERTIFICATE-----\naGVsbG8gd29ybGQsIHRoaXMgaXMgbm90IGEgY2VydGlmaWNhdGU=\n-----END CERTIFICATE-----\n",
        )
        .unwrap();
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "http_client::tests::a_junk_ssl_cert_file_falls_back_to_the_built_in_roots",
                "--nocapture",
                "--test-threads=1",
            ])
            .env(CHILD_MARKER, "1")
            .env("SSL_CERT_FILE", &junk)
            .env_remove("SSL_CERT_DIR")
            .output()
            .unwrap();
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(output.status.success(), "{stdout}\n{stderr}");
        assert!(stdout.contains("1 passed"), "{stdout}\n{stderr}");
    }
}
