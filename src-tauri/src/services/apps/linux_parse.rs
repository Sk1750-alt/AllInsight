//! Pure parsers for the Linux package and desktop-entry formats.
//!
//! Nothing here touches the filesystem, so every parser is compiled and
//! tested on every platform, against fixtures copied from real Debian, Arch
//! and Fedora systems. The code that finds and reads the files lives in
//! `linux.rs` and in the startup service.

use std::collections::HashMap;

/// The keys of one group of a freedesktop `.desktop` file, unlocalised.
///
/// Localised keys (`Name[de]=`) and comments are ignored, and only the first
/// occurrence of a key counts, which is what the specification requires.
pub fn desktop_group(text: &str, group: &str) -> HashMap<String, String> {
    let header = format!("[{group}]");
    let mut out = HashMap::new();
    let mut inside = false;
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if line.starts_with('[') {
            inside = line == header;
            continue;
        }
        if !inside {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let key = key.trim();
        if key.contains('[') {
            continue;
        }
        out.entry(key.to_string())
            .or_insert_with(|| value.trim().to_string());
    }
    out
}

/// Set `key=value` in the `[Desktop Entry]` group, replacing the first
/// unlocalised occurrence or inserting it straight after the group header.
/// Every other line, including comments and other groups, is kept verbatim.
pub fn set_desktop_key(text: &str, key: &str, value: &str) -> String {
    let mut out: Vec<String> = Vec::new();
    let mut inside = false;
    let mut header_at: Option<usize> = None;
    let mut done = false;
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') {
            inside = trimmed == "[Desktop Entry]";
            if inside && header_at.is_none() {
                header_at = Some(out.len());
            }
        } else if inside && !done {
            if let Some((k, _)) = trimmed.split_once('=') {
                if k.trim() == key {
                    out.push(format!("{key}={value}"));
                    done = true;
                    continue;
                }
            }
        }
        out.push(line.to_string());
    }
    if !done {
        match header_at {
            Some(at) => out.insert(at + 1, format!("{key}={value}")),
            None => {
                out.insert(0, format!("{key}={value}"));
                out.insert(0, "[Desktop Entry]".to_string());
            }
        }
    }
    let mut joined = out.join("\n");
    joined.push('\n');
    joined
}

/// Whether an entry applies to the running desktop, from `OnlyShowIn` and
/// `NotShowIn` against the `XDG_CURRENT_DESKTOP` list.
pub fn shown_in(group: &HashMap<String, String>, current_desktop: &str) -> bool {
    let current: Vec<&str> = current_desktop
        .split(':')
        .filter(|d| !d.is_empty())
        .collect();
    let listed = |key: &str| -> Option<Vec<String>> {
        group.get(key).map(|v| {
            v.split(';')
                .filter(|d| !d.is_empty())
                .map(|d| d.to_string())
                .collect()
        })
    };
    if let Some(only) = listed("OnlyShowIn") {
        // With no desktop reported there is nothing to compare against, and an
        // entry that would run somewhere is shown rather than hidden.
        if !current.is_empty() && !only.iter().any(|d| current.contains(&d.as_str())) {
            return false;
        }
    }
    if let Some(not) = listed("NotShowIn") {
        if not.iter().any(|d| current.contains(&d.as_str())) {
            return false;
        }
    }
    true
}

/// A desktop entry reduced to what the application and startup lists need.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DesktopEntry {
    pub name: String,
    pub exec: Option<String>,
    pub is_application: bool,
    /// `NoDisplay=true`: a helper that launchers hide.
    pub no_display: bool,
    /// `Hidden=true`: the entry is deleted as far as the user is concerned.
    pub hidden: bool,
    /// `X-GNOME-Autostart-enabled=false` turns an autostart entry off in
    /// GNOME without hiding it.
    pub gnome_autostart_disabled: bool,
}

fn truthy(v: Option<&String>) -> bool {
    v.map(|s| s.eq_ignore_ascii_case("true")).unwrap_or(false)
}

