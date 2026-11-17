use crate::error::{AppError, Result};
use fastnbt::Value;
use flate2::read::GzDecoder;
use flate2::write::GzEncoder;
use flate2::Compression;
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

const OFFICIAL_MARKER: &str = "horizon_official_server.txt";
const OFFICIAL_SERVER: &str = "play.playhorizon.dev";
const PREVIOUS_OFFICIAL_SERVERS: [&str; 1] = ["horizon.bunchy7s.dev"];

pub(crate) fn apply(game_dir: &Path, show_official: bool) {
    if let Err(error) = apply_result(game_dir, show_official) {
        tracing::warn!("Liste des serveurs Horizon : {error}");
    }
}

fn apply_result(game_dir: &Path, show_official: bool) -> Result<()> {
    let marker = game_dir.join(OFFICIAL_MARKER);
    if show_official {
        let _ = fs::remove_file(marker);
        return Ok(());
    }
    fs::write(&marker, format!("{OFFICIAL_SERVER}\n"))?;
    remove_official_entries(&game_dir.join("servers.dat"))
}

fn official_entry(entry: &Value) -> bool {
    let Value::Compound(values) = entry else {
        return false;
    };
    let Some(Value::String(ip)) = values.get("ip") else {
        return false;
    };
    let host = ip
        .split(':')
        .next()
        .unwrap_or(ip)
        .trim_end_matches('.')
        .to_ascii_lowercase();
    host == OFFICIAL_SERVER || PREVIOUS_OFFICIAL_SERVERS.contains(&host.as_str())
}

fn remove_official_entries(path: &Path) -> Result<()> {
    if !path.is_file() {
        return Ok(());
    }
    let file = fs::File::open(path)?;
    let mut decoder = GzDecoder::new(file);
    let mut bytes = Vec::new();
    decoder.read_to_end(&mut bytes)?;
    let mut value: Value = fastnbt::from_bytes(&bytes)
        .map_err(|error| AppError::new(format!("servers.dat Horizon illisible : {error}")))?;
    let Value::Compound(root) = &mut value else {
        return Ok(());
    };
    let Some(Value::List(servers)) = root.get_mut("servers") else {
        return Ok(());
    };
    let before = servers.len();
    servers.retain(|entry| !official_entry(entry));
    if servers.len() == before {
        return Ok(());
    }
    crate::options::backup_once(path)?;
    let bytes = fastnbt::to_bytes(&value).map_err(|error| {
        AppError::new(format!("servers.dat Horizon non sérialisable : {error}"))
    })?;
    write_gzip(path, &bytes)
}

fn write_gzip(path: &Path, bytes: &[u8]) -> Result<()> {
    let temporary = temporary_path(path);
    let file = fs::File::create(&temporary)?;
    let mut encoder = GzEncoder::new(file, Compression::default());
    encoder.write_all(bytes)?;
    encoder.finish()?.sync_all()?;
    fs::rename(&temporary, path)?;
    Ok(())
}

fn temporary_path(path: &Path) -> PathBuf {
    PathBuf::from(format!("{}.xal-tmp", path.display()))
}

#[cfg(test)]
mod tests {
    use super::{apply, remove_official_entries, OFFICIAL_MARKER, OFFICIAL_SERVER};
    use fastnbt::Value;
    use flate2::read::GzDecoder;
    use flate2::write::GzEncoder;
    use flate2::Compression;
    use std::collections::HashMap;
    use std::fs;
    use std::io::{Read, Write};

    fn server(name: &str, ip: &str) -> Value {
        let mut entry = HashMap::new();
        entry.insert("name".to_string(), Value::String(name.to_string()));
        entry.insert("ip".to_string(), Value::String(ip.to_string()));
        entry.insert("acceptTextures".to_string(), Value::Byte(1));
        Value::Compound(entry)
    }

    fn write_list(path: &std::path::Path, entries: Vec<Value>) {
        let mut root = HashMap::new();
        root.insert("servers".to_string(), Value::List(entries));
        let bytes = fastnbt::to_bytes(&Value::Compound(root)).unwrap();
        let file = fs::File::create(path).unwrap();
        let mut encoder = GzEncoder::new(file, Compression::default());
        encoder.write_all(&bytes).unwrap();
        encoder.finish().unwrap();
    }

    fn read_list(path: &std::path::Path) -> Vec<(String, String)> {
        let file = fs::File::open(path).unwrap();
        let mut decoder = GzDecoder::new(file);
        let mut bytes = Vec::new();
        decoder.read_to_end(&mut bytes).unwrap();
        let value: Value = fastnbt::from_bytes(&bytes).unwrap();
        let Value::Compound(root) = value else {
            panic!("root");
        };
        let Some(Value::List(servers)) = root.get("servers") else {
            panic!("servers");
        };
        servers
            .iter()
            .map(|entry| {
                let Value::Compound(values) = entry else {
                    panic!("entry");
                };
                let name = match values.get("name").unwrap() {
                    Value::String(value) => value.clone(),
                    _ => panic!("name"),
                };
                let ip = match values.get("ip").unwrap() {
                    Value::String(value) => value.clone(),
                    _ => panic!("ip"),
                };
                (name, ip)
            })
            .collect()
    }

    #[test]
    fn writes_the_marker_and_removes_the_official_server() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("servers.dat");
        write_list(
            &path,
            vec![
                server("Horizon Official", OFFICIAL_SERVER),
                server("Mon SMP", "mon.smp.fr"),
                server("Ancien", "horizon.bunchy7s.dev:25565"),
            ],
        );

        apply(directory.path(), false);

        assert_eq!(
            fs::read_to_string(directory.path().join(OFFICIAL_MARKER)).unwrap(),
            format!("{OFFICIAL_SERVER}\n")
        );
        assert_eq!(
            read_list(&path),
            vec![("Mon SMP".to_string(), "mon.smp.fr".to_string())]
        );
        assert!(directory.path().join("servers.dat.xal-backup").is_file());
    }

    #[test]
    fn keeps_other_entries_and_does_not_touch_a_missing_file() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("servers.dat");
        write_list(&path, vec![server("Mon SMP", "mon.smp.fr")]);

        remove_official_entries(&path).unwrap();

        assert_eq!(
            read_list(&path),
            vec![("Mon SMP".to_string(), "mon.smp.fr".to_string())]
        );
        assert!(!directory.path().join("servers.dat.xal-backup").exists());
    }

    #[test]
    fn showing_the_official_server_removes_the_marker() {
        let directory = tempfile::tempdir().unwrap();
        fs::write(directory.path().join(OFFICIAL_MARKER), "old").unwrap();

        apply(directory.path(), true);

        assert!(!directory.path().join(OFFICIAL_MARKER).exists());
    }
}
