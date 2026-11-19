//! 데이터 폴더와 번들 임베딩 모델의 위치. 이전 데스크톱 앱(0.2.x)과 같은 규칙을 따른다.

use std::fs;
use std::path::{Path, PathBuf};

use omypic_infrastructure::config::AppConfig;
use serde::Deserialize;

/// `bundle.resources` 아래에서 임베딩 모델을 담는 폴더 이름.
const MODELS_DIR: &str = "models";
const MANIFEST_FILE: &str = "manifest.json";

/// 데이터 폴더: `env`(`OMYPIC_DATA_DIR`)가 있으면 그 값, 없으면 `<config_dir>/OMyPIc/omypic-data`.
/// 설정 폴더를 모르면 현재 폴더 아래에 둔다.
pub fn resolve_data_dir(env: Option<String>, config_dir: Option<PathBuf>) -> PathBuf {
    if let Some(dir) = env.filter(|value| !value.trim().is_empty()) {
        return PathBuf::from(dir);
    }
    config_dir
        .unwrap_or_else(|| PathBuf::from("."))
        .join("OMyPIc")
        .join("omypic-data")
}

/// 설치본에 들어 있는 임베딩 모델 파일.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BundledModel {
    pub path: PathBuf,
    /// `manifest.json`의 `filename`, 없으면 파일 이름.
    pub version: String,
    /// `manifest.json`의 `sha256`.
    pub sha256: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
struct Manifest {
    filename: Option<String>,
    sha256: Option<String>,
}

/// `<resource_dir>/models`에서 `manifest.json`을 뺀 첫 파일(이름순)을 고른다. 폴더나 파일이
/// 없으면 `None`. 깨진 `manifest.json`은 없는 것으로 본다.
pub fn resolve_bundled_embedding(resource_dir: &Path) -> Option<BundledModel> {
    let models_dir = resource_dir.join(MODELS_DIR);
    let mut names: Vec<String> = fs::read_dir(&models_dir)
        .ok()?
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_file()))
        .filter_map(|entry| entry.file_name().into_string().ok())
        .filter(|name| name != MANIFEST_FILE)
        .collect();
    names.sort();
    let selected = names.into_iter().next()?;
    let manifest: Manifest = fs::read_to_string(models_dir.join(MANIFEST_FILE))
        .ok()
        .and_then(|raw| serde_json::from_str(&raw).ok())
        .unwrap_or_default();
    Some(BundledModel {
        path: models_dir.join(&selected),
        version: manifest
            .filename
            .filter(|value| !value.is_empty())
            .unwrap_or(selected),
        sha256: manifest.sha256.filter(|value| !value.is_empty()),
    })
}

