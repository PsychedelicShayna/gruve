//! Delivery directory: names, abbreviations' inputs, cycle, owned-temp point.
//! Never touches Python's `.<link>.tmp`.

use std::ffi::{OsStr, OsString};
use std::fs;
use std::io;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::symlink;
use std::path::{Path, PathBuf};
use std::time::Duration;

use bnuuy_rustblocks::widget::cycle_index;

pub struct Home {
    root: PathBuf,
    seq: u64,
}

pub enum LinkRead {
    Absent,
    Name(OsString),
    Failed,
}

impl Home {
    pub fn new(root: PathBuf) -> Self {
        Self { root, seq: 0 }
    }

    pub fn read_preset(&self) -> LinkRead {
        read_link_name(&self.root.join("active-preset"))
    }

    pub fn read_input(&self) -> LinkRead {
        read_link_name(&self.root.join("active-input-method"))
    }

    pub fn cycle_preset(&mut self, step: i32) -> io::Result<()> {
        let names = list_dir(&self.root.join("presets"), true)?;
        let mut list: Vec<Option<OsString>> = Vec::with_capacity(names.len() + 1);
        list.push(None);
        for name in names {
            list.push(Some(name));
        }
        let current = match self.read_preset() {
            LinkRead::Absent => Some(0usize),
            LinkRead::Name(name) => list.iter().position(|item| item.as_ref() == Some(&name)),
            LinkRead::Failed => {
                return Err(io::Error::other("readlink failed"));
            }
        };
        let Some(idx) = cycle_index(list.len(), current, step) else {
            return Ok(());
        };
        match &list[idx] {
            None => unlink_missing_ok(&self.root.join("active-preset")),
            Some(name) => self.point("active-preset", &rel_target("presets", name)),
        }
    }

    pub fn cycle_input(&mut self, step: i32) -> io::Result<()> {
        let names = list_dir(&self.root.join("input-methods"), false)?;
        let current = match self.read_input() {
            LinkRead::Absent => None,
            LinkRead::Name(name) => names.iter().position(|item| item == &name),
            LinkRead::Failed => {
                return Err(io::Error::other("readlink failed"));
            }
        };
        let Some(idx) = cycle_index(names.len(), current, step) else {
            return Ok(());
        };
        self.point(
            "active-input-method",
            &rel_target("input-methods", &names[idx]),
        )
    }

    fn point(&mut self, link_name: &str, target: &OsStr) -> io::Result<()> {
        self.seq = self.seq.saturating_add(1);
        let temp_name = format!(
            ".{link_name}.gruve.{}.{}",
            std::process::id(),
            self.seq
        );
        let temp = self.root.join(&temp_name);
        let link = self.root.join(link_name);
        if let Err(err) = symlink(target, &temp) {
            let _ = fs::remove_file(&temp);
            return Err(err);
        }
        if let Err(err) = test_pause(&temp) {
            let _ = fs::remove_file(&temp);
            return Err(err);
        }
        if let Err(err) = fs::rename(&temp, &link) {
            let _ = fs::remove_file(&temp);
            return Err(err);
        }
        Ok(())
    }
}

fn rel_target(dir: &str, name: &OsStr) -> OsString {
    let mut target = OsString::from(dir);
    target.push("/");
    target.push(name);
    target
}

/// `GRUVE_BLOCK_TEST_PAUSE=<path>`: after the owned temp symlink exists and
/// before rename, atomically publish that path (one line, absolute temp path,
/// trailing newline) and block until the harness unlinks it.
fn test_pause(temp: &Path) -> io::Result<()> {
    let Some(path) = std::env::var_os("GRUVE_BLOCK_TEST_PAUSE") else {
        return Ok(());
    };
    if path.is_empty() {
        return Ok(());
    }
    let path = PathBuf::from(path);
    let mut bytes = temp.as_os_str().as_bytes().to_vec();
    bytes.push(b'\n');
    let mut partial = path.as_os_str().to_os_string();
    partial.push(".partial");
    let partial = PathBuf::from(partial);
    fs::write(&partial, &bytes)?;
    fs::rename(&partial, &path)?;
    loop {
        match fs::symlink_metadata(&path) {
            Ok(_) => std::thread::sleep(Duration::from_millis(5)),
            Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(()),
            Err(err) => return Err(err),
        }
    }
}

fn read_link_name(path: &Path) -> LinkRead {
    let meta = match fs::symlink_metadata(path) {
        Ok(meta) => meta,
        Err(err) if err.kind() == io::ErrorKind::NotFound => return LinkRead::Absent,
        Err(_) => return LinkRead::Failed,
    };
    if !meta.file_type().is_symlink() {
        return LinkRead::Name(path.file_name().unwrap_or_default().to_os_string());
    }
    match fs::read_link(path) {
        Ok(target) => {
            let name = target
                .file_name()
                .unwrap_or(target.as_os_str())
                .to_os_string();
            LinkRead::Name(name)
        }
        Err(err) if err.kind() == io::ErrorKind::NotFound => LinkRead::Absent,
        Err(err) if err.raw_os_error() == Some(libc::EINVAL) => {
            LinkRead::Name(path.file_name().unwrap_or_default().to_os_string())
        }
        Err(_) => LinkRead::Failed,
    }
}

fn list_dir(dir: &Path, presets: bool) -> io::Result<Vec<OsString>> {
    let entries = match fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(err) => return Err(err),
    };
    let mut names = Vec::new();
    for entry in entries {
        let entry = entry?;
        let name = entry.file_name();
        if name.as_bytes().first() == Some(&b'.') {
            continue;
        }
        let path = entry.path();
        let keep = if presets {
            path.is_dir() || is_exec(&path)
        } else {
            is_exec(&path)
        };
        if keep {
            names.push(name);
        }
    }
    names.sort_by(|a, b| a.as_bytes().cmp(b.as_bytes()));
    Ok(names)
}

fn is_exec(path: &Path) -> bool {
    if !path.is_file() {
        return false;
    }
    let Ok(c_path) = std::ffi::CString::new(path.as_os_str().as_bytes()) else {
        return false;
    };
    unsafe { libc::access(c_path.as_ptr(), libc::X_OK) == 0 }
}

fn unlink_missing_ok(path: &Path) -> io::Result<()> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(err) => Err(err),
    }
}
