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