pub fn parse_desktop_entry(text: &str) -> Option<DesktopEntry> {
    let group = desktop_group(text, "Desktop Entry");
    let name = group.get("Name")?.clone();
    if name.is_empty() {
        return None;
    }
    Some(DesktopEntry {
        name,
        exec: group.get("Exec").cloned().filter(|e| !e.is_empty()),
        is_application: group
            .get("Type")
            .map(|t| t == "Application")
            .unwrap_or(false),
        no_display: truthy(group.get("NoDisplay")),
        hidden: truthy(group.get("Hidden")),
        gnome_autostart_disabled: group
            .get("X-GNOME-Autostart-enabled")
            .map(|v| v.eq_ignore_ascii_case("false"))
            .unwrap_or(false),
    })
}

/// The program an `Exec=` line starts, with field codes and quoting removed.
/// `env FOO=1 app %U` names `app`.
pub fn exec_program(exec: &str) -> Option<String> {
    let mut words = split_exec(exec).into_iter();
    let mut first = words.next()?;
    if first == "env" || first.ends_with("/env") {
        first = words.find(|w| !w.contains('='))?;
    }
    Some(first)
}

/// Split an `Exec=` value into words, honouring double quotes and dropping
/// `%f`-style field codes.
pub fn split_exec(exec: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut current = String::new();
    let mut quoted = false;
    let mut chars = exec.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '"' => quoted = !quoted,
            '\\' if quoted => {
                if let Some(next) = chars.next() {
                    current.push(next);
                }
            }
            c if c.is_whitespace() && !quoted => {
                if !current.is_empty() {
                    words.push(std::mem::take(&mut current));
                }
            }
            c => current.push(c),
        }
    }
    if !current.is_empty() {
        words.push(current);
    }
    words
        .into_iter()
        .filter(|w| !(w.len() == 2 && w.starts_with('%')))
        .collect()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackageRecord {
    pub name: String,
    pub version: Option<String>,
    pub publisher: Option<String>,
    pub size_bytes: Option<u64>,
    /// `YYYY-MM-DD`, when the database records it.
    pub install_date: Option<String>,
}

/// Strip the `Name <mail@host>` address from a maintainer or packager field.
fn person(field: &str) -> Option<String> {
    let name = field.split('<').next().unwrap_or(field).trim();
    (!name.is_empty()).then(|| name.to_string())
}

fn date_from_epoch(secs: i64) -> Option<String> {
    chrono::DateTime::from_timestamp(secs, 0).map(|d| d.format("%Y-%m-%d").to_string())
}

/// Parse `/var/lib/dpkg/status`, keeping only packages that are installed.
pub fn parse_dpkg_status(text: &str) -> HashMap<String, PackageRecord> {
    let mut out = HashMap::new();
    for stanza in text.split("\n\n") {
        let mut fields: HashMap<&str, &str> = HashMap::new();
        for line in stanza.lines() {
            if line.starts_with(' ') || line.starts_with('\t') {
                continue;
            }
            if let Some((k, v)) = line.split_once(':') {
                fields.insert(k.trim(), v.trim());
            }
        }
        let Some(name) = fields.get("Package") else {
            continue;
        };
        let installed = fields
            .get("Status")
            .map(|s| s.ends_with(" installed"))
            .unwrap_or(false);
        if !installed {
            continue;
        }
        out.insert(
            name.to_string(),
            PackageRecord {
                name: name.to_string(),
                version: fields.get("Version").map(|v| v.to_string()),
                publisher: fields.get("Maintainer").and_then(|m| person(m)),
                // dpkg records kibibytes.
                size_bytes: fields
                    .get("Installed-Size")
                    .and_then(|s| s.parse::<u64>().ok())
                    .map(|k| k * 1024),
                install_date: None,
            },
        );
    }
    out
}

/// Parse one `/var/lib/pacman/local/<pkg>/desc` file.
pub fn parse_pacman_desc(text: &str) -> Option<PackageRecord> {
    let mut fields: HashMap<&str, &str> = HashMap::new();
    let mut lines = text.lines();
    while let Some(line) = lines.next() {
        if line.starts_with('%') && line.ends_with('%') && line.len() > 2 {
            if let Some(value) = lines.next() {
                fields.insert(&line[1..line.len() - 1], value.trim());
            }
        }
    }
    let name = fields.get("NAME")?.to_string();
    Some(PackageRecord {
        name,
        version: fields.get("VERSION").map(|v| v.to_string()),
        publisher: fields.get("PACKAGER").and_then(|p| person(p)),
        size_bytes: fields.get("SIZE").and_then(|s| s.parse().ok()),
        install_date: fields
            .get("INSTALLDATE")
            .and_then(|s| s.parse().ok())
            .and_then(date_from_epoch),
    })
}

