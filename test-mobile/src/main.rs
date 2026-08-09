#![feature(exit_status_error)]

use std::{
    fmt::Display,
    fs::{File, copy, create_dir, read_dir, read_to_string, remove_dir_all, remove_file},
    io::Write,
    path::{Path, PathBuf},
    process::Command,
};

use anyhow::{Result, anyhow, bail};
use convert_case::{Case, Casing};
use structopt::StructOpt;
use toml::Value;

fn copy_file(names: &Names, src: &Path, dest: &Path) -> Result<()> {
    if src.to_string_lossy().contains(".DS_Store") {
        return Ok(());
    }

    let Ok(content) = read_to_string(src) else {
        copy(src, dest)?;
        return Ok(());
    };

    let content = names.replace_string(content);

    let mut file = File::create(dest)?;
    file.write_all(content.as_bytes())?;

    Ok(())
}

fn copy_dir(names: &Names, src: &Path, dest: &Path) -> Result<()> {
    if !dest.exists() {
        create_dir(dest)?;
    }
    for entry in read_dir(src)? {
        let entry = entry?;
        let src_path = entry.path();
        let dest_path = dest.join(entry.file_name()).display().to_string();

        if dest_path.contains("xcuserdata") {
            continue;
        }

        let dest_path: PathBuf = names.replace_string(dest_path).into();

        if src_path.is_dir() {
            copy_dir(names, &src_path, &dest_path)?;
        } else {
            copy_file(names, &src_path, &dest_path)?;
        }
    }
    Ok(())
}

const SNAKE_REPLACE: &str = "TEST_MOBILE_PROJECT_NAME_SNAKE_CASE";
const CAMEL_REPLACE: &str = "TEST_MOBILE_PROJECT_NAME_CAMEL_CASE";
const KEBAB_REPLACE: &str = "TEST_MOBILE_PROJECT_NAME_KEBAB_CASE";
const TITLE_REPLACE: &str = "TEST_MOBILE_PROJECT_NAME_TITLE_CASE";
const BUNDLE_REPLACE: &str = "TEST_MOBILE_BINDLE_IDENTIFIER";
const LIB_REPLACE: &str = "TEST_MOBILE_LIB_NAME";
const CARGO_TARGET: &str = "TEST_MOBILE_CARGO_TARGET";
const CARGO_PROFILE: &str = "TEST_MOBILE_CARGO_PROFILE";

const REPO: &str = "https://github.com/hilen/test-mobile";
const REPO_TEMP: &str = "_test_mobile_temp";

#[derive(Debug)]
struct Names {
    camel:   String,
    snake:   String,
    kebab:   String,
    title:   String,
    lib:     String,
    bundle:  String,
    target:  String,
    profile: String,
}

impl Names {
    fn replace_string(&self, string: impl Display) -> String {
        let string = format!("{string}");
        let string = string.replace(LIB_REPLACE, &self.lib);
        let string = string.replace(SNAKE_REPLACE, &self.snake);
        let string = string.replace(CAMEL_REPLACE, &self.camel);
        let string = string.replace(TITLE_REPLACE, &self.title);
        let string = string.replace(KEBAB_REPLACE, &self.kebab);
        let string = string.replace(BUNDLE_REPLACE, &self.bundle);
        let string = string.replace(CARGO_TARGET, &self.target);
        string.replace(CARGO_PROFILE, &self.profile)
    }
}

// Plain git instead of the git2 crate. libgit2 drags a C toolchain into
// every `cargo install test-mobile`, and a broken host cc breaks installs.
fn clone_repo(repo_url: &str, dest_path: &str) -> Result<()> {
    let repo_path = Path::new(dest_path);
    if repo_path.exists() {
        remove_dir_all(repo_path)?;
    }
    Command::new("git")
        .args(["clone", "--depth", "1", repo_url, dest_path])
        .status()?
        .exit_ok()?;
    Ok(())
}

struct TempDir {
    path: &'static str,
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = remove_dir_all(self.path);
    }
}

#[derive(StructOpt, Debug)]
struct Args {
    /// Path to template
    #[structopt(long, short)]
    path: Option<PathBuf>,

    ///Cargo profile
    #[structopt(long, default_value = "release")]
    profile: String,
}

fn main() -> Result<()> {
    let args = Args::from_args();

    let project_info = read_to_string("test-engine.toml")
        .or_else(|_| bail!("Please put \'test-engine.toml' file with project info at the project root."))?;

    let project_info: Value = project_info.parse()?;

    let project_name = project_info["project_name"]
        .as_str()
        .ok_or(anyhow!("project_name not found in test-engine.toml"))?;

    let temp_dir = TempDir { path: REPO_TEMP };

    let template_path = if let Some(path) = args.path {
        path
    } else {
        clone_repo(REPO, REPO_TEMP)?;
        Path::new(REPO_TEMP).join("mobile-template")
    };

    let _ = remove_dir_all("mobile");

    let names = Names {
        camel:   project_name.to_case(Case::UpperCamel),
        snake:   project_name.to_case(Case::Snake),
        kebab:   project_name.to_case(Case::Kebab),
        title:   project_name.to_case(Case::Title),
        lib:     format!("lib{}.a", project_name.to_case(Case::Snake)),
        bundle:  project_info["bundle_id"].as_str().unwrap().to_string(),
        target:  "aarch64-apple-ios".to_string(),
        profile: args.profile,
    };

    let dest = Path::new("mobile");

    copy_dir(&names, &template_path, dest)?;

    let app_icon_path = PathBuf::from("Assets/AppIcon.appiconset");

    if app_icon_path.exists() {
        let target_app_icon_path = PathBuf::from(format!(
            "mobile/iOS/{}/Assets.xcassets/AppIcon.appiconset",
            names.camel
        ));

        let _ = remove_dir_all(&target_app_icon_path);

        copy_dir(&names, &app_icon_path, &target_app_icon_path)?;
    }

    let launch_storyboard_path = PathBuf::from("Assets/LaunchScreen.storyboard");

    if launch_storyboard_path.exists() {
        let target_launch_storyboard_path = PathBuf::from(format!(
            "mobile/iOS/{}/Base.lproj/LaunchScreen.storyboard",
            names.camel
        ));

        let _ = remove_file(&target_launch_storyboard_path);

        copy_file(&names, &launch_storyboard_path, &target_launch_storyboard_path)?;
    }

    drop(temp_dir);

    #[cfg(not(target_os = "windows"))]
    std::process::Command::new("chmod")
        .args(["+x", "./mobile/android/gradlew"])
        .output()?
        .status
        .exit_ok()?;

    println!(
        "iOS and Android projects for {} have been generated successfully.",
        names.camel
    );

    Ok(())
}
