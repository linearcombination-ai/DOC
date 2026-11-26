//! Configuration from the environment, with the names DOC's `.env` already uses so one file can
//! serve both services. Parsed once at start; every problem is reported together (fail fast).

use std::collections::HashMap;
use std::path::PathBuf;
use std::str::FromStr;
use std::time::Duration;

use doc_acquire::GitConfig;
use doc_app::AppConfig;
use doc_catalog::CatalogConfig;
use doc_contract::Mode;

#[derive(Debug)]
pub struct Config {
    pub addr: String,
    pub cors_origins: Vec<String>,
    pub log_json: bool,
    pub app: AppConfig,
    pub catalog: CatalogConfig,
    pub git: GitConfig,
}

#[derive(Debug, thiserror::Error)]
#[error("invalid configuration:\n  {}", .0.join("\n  "))]
pub struct ConfigError(pub Vec<String>);

struct Env<'a> {
    vars: &'a HashMap<String, String>,
    problems: Vec<String>,
}

impl Env<'_> {
    fn get(&self, name: &str) -> Option<&str> {
        self.vars.get(name).map(String::as_str).filter(|v| !v.trim().is_empty())
    }
    fn parse<T: FromStr>(&mut self, name: &str, default: T) -> T {
        match self.get(name) {
            None => default,
            Some(v) => match v.trim().parse::<T>() {
                Ok(x) => x,
                Err(_) => {
                    self.problems.push(format!("{name}={v:?} is not valid"));
                    default
                }
            },
        }
    }
    fn secs(&mut self, name: &str, default: u64) -> Duration {
        Duration::from_secs(self.parse(name, default))
    }
}

impl Config {
    pub fn from_vars(vars: &HashMap<String, String>) -> Result<Config, ConfigError> {
        let mut e = Env { vars, problems: Vec::new() };
        let data_api_url = match e.get("DATA_API_URL") {
            Some(u) => u.to_string(),
            None => {
                e.problems.push("DATA_API_URL is required".into());
                String::new()
            }
        };
        let raw_mode = e.get("DOC_SHELL_MODE").map(str::to_string);
        let mode = match raw_mode {
            None => Mode::Fixed,
            Some(m) => match Mode::parse(&m) {
                Some(mode) => mode,
                None => {
                    e.problems.push(format!("DOC_SHELL_MODE={m:?} must be as_built or fixed"));
                    Mode::AsBuilt
                }
            },
        };
        let name_rule = match e.get("DOC_BOOK_NAME_RULE").map(str::to_string) {
            None => doc_lookup::booklist::NameRule::Written,
            Some(r) if r == "written" => doc_lookup::booklist::NameRule::Written,
            Some(r) if r == "doc" => doc_lookup::booklist::NameRule::Doc,
            Some(r) => {
                e.problems.push(format!("DOC_BOOK_NAME_RULE={r:?} must be written or doc"));
                doc_lookup::booklist::NameRule::Written
            }
        };
        let user_agent = e.get("USER_AGENT_STR").unwrap_or("wa-doc").trim_matches('"').to_string();
        let xrw = e.get("X_REQUESTED_WITH_VALUE").unwrap_or("WA-Tool-Doc").trim_matches('"').to_string();
        let raw_cors = e.get("BACKEND_CORS_ORIGINS").map(str::to_string);
        let cors_origins: Vec<String> = match raw_cors {
            None => Vec::new(),
            Some(raw) => match serde_json::from_str::<Vec<String>>(raw.trim_matches('\'')) {
                Ok(v) => v,
                Err(err) => {
                    e.problems.push(format!("BACKEND_CORS_ORIGINS must be a JSON array of strings: {err}"));
                    Vec::new()
                }
            },
        };
        let mut catalog = CatalogConfig::new(&data_api_url);
        catalog.user_agent = user_agent.clone();
        catalog.x_requested_with = xrw.clone();
        catalog.ttl = e.secs("DATA_API_CACHE_TTL_SECONDS", 180);
        catalog.max_attempts = e.parse("DOC_SHELL_CATALOG_ATTEMPTS", catalog.max_attempts);
        catalog.attempt_timeout = e.secs("DOC_SHELL_CATALOG_TIMEOUT_SECS", catalog.attempt_timeout.as_secs());
        catalog.connect_timeout = e.secs("DOC_SHELL_CATALOG_CONNECT_TIMEOUT_SECS", catalog.connect_timeout.as_secs());
        catalog.refresh_ceiling = e.secs("DOC_SHELL_CATALOG_CEILING_SECS", catalog.refresh_ceiling.as_secs());
        let cache_dir = PathBuf::from(e.get("DOC_SHELL_CACHE_DIR").or(e.get("RESOURCE_ASSETS_DIR")).unwrap_or("assets_download_rs"));
        let mut git = GitConfig::new(cache_dir);
        git.user_agent = user_agent;
        git.x_requested_with = xrw;
        if let Some(bin) = e.get("DOC_SHELL_GIT_BIN") {
            git.git_bin = PathBuf::from(bin);
        }
        git.stale_after = Duration::from_secs(e.parse::<u64>("ASSET_CACHING_PERIOD", 30) * 60);
        git.clone_timeout = e.secs("DOC_SHELL_CLONE_TIMEOUT_SECS", git.clone_timeout.as_secs());
        git.max_attempts = e.parse("DOC_SHELL_CLONE_ATTEMPTS", git.max_attempts);
        git.max_parallel_clones = e.parse("DOC_SHELL_CLONE_PARALLEL", git.max_parallel_clones);
        git.fetch_ceiling = e.secs("DOC_SHELL_FETCH_CEILING_SECS", git.fetch_ceiling.as_secs());
        git.lock_lease = e.secs("DOC_SHELL_CLONE_LOCK_LEASE_SECS", git.lock_lease.as_secs());
        git.lock_wait = e.secs("DOC_SHELL_CLONE_LOCK_WAIT_SECS", git.lock_wait.as_secs());
        let app = AppConfig {
            default_mode: mode,
            request_deadline: e.secs("DOC_SHELL_REQUEST_DEADLINE_SECS", 300),
            as_built_failure_memory: catalog.ttl,
            name_rule,
        };
        let addr = e.get("DOC_SHELL_ADDR").unwrap_or("127.0.0.1:8081").to_string();
        let log_json = e.get("DOC_SHELL_LOG_FORMAT").is_some_and(|v| v.eq_ignore_ascii_case("json"));
        if e.problems.is_empty() {
            Ok(Config { addr, cors_origins, log_json, app, catalog, git })
        } else {
            Err(ConfigError(e.problems))
        }
    }

    pub fn from_env() -> Result<Config, ConfigError> {
        Config::from_vars(&std::env::vars().collect())
    }
}
