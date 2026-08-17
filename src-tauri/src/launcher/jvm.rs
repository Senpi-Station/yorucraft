use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Stdio;

use thiserror::Error;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::{Child, Command};

#[derive(Error, Debug)]
pub enum LauncherError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Failed to spawn game process: {0}")]
    SpawnFailed(String),
}

pub type Result<T> = std::result::Result<T, LauncherError>;

// ─── Game launch configuration ─────────────────────────────────────

#[derive(Debug, Clone)]
pub struct GameLaunchConfig {
    pub java_path: PathBuf,
    pub game_dir: PathBuf,
    pub assets_dir: PathBuf,
    pub libraries: Vec<PathBuf>,
    pub client_jar: PathBuf,
    pub username: String,
    pub uuid: String,
    pub access_token: String,
    pub user_type: String,
    pub version_id: String,
    pub version_name: String,
    pub main_class: String,
    pub jvm_args: Option<Vec<String>>,
    pub max_memory: String,
    pub min_memory: String,
    pub window_width: u32,
    pub window_height: u32,
    pub launcher_name: String,
    pub launcher_version: String,
    pub natives_dir: PathBuf,
    pub assets_index_name: String,
    pub demo: bool,
}

// ─── Child process wrapper ─────────────────────────────────────────

pub struct ChildProcess {
    pub pid: u32,
    stdout_handle: Option<tokio::task::JoinHandle<()>>,
    stderr_handle: Option<tokio::task::JoinHandle<()>>,
    child: Child,
}

impl ChildProcess {
    pub async fn wait(&mut self) -> Result<i32> {
        let status = self.child.wait().await?;
        if let Some(h) = self.stdout_handle.take() {
            let _ = h.await;
        }
        if let Some(h) = self.stderr_handle.take() {
            let _ = h.await;
        }
        Ok(status.code().unwrap_or(-1))
    }

    pub async fn kill(&mut self) -> Result<()> {
        self.child.kill().await?;
        Ok(())
    }

    pub fn is_running(&self) -> bool {
        self.child.try_wait().ok().flatten().is_none()
    }
}

// ─── JVM Launcher ──────────────────────────────────────────────────

pub struct JvmLauncher {
    config: GameLaunchConfig,
}

impl JvmLauncher {
    pub fn new(config: GameLaunchConfig) -> Self {
        Self { config }
    }

    pub fn build_command(&self) -> Result<Command> {
        let mut cmd = Command::new(&self.config.java_path);
        cmd.current_dir(&self.config.game_dir);
        cmd.env("APP_HOME", &self.config.game_dir);

        // Memory
        cmd.arg(format!("-Xmx{}", self.config.max_memory));
        cmd.arg(format!("-Xms{}", self.config.min_memory));
        cmd.arg("-Xss1M");

        // Native paths
        let natives = self.config.natives_dir.to_string_lossy();
        cmd.arg(format!("-Djava.library.path={}", natives));
        cmd.arg(format!("-Djna.tmpdir={}", natives));
        cmd.arg(format!(
            "-Dorg.lwjgl.system.SharedLibraryExtractPath={}",
            natives
        ));
        cmd.arg(format!("-Dio.netty.native.workdir={}", natives));

        // Launcher branding
        cmd.arg(format!(
            "-Dminecraft.launcher.brand={}",
            self.config.launcher_name
        ));
        cmd.arg(format!(
            "-Dminecraft.launcher.version={}",
            self.config.launcher_version
        ));

        // Security
        cmd.arg("-Dlog4j2.formatMsgNoLookups=true");

        // OS-specific
        if cfg!(target_os = "macos") {
            cmd.arg("-XstartOnFirstThread");
        }
        if cfg!(target_os = "windows") {
            cmd.arg("-XX:HeapDumpPath=MojangTricksIntelDriversForPerformance_javaw.exe_minecraft.exe.heapdump");
        }

        // User-defined JVM args
        if let Some(ref args) = self.config.jvm_args {
            for arg in args {
                cmd.arg(arg);
            }
        }

        // Classpath
        let separator = if cfg!(windows) { ";" } else { ":" };
        let mut classpath_parts: Vec<String> = self
            .config
            .libraries
            .iter()
            .map(|p| p.display().to_string())
            .collect();
        classpath_parts.push(self.config.client_jar.display().to_string());
        let classpath = classpath_parts.join(separator);
        cmd.arg("-cp").arg(&classpath);

        // Main class
        cmd.arg(&self.config.main_class);

        // Game arguments
        let game_args = self.build_game_args();
        for arg in game_args {
            cmd.arg(arg);
        }

        // Pipe settings
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());