/// The desktop files a package ships, from a pacman `files` list (paths are
/// relative to `/`) or a dpkg `.list` file (paths are absolute).
pub fn desktop_files_in_listing(text: &str) -> Vec<String> {
    text.lines()
        .map(str::trim)
        .filter(|l| l.ends_with(".desktop") && l.contains("share/applications/"))
        .map(|l| {
            if l.starts_with('/') {
                l.to_string()
            } else {
                format!("/{l}")
            }
        })
        .collect()
}

/// The query format handed to `rpm -qf`, one tab-separated line per file.
pub const RPM_QUERY_FORMAT: &str =
    "%{NAME}\\t%{VERSION}-%{RELEASE}\\t%{VENDOR}\\t%{SIZE}\\t%{INSTALLTIME}\\n";

/// Parse the output of `rpm -qf --qf RPM_QUERY_FORMAT file`. Lines that are not
/// a record, such as older rpm's "file ... is not owned by any package", come
/// back as `None`. Newer rpm prints that sentence on stderr instead, which is
/// why callers ask about one file at a time.
pub fn parse_rpm_query(text: &str) -> Vec<Option<PackageRecord>> {
    text.lines()
        .map(|line| {
            let parts: Vec<&str> = line.split('\t').collect();
            if parts.len() != 5 {
                return None;
            }
            let clean = |s: &str| (!s.is_empty() && s != "(none)").then(|| s.to_string());
            Some(PackageRecord {
                name: parts[0].to_string(),
                version: clean(parts[1]),
                publisher: clean(parts[2]),
                size_bytes: parts[3].parse().ok(),
                install_date: parts[4].parse().ok().and_then(date_from_epoch),
            })
        })
        .collect()
}