/// 번들 모델로 RAG 모델 경로/버전/해시를 채운다. `lookup`(환경 변수)에
/// `OMYPIC_RAG_MODEL_PATH`가 있으면 아무것도 바꾸지 않고, 버전과 해시는 환경 변수가 없을 때만 채운다.
pub fn apply_bundled_model(
    config: &mut AppConfig,
    bundled: Option<BundledModel>,
    lookup: impl Fn(&str) -> Option<String>,
) {
    let is_set = |name: &str| lookup(name).is_some_and(|value| !value.trim().is_empty());
    if is_set("OMYPIC_RAG_MODEL_PATH") {
        return;
    }
    let Some(model) = bundled else {
        return;
    };
    config.rag.model_path = model.path.to_string_lossy().into_owned();
    if !is_set("OMYPIC_RAG_MODEL_VERSION") {
        config.rag.model_version = model.version;
    }
    if let Some(sha256) = model.sha256 {
        if !is_set("OMYPIC_RAG_MODEL_SHA256") {
            config.rag.model_sha256 = sha256;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn data_dir_prefers_env() {
        let dir = resolve_data_dir(Some("/data/omypic".into()), Some(PathBuf::from("/c")));
        assert_eq!(PathBuf::from("/data/omypic"), dir);
    }

    #[test]
    fn data_dir_defaults_to_config_dir_omypic() {
        assert_eq!(
            PathBuf::from("/c/OMyPIc/omypic-data"),
            resolve_data_dir(None, Some(PathBuf::from("/c")))
        );
        assert_eq!(
            PathBuf::from("/c/OMyPIc/omypic-data"),
            resolve_data_dir(Some("  ".into()), Some(PathBuf::from("/c")))
        );
    }

    #[test]
    fn data_dir_falls_back_to_current_dir_when_no_config_dir() {
        assert_eq!(
            PathBuf::from("./OMyPIc/omypic-data"),
            resolve_data_dir(None, None)
        );
    }

    #[test]
    fn bundled_embedding_none_when_dir_missing() {
        let root = tempfile::tempdir().unwrap();
        assert_eq!(None, resolve_bundled_embedding(root.path()));
        fs::create_dir(root.path().join("models")).unwrap();
        fs::write(root.path().join("models/manifest.json"), "{}").unwrap();
        assert_eq!(None, resolve_bundled_embedding(root.path()));
    }

    #[test]
    fn bundled_embedding_picks_first_file_and_reads_manifest() {
        let root = tempfile::tempdir().unwrap();
        let models = root.path().join("models");
        fs::create_dir_all(models.join("a-dir")).unwrap();
        fs::write(models.join("b.vec.gz"), "b").unwrap();
        fs::write(models.join("a.vec.gz"), "a").unwrap();
        fs::write(
            models.join("manifest.json"),
            r#"{"filename":"cc.ko.300.vec.gz","sha256":"abc"}"#,
        )
        .unwrap();

        let model = resolve_bundled_embedding(root.path()).unwrap();

        assert_eq!(
            BundledModel {
                path: models.join("a.vec.gz"),
                version: "cc.ko.300.vec.gz".into(),
                sha256: Some("abc".into()),
            },
            model
        );
    }

    #[test]
    fn bundled_embedding_ignores_broken_manifest() {
        let root = tempfile::tempdir().unwrap();
        let models = root.path().join("models");
        fs::create_dir(&models).unwrap();
        fs::write(models.join("ko.vec.gz"), "a").unwrap();
        fs::write(models.join("manifest.json"), "{not json").unwrap();

        let model = resolve_bundled_embedding(root.path()).unwrap();

        assert_eq!(models.join("ko.vec.gz"), model.path);
        assert_eq!("ko.vec.gz", model.version);
        assert_eq!(None, model.sha256);
    }

    fn bundled() -> BundledModel {
        BundledModel {
            path: PathBuf::from("/res/models/ko.vec.gz"),
            version: "ko.vec.gz".into(),
            sha256: Some("abc".into()),
        }
    }

    #[test]
    fn bundled_model_fills_rag_settings() {
        let mut config = AppConfig::default();
        apply_bundled_model(&mut config, Some(bundled()), |_| None);
        assert_eq!(
            PathBuf::from("/res/models/ko.vec.gz").to_string_lossy(),
            config.rag.model_path
        );
        assert_eq!("ko.vec.gz", config.rag.model_version);
        assert_eq!("abc", config.rag.model_sha256);
    }

    #[test]
    fn env_rag_settings_win_over_bundled_model() {
        let defaults = AppConfig::default();

        let mut config = AppConfig::default();
        apply_bundled_model(&mut config, Some(bundled()), |name| {
            (name == "OMYPIC_RAG_MODEL_PATH").then(|| "/env/model".to_string())
        });
        assert_eq!(defaults.rag, config.rag);

        let mut config = AppConfig::default();
        apply_bundled_model(&mut config, Some(bundled()), |name| {
            matches!(name, "OMYPIC_RAG_MODEL_VERSION" | "OMYPIC_RAG_MODEL_SHA256")
                .then(|| "env".to_string())
        });
        assert_eq!(
            PathBuf::from("/res/models/ko.vec.gz").to_string_lossy(),
            config.rag.model_path
        );
        assert_eq!(defaults.rag.model_version, config.rag.model_version);
        assert_eq!(defaults.rag.model_sha256, config.rag.model_sha256);
    }

    #[test]
    fn no_bundled_model_keeps_config() {
        let mut config = AppConfig::default();
        apply_bundled_model(&mut config, None, |_| None);
        assert_eq!(AppConfig::default().rag, config.rag);
    }
}