        Ok(cmd)
    }

    pub fn build_game_args(&self) -> Vec<String> {
        let mut vars = HashMap::new();
        vars.insert(
            "auth_player_name".to_string(),
            self.config.username.clone(),
        );
        vars.insert("version_name".to_string(), self.config.version_name.clone());
        vars.insert(
            "game_directory".to_string(),
            self.config.game_dir.display().to_string(),
        );
        vars.insert(
            "assets_root".to_string(),
            self.config.assets_dir.display().to_string(),
        );
        vars.insert(
            "assets_index_name".to_string(),
            self.config.assets_index_name.clone(),
        );
        vars.insert(
            "auth_access_token".to_string(),
            self.config.access_token.clone(),
        );
        vars.insert("auth_uuid".to_string(), self.config.uuid.clone());
        vars.insert("user_type".to_string(), self.config.user_type.clone());
        vars.insert(
            "resolution_width".to_string(),
            self.config.window_width.to_string(),
        );
        vars.insert(
            "resolution_height".to_string(),
            self.config.window_height.to_string(),
        );
        vars.insert("version_type".to_string(), "release".to_string());
        vars.insert(
            "launcher_name".to_string(),
            self.config.launcher_name.clone(),
        );
        vars.insert(
            "launcher_version".to_string(),
            self.config.launcher_version.clone(),
        );
        vars.insert("user_properties".to_string(), "{}".to_string());

        let mut args = vec![
            "--username".to_string(),
            self.config.username.clone(),
            "--version".to_string(),
            self.config.version_name.clone(),
            "--gameDir".to_string(),
            self.config.game_dir.display().to_string(),
            "--assetsDir".to_string(),
            self.config.assets_dir.display().to_string(),
            "--assetIndex".to_string(),
            self.config.assets_index_name.clone(),
            "--accessToken".to_string(),
            self.config.access_token.clone(),
            "--uuid".to_string(),
            self.config.uuid.clone(),
            "--userType".to_string(),
            self.config.user_type.clone(),
            "--width".to_string(),
            self.config.window_width.to_string(),
            "--height".to_string(),
            self.config.window_height.to_string(),
            "--versionType".to_string(),
            "release".to_string(),
        ];

        if self.config.demo {
            args.push("--demo".to_string());
        }

        args
    }

    pub fn interpolate(template: &str, vars: &HashMap<String, String>) -> String {
        let mut result = template.to_string();
        for (key, value) in vars {
            result = result.replace(&format!("${{{}}}", key), value);
        }
        result
    }

    pub async fn spawn_game(&self) -> Result<ChildProcess> {
        let mut cmd = self.build_command()?;
        let mut child = cmd
            .spawn()
            .map_err(|e| LauncherError::SpawnFailed(e.to_string()))?;

        let pid = child.id().unwrap_or(0);

        let stdout_handle = child.stdout.take().map(|stdout| {
            tokio::spawn(async move {
                let mut lines = BufReader::new(stdout).lines();
                while let Ok(Some(line)) = lines.next_line().await {
                    log::info!("[GAME] {}", line);
                }
            })
        });

        let stderr_handle = child.stderr.take().map(|stderr| {
            tokio::spawn(async move {
                let mut lines = BufReader::new(stderr).lines();
                while let Ok(Some(line)) = lines.next_line().await {
                    log::error!("[GAME] {}", line);
                }
            })
        });

        Ok(ChildProcess {
            pid,
            stdout_handle,
            stderr_handle,
            child,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_interpolate() {
        let mut vars = HashMap::new();
        vars.insert("name".to_string(), "Steve".to_string());
        vars.insert("version".to_string(), "1.21.4".to_string());

        let result = JvmLauncher::interpolate("Hello ${name}, version ${version}", &vars);
        assert_eq!(result, "Hello Steve, version 1.21.4");
    }

    #[test]
    fn test_interpolate_missing_key() {
        let vars = HashMap::new();
        let result = JvmLauncher::interpolate("Hello ${name}", &vars);
        assert_eq!(result, "Hello ${name}");
    }

    #[test]
    fn test_build_game_args() {
        let config = GameLaunchConfig {
            java_path: PathBuf::from("/usr/bin/java"),
            game_dir: PathBuf::from("/home/user/.minecraft"),
            assets_dir: PathBuf::from("/home/user/.minecraft/assets"),
            libraries: vec![PathBuf::from("/libs/a.jar")],
            client_jar: PathBuf::from("/versions/1.21.4/client.jar"),
            username: "Steve".to_string(),
            uuid: "abc123".to_string(),
            access_token: "token".to_string(),
            user_type: "msa".to_string(),
            version_id: "1.21.4".to_string(),
            version_name: "1.21.4".to_string(),
            main_class: "net.minecraft.client.main.Main".to_string(),
            jvm_args: None,
            max_memory: "4G".to_string(),
            min_memory: "4G".to_string(),
            window_width: 854,
            window_height: 480,
            launcher_name: "YoruCraft".to_string(),
            launcher_version: "1.0.0".to_string(),
            natives_dir: PathBuf::from("/natives"),
            assets_index_name: "1234".to_string(),
            demo: false,
        };

        let launcher = JvmLauncher::new(config);
        let args = launcher.build_game_args();
        assert!(args.contains(&"--username".to_string()));
        assert!(args.contains(&"Steve".to_string()));
        assert!(!args.contains(&"--demo".to_string()));
    }

    #[test]
    fn test_build_game_args_demo() {
        let config = GameLaunchConfig {
            java_path: PathBuf::from("/usr/bin/java"),
            game_dir: PathBuf::from("/home/user/.minecraft"),
            assets_dir: PathBuf::from("/home/user/.minecraft/assets"),
            libraries: vec![PathBuf::from("/libs/a.jar")],
            client_jar: PathBuf::from("/versions/1.21.4/client.jar"),
            username: "Steve".to_string(),
            uuid: "abc123".to_string(),
            access_token: "token".to_string(),
            user_type: "msa".to_string(),
            version_id: "1.21.4".to_string(),
            version_name: "1.21.4".to_string(),
            main_class: "net.minecraft.client.main.Main".to_string(),
            jvm_args: None,
            max_memory: "4G".to_string(),
            min_memory: "4G".to_string(),
            window_width: 854,
            window_height: 480,
            launcher_name: "YoruCraft".to_string(),
            launcher_version: "1.0.0".to_string(),
            natives_dir: PathBuf::from("/natives"),
            assets_index_name: "1234".to_string(),
            demo: true,
        };

        let launcher = JvmLauncher::new(config);
        let args = launcher.build_game_args();
        assert!(args.contains(&"--demo".to_string()));
    }
}