/// The `version:` of a snap, from its `meta/snap.yaml`.
pub fn snap_version(yaml: &str) -> Option<String> {
    yaml.lines().find_map(|l| {
        let v = l
            .strip_prefix("version:")?
            .trim()
            .trim_matches(|c| c == '\'' || c == '"');
        (!v.is_empty()).then(|| v.to_string())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIREFOX_DESKTOP: &str = "\
[Desktop Entry]
Version=1.0
Name=Firefox Web Browser
Name[de]=Firefox-Webbrowser
Comment=Browse the World Wide Web
Exec=env BAMF_DESKTOP_FILE_HINT=/var/lib/snapd/desktop/applications/firefox_firefox.desktop /snap/bin/firefox %u
Terminal=false
Type=Application

[Desktop Action new-window]
Name=Open a New Window
Exec=/snap/bin/firefox -new-window
";

    #[test]
    fn a_desktop_entry_reads_only_its_own_group() {
        let e = parse_desktop_entry(FIREFOX_DESKTOP).unwrap();
        assert_eq!(e.name, "Firefox Web Browser");
        assert!(e.is_application);
        assert!(!e.no_display && !e.hidden);
        assert_eq!(
            exec_program(e.exec.as_deref().unwrap()).as_deref(),
            Some("/snap/bin/firefox")
        );
    }

    #[test]
    fn autostart_switches_are_understood() {
        let e = parse_desktop_entry(
            "[Desktop Entry]\nType=Application\nName=Agent\nExec=agent\nHidden=true\nX-GNOME-Autostart-enabled=false\n",
        )
        .unwrap();
        assert!(e.hidden && e.gnome_autostart_disabled);
        assert!(parse_desktop_entry("[Desktop Entry]\nType=Application\n").is_none());
    }

    #[test]
    fn setting_a_key_replaces_or_inserts_and_keeps_everything_else() {
        let original = "# comment\n[Desktop Entry]\nName=Agent\nHidden=false\n\n[Desktop Action x]\nHidden=keep\n";
        let changed = set_desktop_key(original, "Hidden", "true");
        assert_eq!(
            changed,
            "# comment\n[Desktop Entry]\nName=Agent\nHidden=true\n\n[Desktop Action x]\nHidden=keep\n"
        );
        let inserted = set_desktop_key("[Desktop Entry]\nName=Agent\n", "Hidden", "true");
        assert_eq!(inserted, "[Desktop Entry]\nHidden=true\nName=Agent\n");
        assert!(parse_desktop_entry(&inserted).unwrap().hidden);
    }

    #[test]
    fn desktop_filters_follow_the_running_desktop() {
        let group = desktop_group(
            "[Desktop Entry]\nOnlyShowIn=GNOME;Unity;\n",
            "Desktop Entry",
        );
        assert!(shown_in(&group, "ubuntu:GNOME"));
        assert!(!shown_in(&group, "KDE"));
        let group = desktop_group("[Desktop Entry]\nNotShowIn=KDE;\n", "Desktop Entry");
        assert!(!shown_in(&group, "KDE"));
        assert!(shown_in(&group, "XFCE"));
    }

    #[test]
    fn exec_lines_honour_quotes_and_drop_field_codes() {
        assert_eq!(
            split_exec(r#""/opt/My App/run" --flag %F"#),
            vec!["/opt/My App/run".to_string(), "--flag".to_string()]
        );
    }

    #[test]
    fn dpkg_status_keeps_only_installed_packages() {
        let status = "\
Package: gimp
Status: install ok installed
Priority: optional
Installed-Size: 20480
Maintainer: Ubuntu Developers <ubuntu-devel-discuss@lists.ubuntu.com>
Architecture: amd64
Version: 2.10.36-3
Description: GNU Image Manipulation Program
 GIMP is an advanced picture editor.

Package: removed-thing
Status: deinstall ok config-files
Version: 1.0
";
        let pkgs = parse_dpkg_status(status);
        assert_eq!(pkgs.len(), 1);
        let gimp = &pkgs["gimp"];
        assert_eq!(gimp.version.as_deref(), Some("2.10.36-3"));
        assert_eq!(gimp.publisher.as_deref(), Some("Ubuntu Developers"));
        assert_eq!(gimp.size_bytes, Some(20480 * 1024));
    }

    #[test]
    fn pacman_desc_is_parsed() {
        let desc = "%NAME%\nfirefox\n\n%VERSION%\n128.0-1\n\n%PACKAGER%\nJan Alexander Steffens <heftig@archlinux.org>\n\n%SIZE%\n254000000\n\n%INSTALLDATE%\n1720000000\n";
        let p = parse_pacman_desc(desc).unwrap();
        assert_eq!(p.name, "firefox");
        assert_eq!(p.version.as_deref(), Some("128.0-1"));
        assert_eq!(p.publisher.as_deref(), Some("Jan Alexander Steffens"));
        assert_eq!(p.size_bytes, Some(254_000_000));
        assert_eq!(p.install_date.as_deref(), Some("2024-07-03"));
    }

    #[test]
    fn package_listings_yield_their_desktop_files() {
        let pacman =
            "%FILES%\nusr/\nusr/share/applications/firefox.desktop\nusr/lib/firefox/firefox\n";
        assert_eq!(
            desktop_files_in_listing(pacman),
            vec!["/usr/share/applications/firefox.desktop"]
        );
        let dpkg = "/.\n/usr/share/applications/gimp.desktop\n/usr/share/doc/gimp/x.desktop\n";
        assert_eq!(
            desktop_files_in_listing(dpkg),
            vec!["/usr/share/applications/gimp.desktop"]
        );
    }

    #[test]
    fn rpm_output_keeps_file_order_and_marks_unowned_files() {
        let out = "gimp\t2.10.38-1.fc40\tFedora Project\t120000000\t1715000000\nfile /usr/local/share/applications/x.desktop is not owned by any package\n";
        let parsed = parse_rpm_query(out);
        assert_eq!(parsed.len(), 2);
        assert_eq!(
            parsed[0].as_ref().unwrap().publisher.as_deref(),
            Some("Fedora Project")
        );
        assert!(parsed[1].is_none());
    }

    #[test]
    fn snap_versions_are_read() {
        assert_eq!(
            snap_version("name: firefox\nversion: '130.0'\n").as_deref(),
            Some("130.0")
        );
    }
}
