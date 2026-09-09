use std::{fs, os::unix::fs::MetadataExt, path::PathBuf};

/// Process evidence stays inside the adapter. Never send paths or PIDs to a
/// WebView, and never inspect command lines, environment, or media titles.
#[derive(Clone, Debug)]
pub(crate) struct ProcessIdentity {
    pub executable: String,
    pub instance: String,
}

struct Process {
    parent: u32,
    started: u64,
    executable: PathBuf,
}

fn stat_fields(stat: &str) -> Option<(u32, u64)> {
    // comm can contain spaces and closing parentheses. Fields after the final
    // ') ' start at field 3 (state); starttime is field 22.
    let (_, fields) = stat.rsplit_once(") ")?;
    let fields: Vec<_> = fields.split_whitespace().collect();
    Some((fields.get(1)?.parse().ok()?, fields.get(19)?.parse().ok()?))
}

fn process(pid: u32, uid: u32) -> Option<Process> {
    let directory = PathBuf::from(format!("/proc/{pid}"));
    if fs::metadata(&directory).ok()?.uid() != uid {
        return None;
    }
    let first = fs::read_to_string(directory.join("stat")).ok()?;
    let (parent, started) = stat_fields(&first)?;
    let executable = fs::read_link(directory.join("exe")).ok()?;
    let second = fs::read_to_string(directory.join("stat")).ok()?;
    if stat_fields(&second)? != (parent, started) {
        return None; // Process exit/PID reuse while collecting evidence.
    }
    Some(Process {
        parent,
        started,
        executable,
    })
}

pub(crate) fn resolve_process(pid: u32, uid: u32) -> Option<ProcessIdentity> {
    let current = process(pid, uid)?;
    let mut instance = (pid, current.started);
    let mut parent = current.parent;
    // Browser/electron audio children commonly share the executable with their
    // application root. An unrelated parent shell does not merge applications.
    for _ in 0..64 {
        if parent <= 1 || parent == instance.0 {
            break;
        }
        let Some(ancestor) = process(parent, uid) else {
            break;
        };
        if ancestor.executable == current.executable {
            instance = (parent, ancestor.started);
        }
        if ancestor.parent == parent {
            break;
        }
        parent = ancestor.parent;
    }
    Some(ProcessIdentity {
        executable: current.executable.to_string_lossy().into_owned(),
        instance: format!("{}:{}", instance.0, instance.1),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_process_start_time_without_splitting_the_command_name() {
        let fields: Vec<_> = (3..=22).map(|n| n.to_string()).collect();
        let stat = format!("12 (name with ) spaces) {}", fields.join(" "));
        assert_eq!(stat_fields(&stat), Some((4, 22)));
        assert!(stat_fields("12 (incomplete)").is_none());
    }
}
