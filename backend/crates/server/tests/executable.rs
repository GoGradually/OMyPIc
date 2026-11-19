//! `omypic-server` 실행 파일 시험: 포트 충돌 시 종료 코드, 종료 신호로 정상 종료.

use std::io::{BufRead, BufReader};
use std::net::TcpListener;
use std::process::{Command, Stdio};
use std::time::Duration;

fn server_command(data_dir: &std::path::Path, port: u16) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_omypic-server"));
    command
        .env("OMYPIC_DATA_DIR", data_dir)
        .env("OMYPIC_PORT", port.to_string())
        .env("OMYPIC_RAG_PROVIDER", "hash")
        .env("RUST_LOG", "info")
        .stdout(Stdio::null())
        .stderr(Stdio::piped());
    command
}

#[test]
fn port_in_use_prints_bind_error_and_exits_with_1() {
    let dir = tempfile::tempdir().unwrap();
    let occupied = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = occupied.local_addr().unwrap().port();

    let output = server_command(dir.path(), port).output().unwrap();

    assert_eq!(Some(1), output.status.code());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains(&format!("Failed to bind 127.0.0.1:{port}: ")),
        "{stderr}"
    );
}

#[cfg(unix)]
#[test]
fn interrupt_signal_shuts_down_cleanly() {
    let dir = tempfile::tempdir().unwrap();
    let port = {
        let probe = TcpListener::bind("127.0.0.1:0").unwrap();
        probe.local_addr().unwrap().port()
    };
    let mut child = server_command(dir.path(), port).spawn().unwrap();
    let stderr = child.stderr.take().unwrap();
    let mut lines = BufReader::new(stderr).lines();
    let listening = format!("listening on http://127.0.0.1:{port}");
    loop {
        let line = lines
            .next()
            .expect("server exited before listening")
            .unwrap();
        assert!(!line.contains("Failed"), "{line}");
        // 터미널이 아닌 곳(파일, 상위 프로세스의 파이프)에는 색 코드를 쓰지 않는다.
        assert!(!line.contains('\u{1b}'), "{line:?}");
        if line.contains(&listening) {
            break;
        }
    }
    let health = std::net::TcpStream::connect(("127.0.0.1", port));
    assert!(health.is_ok());
    drop(health);

    let killed = Command::new("kill")
        .args(["-INT", &child.id().to_string()])
        .status()
        .unwrap();
    assert!(killed.success());
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    loop {
        if let Some(status) = child.try_wait().unwrap() {
            assert_eq!(Some(0), status.code());
            break;
        }
        assert!(std::time::Instant::now() < deadline, "server did not stop");
        std::thread::sleep(Duration::from_millis(50));
    }
    let rest: Vec<String> = lines.map_while(Result::ok).collect();
    assert!(
        rest.iter().any(|line| line.contains("Shutting down")),
        "{rest:?}"
    );
}

/// 연결을 받고 아무것도 보내지 않는 서버(멈춘 모델 다운로드). 받은 연결 수를 센다.
fn stalling_server() -> (u16, std::sync::Arc<std::sync::atomic::AtomicUsize>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let accepted = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let counter = accepted.clone();
    std::thread::spawn(move || {
        let mut held = Vec::new();
        for stream in listener.incoming().flatten() {
            counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            held.push(stream);
        }
    });
    (port, accepted)
}

fn wait_until_listening(child: &mut std::process::Child, port: u16) {
    let stderr = child.stderr.take().unwrap();
    let mut lines = BufReader::new(stderr).lines();
    let listening = format!("listening on http://127.0.0.1:{port}");
    loop {
        let line = lines
            .next()
            .expect("server exited before listening")
            .unwrap();
        if line.contains(&listening) {
            break;
        }
    }
    // 남은 로그는 버린다(파이프가 차서 서버가 멈추지 않게).
    std::thread::spawn(move || for _ in lines.map_while(Result::ok) {});
}

#[cfg(unix)]
#[test]
fn terminate_signal_exits_promptly_while_a_blocking_request_is_in_flight() {
    use std::io::Write;

    let dir = tempfile::tempdir().unwrap();
    let port = {
        let probe = TcpListener::bind("127.0.0.1:0").unwrap();
        probe.local_addr().unwrap().port()
    };
    let (model_port, model_requests) = stalling_server();
    let mut child = server_command(dir.path(), port)
        .env("OMYPIC_RAG_PROVIDER", "fasttext")
        .env(
            "OMYPIC_RAG_DOWNLOAD_URL",
            format!("http://127.0.0.1:{model_port}/cc.ko.300.vec.gz"),
        )
        .spawn()
        .unwrap();
    wait_until_listening(&mut child, port);

    // 룰북 업로드는 임베딩 모델을 내려받는 동안 유스케이스 안에서 멈춘다.
    let mut upload = std::net::TcpStream::connect(("127.0.0.1", port)).unwrap();
    let body = "--b\r\nContent-Disposition: form-data; name=\"file\"; filename=\"rules.md\"\r\n\r\n# rules\r\n--b--\r\n";
    write!(
        upload,
        "POST /api/rulebooks HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nContent-Type: multipart/form-data; boundary=b\r\nContent-Length: {}\r\n\r\n{body}",
        body.len()
    )
    .unwrap();
    let waiting = std::time::Instant::now();
    while model_requests.load(std::sync::atomic::Ordering::SeqCst) == 0 {
        assert!(
            waiting.elapsed() < Duration::from_secs(10),
            "upload never reached the model download"
        );
        std::thread::sleep(Duration::from_millis(20));
    }

    let killed = Command::new("kill")
        .args(["-TERM", &child.id().to_string()])
        .status()
        .unwrap();
    assert!(killed.success());
    let signalled = std::time::Instant::now();
    loop {
        if let Some(status) = child.try_wait().unwrap() {
            assert_eq!(Some(0), status.code());
            break;
        }
        if signalled.elapsed() > Duration::from_secs(15) {
            let _ = child.kill();
            panic!(
                "server still running {:?} after SIGTERM",
                signalled.elapsed()
            );
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    // 진행 중 요청을 기다리는 시간(5초)과 마무리 시간 안에 끝난다.
    assert!(
        signalled.elapsed() < Duration::from_secs(8),
        "{:?}",
        signalled.elapsed()
    );
    drop(upload);
}
