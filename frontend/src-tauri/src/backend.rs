//! 앱 프로세스 안에서 도는 백엔드 서버. 자기 tokio 런타임을 갖고, 끝낼 때 그 런타임을
//! `RUNTIME_SHUTDOWN_TIMEOUT`으로 닫아 멈춘 블로킹 작업이 프로세스 종료를 막지 않게 한다.

use std::net::{Ipv4Addr, SocketAddr};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use anyhow::Context;
use omypic_infrastructure::config::AppConfig;
use omypic_server::{RunningServer, RUNTIME_SHUTDOWN_TIMEOUT};

use crate::paths;

/// 환경 변수(`lookup`)로 설정을 읽고, 데이터 폴더(`OMYPIC_DATA_DIR`가 없으면
/// `<config_dir>/omypic-frontend/omypic-data`)와 번들 임베딩 모델을 채운다.
pub fn build_config(
    lookup: impl Fn(&str) -> Option<String>,
    config_dir: Option<PathBuf>,
    resource_dir: Option<&Path>,
) -> anyhow::Result<AppConfig> {
    let mut config = AppConfig::from_env_with(&lookup).map_err(|err| anyhow::anyhow!("{err}"))?;
    config.data_dir = paths::resolve_data_dir(lookup("OMYPIC_DATA_DIR"), config_dir)
        .to_string_lossy()
        .into_owned();
    let bundled = resource_dir.and_then(paths::resolve_bundled_embedding);
    paths::apply_bundled_model(&mut config, bundled, &lookup);
    Ok(config)
}

struct Embedded {
    runtime: tokio::runtime::Runtime,
    server: RunningServer,
}

/// 프런트엔드가 부르는 백엔드. 내장 서버이거나(`start`) 밖에서 띄운 서버(`external`)다.
pub struct Backend {
    url: String,
    embedded: Mutex<Option<Embedded>>,
}

impl Backend {
    /// 내장 서버를 `127.0.0.1`의 빈 포트에 띄운다. 돌아오면 서버가 요청을 받는다.
    pub fn start(config: AppConfig) -> anyhow::Result<Self> {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .thread_name("omypic-backend")
            .build()
            .context("Failed to start async runtime")?;
        let bind = SocketAddr::from((Ipv4Addr::LOCALHOST, 0));
        let server = match runtime.block_on(omypic_server::start(config, bind)) {
            Ok(server) => server,
            Err(err) => {
                runtime.shutdown_timeout(RUNTIME_SHUTDOWN_TIMEOUT);
                return Err(err);
            }
        };
        Ok(Self {
            url: format!("http://{}", server.addr),
            embedded: Mutex::new(Some(Embedded { runtime, server })),
        })
    }

    /// `OMYPIC_BACKEND_URL`로 밖에서 띄운 서버(개발용).
    pub fn external(url: String) -> Self {
        Self {
            url,
            embedded: Mutex::new(None),
        }
    }

    pub fn url(&self) -> &str {
        &self.url
    }

    /// 백엔드를 띄운 뒤의 시작 단계(`step`, 예: 창 보이기)를 부른다. 실패하면 백엔드를 끄고
    /// 그 오류를 돌려준다.
    pub fn finish_startup(&self, step: impl FnOnce() -> anyhow::Result<()>) -> anyhow::Result<()> {
        step().inspect_err(|_| self.shutdown())
    }

