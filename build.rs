use chrono::Local;
use semver::Version;
use serde_json::json;
use std::env;
use std::fs;
use std::path::Path;
use std::process::Command;
use toml_edit::{DocumentMut, value}; // 新增

fn main() {
    println!("cargo:warning=开始获取全部 git commits...");

    // ---------- 1. 获取全部 commits ----------
    let output = Command::new("git")
        .args([
            "log",
            "--all",
            "--reverse",
            "--format=%H%x1f%an%x1f%ae%x1f%ad%x1f%B%x1e",
        ])
        .output()
        .expect("failed to execute git log");

    if !output.status.success() {
        panic!(
            "git log failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    let stdout = String::from_utf8(output.stdout).expect("invalid utf8 from git log");
    let mut commits = Vec::new();

    for record in stdout.split('\x1e') {
        if record.trim().is_empty() {
            continue;
        }
        let fields: Vec<&str> = record.split('\x1f').collect();
        if fields.len() >= 5 {
            commits.push(json!({
                "hash": fields[0],
                "author": fields[1],
                "email": fields[2],
                "date": fields[3],
                "message": fields[4].trim(),
            }));
        }
    }

    let commit_count = commits.len();
    println!("cargo:warning=共获取 {} 条 commit 记录", commit_count);

    // ---------- 2. 版本号处理（仅在 release 模式下更新）----------
    let profile = env::var("PROFILE").unwrap();
    let is_release = profile == "release";

    let final_version = if is_release {
        let cargo_toml_path = Path::new("Cargo.toml");
        let original = fs::read_to_string(cargo_toml_path).expect("Failed to read Cargo.toml");
        let doc = original
            .parse::<DocumentMut>()
            .expect("Failed to parse Cargo.toml");
        let current_version_str = doc["package"]["version"]
            .as_str()
            .expect("version field is not a string");

        let mut current_version =
            Version::parse(current_version_str).expect("Failed to parse version");
        current_version.patch += 1;
        let new_version = current_version.to_string();

        let mut doc_mut = original.parse::<DocumentMut>().unwrap();
        doc_mut["package"]["version"] = value(new_version.clone());
        fs::write(cargo_toml_path, doc_mut.to_string()).expect("Failed to write Cargo.toml");
        println!(
            "cargo:warning=已将版本从 {} 更新为 {}",
            current_version_str, new_version
        );

        new_version
    } else {
        let cargo_toml_path = Path::new("Cargo.toml");
        let original = fs::read_to_string(cargo_toml_path).unwrap();
        let doc = original.parse::<DocumentMut>().unwrap();
        doc["package"]["version"].as_str().unwrap().to_string()
    };

    // ---------- 3. 获取打包时间（UTC 时间，ISO 8601 格式）----------
    let build_time = Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
    println!("cargo:warning=打包时间: {}", build_time);

    // ---------- 4. 写入 JSON（包含 commits、版本号、打包时间）----------
    let json_output = json!({
        "commits": commits,
        "count": commit_count,
        "version": final_version,
        "build_time": build_time,      // 新增字段
    });

    let out_dir = env::var("OUT_DIR").unwrap();
    let dest_path = Path::new(&out_dir).join("git_commits.json");
    let json_str = serde_json::to_string_pretty(&json_output).expect("Failed to serialize JSON");
    fs::write(&dest_path, json_str).expect("Failed to write git_commits.json");

    println!("cargo:warning=已写入 JSON 文件: {}", dest_path.display());

    // ---------- 5. rerun 条件 ----------
    println!("cargo:rerun-if-changed=.git/HEAD");
    println!("cargo:rerun-if-changed=.git/logs/HEAD");
    println!("cargo:rerun-if-changed=Cargo.toml");
}
