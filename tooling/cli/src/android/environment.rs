use crate::{Result, Runner, build::Environment, machine::Machine, require, workspace};
use std::path::{Path, PathBuf};

pub(super) struct Tools {
    pub sdk: PathBuf,
    pub java: Option<PathBuf>,
}

impl Tools {
    pub fn find(root: &Path, env: &Environment) -> Result<Self> {
        let home = env.get("HOME").map(PathBuf::from);
        let local = std::fs::read_to_string(root.join("android/local.properties"))
            .ok()
            .and_then(|text| {
                text.lines().find_map(|line| {
                    let (key, value) = line.trim().split_once('=')?;
                    (key.trim() == "sdk.dir").then(|| {
                        PathBuf::from(
                            value
                                .trim()
                                .replace("\\:", ":")
                                .replace("\\ ", " ")
                                .replace("\\\\", "\\"),
                        )
                    })
                })
            });
        let sdk = env
            .get("ANDROID_HOME")
            .or_else(|| env.get("ANDROID_SDK_ROOT"))
            .filter(|value| !value.is_empty())
            .map(PathBuf::from)
            .or(local)
            .or_else(|| {
                home.as_ref().map(|home| {
                    home.join(match Machine::THIS {
                        Machine::Mac => "Library/Android/sdk",
                        Machine::Linux => "Android/Sdk",
                    })
                })
            })
            .ok_or("Set ANDROID_HOME to the Android SDK directory.")?;
        let java = env
            .get("JAVA_HOME")
            .filter(|value| !value.is_empty())
            .map(PathBuf::from)
            .or_else(|| {
                home.map(|home| home.join(".sdkman/candidates/java/current"))
                    .filter(|java| java.join("bin/java").is_file())
            });
        Ok(Self { sdk, java })
    }

    pub fn adb(&self, args: &[&str], runner: &dyn Runner) -> Result<Vec<u8>> {
        let adb = self.sdk.join("platform-tools/adb");
        require(
            adb.is_file(),
            "ADB is missing: install Android SDK Platform-Tools in Android Studio, or correct ANDROID_HOME.",
        )?;
        let program = adb.to_str().ok_or("The SDK path is not UTF-8.")?;
        runner.command(&[&[program][..], args].concat(), None, 60)
    }

    pub fn java(&self, runner: &dyn Runner) -> Result<String> {
        let path = self.java.as_ref().map(|home| home.join("bin/java"));
        let program = path.as_deref().unwrap_or(Path::new("java"));
        // java --version writes to stdout, unlike older -version.
        let bytes = runner.command(
            &[
                program.to_str().ok_or("The Java path is not UTF-8.")?,
                "--version",
            ],
            None,
            15,
        )?;
        let text = String::from_utf8(bytes)?;
        let version = text.split_whitespace().nth(1).unwrap_or("");
        require(
            version.split('.').next() == Some("25"),
            "Android builds require Java 25: set JAVA_HOME to its installation.",
        )?;
        Ok(text.lines().next().unwrap_or("").to_owned())
    }

    pub fn gradle(&self, root: &Path, args: &[&str], runner: &dyn Runner) -> Result<Vec<u8>> {
        let mut command = vec![
            "env".to_owned(),
            format!("ANDROID_HOME={}", self.sdk.display()),
        ];
        if let Some(java) = &self.java {
            command.push(format!("JAVA_HOME={}", java.display()));
        }
        command.extend(["./gradlew", "--console=plain"].map(str::to_owned));
        command.extend(args.iter().map(|value| (*value).to_owned()));
        let log = tempfile::Builder::new()
            .prefix("android-gradle-")
            .suffix(".log")
            .tempfile_in(workspace::checkout_scratch(root)?)?
            .into_temp_path()
            .keep()?;
        let command = command
            .iter()
            .map(|value| workspace::quote(value))
            .collect::<Vec<_>>()
            .join(" ");
        let log_arg = workspace::quote(log.to_str().ok_or("The log path is not UTF-8.")?);
        let script = format!("{command} 2>&1 | tee {log_arg}");
        runner
            .command(
                &["bash", "-o", "pipefail", "-c", &script],
                Some(&root.join("android")),
                900,
            )
            .map_err(|error| format!("{error}\nGradle log: {}", log.display()).into())
    }
}

pub(super) fn doctor(root: &Path, env: &Environment, runner: &dyn Runner) -> Result<()> {
    let tools = Tools::find(root, env)?;
    let checks = [
        (
            "SDK",
            if tools.sdk.is_dir() {
                Ok(tools.sdk.display().to_string())
            } else {
                Err(
                    "SDK directory is missing: install it in Android Studio or set ANDROID_HOME."
                        .into(),
                )
            },
        ),
        ("Java", tools.java(runner)),
        (
            "ADB",
            tools.adb(&["version"], runner).and_then(|bytes| {
                Ok(String::from_utf8(bytes)?
                    .lines()
                    .next()
                    .unwrap_or("")
                    .to_owned())
            }),
        ),
        (
            "FFmpeg/libx264",
            runner
                .command(
                    &[
                        "ffmpeg",
                        "-v",
                        "error",
                        "-f",
                        "lavfi",
                        "-i",
                        "color=size=16x16:rate=1",
                        "-frames:v",
                        "1",
                        "-c:v",
                        "libx264",
                        "-f",
                        "null",
                        "-",
                    ],
                    None,
                    15,
                )
                .map(|_| "ready".to_owned())
                .map_err(|_| "Install FFmpeg with libx264 support for T3's Device panel.".into()),
        ),
    ];
    let mut passed = true;
    for (name, check) in checks {
        match check {
            Ok(value) => println!("{name}: {value}"),
            Err(error) => {
                eprintln!("{name}: {error}");
                passed = false;
            }
        }
    }
    require(passed, "Android readiness checks failed.")
}
