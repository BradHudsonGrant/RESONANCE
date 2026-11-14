use std::process::Command;

use sysinfo::{CpuRefreshKind, MemoryRefreshKind, RefreshKind, System};

const UNKNOWN: &str = "unknown";

#[derive(Debug, Clone, PartialEq)]
pub struct Machine {
    pub processor: String,
    pub logical_cores: usize,
    pub memory_bytes: u64,
    pub system: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Versions {
    pub resonance: String,
    /// The commit the binary was run from, marked when the working tree had changes. Markdown is
    /// left out, since the record itself is one of those files.
    pub commit: String,
    pub rustc: String,
}

pub fn machine() -> Machine {
    let refresh = RefreshKind::nothing()
        .with_cpu(CpuRefreshKind::nothing())
        .with_memory(MemoryRefreshKind::nothing().with_ram());
    let system = System::new_with_specifics(refresh).ok();
    let cpus = system.as_ref().map_or(&[][..], |system| system.cpus());
    let build = System::kernel_version().unwrap_or_else(|_| UNKNOWN.to_owned());
    Machine {
        processor: cpus
            .first()
            .map_or(UNKNOWN, |cpu| cpu.brand().trim())
            .to_owned(),
        logical_cores: cpus.len(),
        memory_bytes: system.as_ref().map_or(0, System::total_memory),
        system: format!(
            "{}, build {build}",
            System::long_os_version().unwrap_or_else(|_| UNKNOWN.to_owned())
        ),
    }
}

pub fn versions() -> Versions {
    let commit = output_of("git", &["rev-parse", "--short", "HEAD"]);
    let changed = output_of("git", &["status", "--porcelain", "--", ".", ":!*.md"])
        .is_some_and(|s| !s.is_empty());
    Versions {
        resonance: env!("CARGO_PKG_VERSION").to_owned(),
        commit: match (commit, changed) {
            (Some(commit), true) => format!("{commit} with uncommitted changes"),
            (Some(commit), false) => commit,
            (None, _) => UNKNOWN.to_owned(),
        },
        rustc: output_of("rustc", &["--version"]).unwrap_or_else(|| UNKNOWN.to_owned()),
    }
}

fn output_of(program: &str, args: &[&str]) -> Option<String> {
    let output = Command::new(program).args(args).output().ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).trim().to_owned())
}