    /// 내장 서버를 끄고 런타임을 닫는다. 최대 약 7초 걸린다: `SHUTDOWN_GRACE`(5초) +
    /// `RELEASE_GRACE`(1초) + `RUNTIME_SHUTDOWN_TIMEOUT`(1초). 두 번째 호출부터는 아무것도 하지
    /// 않는다. 비동기 런타임 안에서 부르면 안 된다.
    pub fn shutdown(&self) {
        let embedded = self
            .embedded
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .take();
        let Some(Embedded { runtime, server }) = embedded else {
            return;
        };
        tracing::info!("Shutting down the backend");
        if let Err(err) = runtime.block_on(server.shutdown()) {
            tracing::error!("Backend shutdown failed: {err:#}");
        }
        runtime.shutdown_timeout(RUNTIME_SHUTDOWN_TIMEOUT);
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::time::Duration;

    use super::*;

    fn lookup(vars: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> {
        let vars: HashMap<String, String> = vars
            .iter()
            .map(|(name, value)| (name.to_string(), value.to_string()))
            .collect();
        move |name| vars.get(name).cloned()
    }

    #[test]
    fn config_uses_config_dir_and_bundled_model() {
        let resources = tempfile::tempdir().unwrap();
        std::fs::create_dir(resources.path().join("models")).unwrap();
        std::fs::write(resources.path().join("models/ko.vec.gz"), "m").unwrap();

        let config = build_config(
            lookup(&[]),
            Some(PathBuf::from("/c")),
            Some(resources.path()),
        )
        .unwrap();

        assert_eq!(
            PathBuf::from("/c/omypic-frontend/omypic-data").to_string_lossy(),
            config.data_dir
        );
        assert_eq!(
            resources.path().join("models/ko.vec.gz").to_string_lossy(),
            config.rag.model_path
        );
        assert_eq!("ko.vec.gz", config.rag.model_version);
    }

    #[test]
    fn config_prefers_env_data_dir() {
        let config = build_config(
            lookup(&[("OMYPIC_DATA_DIR", "/env/data")]),
            Some(PathBuf::from("/c")),
            None,
        )
        .unwrap();
        assert_eq!("/env/data", config.data_dir);
    }

    #[test]
    fn invalid_env_is_a_config_error() {
        let err = build_config(lookup(&[("OMYPIC_PORT", "nope")]), None, None).unwrap_err();
        assert!(err.to_string().contains("OMYPIC_PORT"), "{err}");
    }

    #[test]
    fn embedded_backend_answers_and_stops() {
        let data = tempfile::tempdir().unwrap();
        let config = AppConfig {
            data_dir: data.path().to_string_lossy().into_owned(),
            ..AppConfig::default()
        };

        let backend = Backend::start(config).unwrap();
        let health = format!("{}/actuator/health", backend.url());
        assert!(backend.url().starts_with("http://127.0.0.1:"));
        let client = reqwest::blocking::Client::builder()
            .no_proxy()
            .timeout(Duration::from_secs(5))
            .build()
            .unwrap();
        let response = client.get(&health).send().unwrap();
        assert!(response.status().is_success(), "{}", response.status());

        backend.shutdown();
        backend.shutdown();

        assert!(client.get(&health).send().is_err());
    }

    #[test]
    fn failed_startup_step_stops_the_backend() {
        let data = tempfile::tempdir().unwrap();
        let config = AppConfig {
            data_dir: data.path().to_string_lossy().into_owned(),
            ..AppConfig::default()
        };
        let backend = Backend::start(config).unwrap();
        let health = format!("{}/actuator/health", backend.url());
        let client = reqwest::blocking::Client::builder()
            .no_proxy()
            .timeout(Duration::from_secs(5))
            .build()
            .unwrap();

        let err = backend
            .finish_startup(|| Err(anyhow::anyhow!("cannot show the window")))
            .unwrap_err();

        assert_eq!("cannot show the window", err.to_string());
        assert!(client.get(&health).send().is_err());
    }

    #[test]
    fn successful_startup_step_keeps_the_backend_running() {
        let data = tempfile::tempdir().unwrap();
        let config = AppConfig {
            data_dir: data.path().to_string_lossy().into_owned(),
            ..AppConfig::default()
        };
        let backend = Backend::start(config).unwrap();
        let health = format!("{}/actuator/health", backend.url());
        let client = reqwest::blocking::Client::builder()
            .no_proxy()
            .timeout(Duration::from_secs(5))
            .build()
            .unwrap();

        backend.finish_startup(|| Ok(())).unwrap();

        assert!(client.get(&health).send().unwrap().status().is_success());
        backend.shutdown();
    }

    #[test]
    fn external_backend_has_nothing_to_stop() {
        let backend = Backend::external("http://localhost:4317".into());
        assert_eq!("http://localhost:4317", backend.url());
        backend.shutdown();
    }
}
