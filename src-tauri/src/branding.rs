use crate::error::Result;
use crate::models::Instance;
use crate::options;
use crate::store::Store;
use std::fs;
use std::io::Write;
use std::path::Path;
use zip::write::SimpleFileOptions;

const PACK_FILE: &str = "XalLauncher-Branding.zip";
const PACK_ENTRY: &str = "file/XalLauncher-Branding.zip";
const MARKER: &str = ".xal-branding-version";
const PACK_REVISION: &str = "3";

const SPLASHES: &str = "XalLauncher, par Xaltar Studio !\nPret pour l'aventure ?\nDes nebuleuses plein les yeux !\nCape officielle, style garanti\nUn launcher, mille mondes\nLa performance, sans compromis\nXaltar Studio veille sur vous\nEspace infini, possibilites infinies\nJouez plus vite, jouez plus loin\nVotre monde, votre launcher\n";

/// Panorama cubemap 2048×2048 rendu par `examples/bake_branding.rs`.
/// Les PNG sont embarqués : aucun rendu n'a lieu au lancement.
const PANORAMA_FACES: [&[u8]; 6] = [
    include_bytes!("../assets/branding/panorama_0.png"),
    include_bytes!("../assets/branding/panorama_1.png"),
    include_bytes!("../assets/branding/panorama_2.png"),
    include_bytes!("../assets/branding/panorama_3.png"),
    include_bytes!("../assets/branding/panorama_4.png"),
    include_bytes!("../assets/branding/panorama_5.png"),
];
const PACK_ICON: &[u8] = include_bytes!("../assets/branding/pack.png");

pub fn ensure(store: &Store, instance: &Instance) -> Result<()> {
    let game_dir = store.game_dir(&instance.id);
    let directory = game_dir.join("resourcepacks");
    fs::create_dir_all(&directory)?;
    let path = directory.join(PACK_FILE);
    let marker = directory.join(MARKER);
    let revision = format!("{}-{PACK_REVISION}", env!("CARGO_PKG_VERSION"));
    let current = fs::read_to_string(&marker).unwrap_or_default();
    if !path.exists() || current.trim() != revision {
        write_pack(&path, &instance.version_id, env!("CARGO_PKG_VERSION"))?;
        fs::write(&marker, revision)?;
    }
    enable(&game_dir)
}

pub fn disable(store: &Store, instance: &Instance) -> Result<()> {
    let game_dir = store.game_dir(&instance.id);
    let options_path = game_dir.join("options.txt");
    if !options_path.exists() {
        return Ok(());
    }
    let mut entries = options::read(&options_path)?;
    options::remove_from_list(&mut entries, "resourcePacks", PACK_ENTRY)?;
    options::remove_from_list(&mut entries, "incompatibleResourcePacks", PACK_ENTRY)?;
    options::write(&options_path, &entries)
}

fn enable(game_dir: &Path) -> Result<()> {
    let path = game_dir.join("options.txt");
    if path.exists() {
        options::backup_once(&path)?;
    }
    let mut entries = options::read(&path)?;
    options::ensure_list(&mut entries, "resourcePacks", PACK_ENTRY)?;
    options::ensure_list(&mut entries, "incompatibleResourcePacks", PACK_ENTRY)?;
    options::write(&path, &entries)
}

fn write_pack(path: &Path, version_id: &str, launcher_version: &str) -> Result<()> {
    let temp = path.with_extension("zip.tmp");
    let file = fs::File::create(&temp)?;
    let mut zip = zip::ZipWriter::new(file);
    // Les panoramas sont des PNG déjà compressés : Stored évite un deflate inutile.
    let stored = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);

    let meta = format!(
        r#"{{"pack":{{"pack_format":{},"description":"XalLauncher Branding {}","supported_formats":{{"min_inclusive":1,"max_inclusive":999}}}}}}"#,
        pack_format(version_id),
        launcher_version
    );
    zip.start_file("pack.mcmeta", stored)?;
    zip.write_all(meta.as_bytes())?;
    zip.start_file("pack.png", stored)?;
    zip.write_all(PACK_ICON)?;
    zip.start_file("assets/minecraft/texts/splashes.txt", stored)?;
    zip.write_all(SPLASHES.as_bytes())?;
    for (index, bytes) in PANORAMA_FACES.iter().enumerate() {
        zip.start_file(
            format!("assets/minecraft/textures/gui/title/background/panorama_{index}.png"),
            stored,
        )?;
        zip.write_all(bytes)?;
    }
    zip.start_file(
        "assets/minecraft/textures/gui/title/background/panorama.png",
        stored,
    )?;
    zip.write_all(PANORAMA_FACES[0])?;
    zip.finish()?;

    if path.exists() {
        fs::remove_file(path)?;
    }
    fs::rename(&temp, path)?;
    Ok(())
}

fn pack_format(version_id: &str) -> u32 {
    let mut parts = version_id
        .split('.')
        .filter_map(|part| part.parse::<u32>().ok());
    let major = parts.next().unwrap_or(1);
    let minor = parts.next().unwrap_or(21);
    let patch = parts.next().unwrap_or(0);
    if major != 1 || minor < 13 {
        return 4;
    }
    match (minor, patch) {
        (13..=14, _) => 4,
        (15, _) => 5,
        (16, 0..=1) => 5,
        (16, _) => 6,
        (17, _) => 7,
        (18, _) => 8,
        (19, 0..=2) => 9,
        (19, 3) => 12,
        (19, _) => 13,
        (20, 0..=1) => 15,
        (20, 2) => 18,
        (20, 3..=4) => 22,
        (20, _) => 32,
        (21, 0..=1) => 34,
        (21, 2..=3) => 42,
        (21, 4) => 46,
        (21, 5) => 55,
        (21, 6) => 63,
        _ => 64,
    }
}

#[cfg(test)]
mod tests {
    use super::{pack_format, write_pack, PANORAMA_FACES};

    fn faces() -> Vec<image::RgbaImage> {
        PANORAMA_FACES
            .iter()
            .map(|bytes| {
                image::load_from_memory(bytes)
                    .expect("PNG embarqué invalide")
                    .to_rgba8()
            })
            .collect()
    }

    #[test]
    fn maps_known_pack_formats() {
        assert_eq!(pack_format("1.20.1"), 15);
        assert_eq!(pack_format("1.21.1"), 34);
        assert_eq!(pack_format("1.21.5"), 55);
        assert_eq!(pack_format("1.8.9"), 4);
    }

    #[test]
    fn baked_panorama_is_large_and_seamless() {
        let faces = faces();
        let size = faces[0].width();
        assert_eq!(size, 2048, "le panorama doit être cuit en 2048²");
        for face in &faces {
            assert_eq!((face.width(), face.height()), (size, size));
        }
        let last = size - 1;
        for (left, right) in [(0usize, 1usize), (1, 2), (2, 3), (3, 0)] {
            let delta = median_columns(&faces[left], last, &faces[right], 0);
            assert!(
                delta <= 6,
                "couture entre les faces {left} et {right} : {delta}"
            );
        }
        assert!(
            median_rows(&faces[0], 0, &faces[4], last) <= 6,
            "couture haute"
        );
        assert!(
            median_rows(&faces[0], last, &faces[5], 0) <= 6,
            "couture basse"
        );
    }

    /// Écart médian de deux colonnes de bord : insensible au dithering et aux
    /// étoiles, mais révèle un décalage de faces.
    fn median_columns(
        left: &image::RgbaImage,
        left_x: u32,
        right: &image::RgbaImage,
        right_x: u32,
    ) -> u32 {
        let size = left.height();
        let mut deltas = (0..size)
            .map(|y| channel_delta(left.get_pixel(left_x, y), right.get_pixel(right_x, y)))
            .collect::<Vec<_>>();
        deltas.sort_unstable();
        deltas[deltas.len() / 2]
    }

    fn median_rows(
        top: &image::RgbaImage,
        top_y: u32,
        bottom: &image::RgbaImage,
        bottom_y: u32,
    ) -> u32 {
        let size = top.width();
        let mut deltas = (0..size)
            .map(|x| channel_delta(top.get_pixel(x, top_y), bottom.get_pixel(x, bottom_y)))
            .collect::<Vec<_>>();
        deltas.sort_unstable();
        deltas[deltas.len() / 2]
    }

    fn channel_delta(left: &image::Rgba<u8>, right: &image::Rgba<u8>) -> u32 {
        (0..3)
            .map(|channel| left.0[channel].abs_diff(right.0[channel]) as u32)
            .max()
            .unwrap_or(0)
    }

    #[test]
    fn baked_panorama_keeps_contrast_and_detail() {
        let face = &faces()[0];
        let mut low = 255u8;
        let mut high = 0u8;
        let mut shades = std::collections::HashSet::new();
        for pixel in face.pixels().step_by(13) {
            low = low.min(pixel.0[2]);
            high = high.max(pixel.0[2]);
            shades.insert(
                ((pixel.0[0] as u32) << 16) | ((pixel.0[1] as u32) << 8) | pixel.0[2] as u32,
            );
        }
        assert!(high > 230, "zones lumineuses absentes : {high}");
        assert!(low < 24, "zones sombres absentes : {low}");
        assert!(shades.len() > 5000, "manque de nuances : {}", shades.len());
    }

    #[test]
    fn pack_contains_every_branding_asset() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("pack.zip");
        write_pack(&path, "1.21.5", "1.0.0").unwrap();
        let archive = std::fs::File::open(&path).unwrap();
        let mut zip = zip::ZipArchive::new(archive).unwrap();
        let names = (0..zip.len())
            .map(|index| zip.by_index(index).unwrap().name().unwrap().to_string())
            .collect::<Vec<_>>();
        for name in [
            "pack.mcmeta",
            "pack.png",
            "assets/minecraft/texts/splashes.txt",
            "assets/minecraft/textures/gui/title/background/panorama.png",
        ] {
            assert!(
                names.iter().any(|entry| entry == name),
                "entrée absente : {name}"
            );
        }
        for index in 0..6 {
            let name =
                format!("assets/minecraft/textures/gui/title/background/panorama_{index}.png");
            assert!(
                names.iter().any(|entry| entry == &name),
                "entrée absente : {name}"
            );
        }
        assert!(
            std::fs::metadata(&path).unwrap().len() > 20 * 1024 * 1024,
            "panorama embarqué trop léger"
        );
    }

    #[test]
    #[ignore = "écrit les panoramas embarqués dans src-tauri/target/panorama-preview"]
    fn panorama_preview() {
        let directory = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("target")
            .join("panorama-preview");
        std::fs::create_dir_all(&directory).unwrap();
        for (index, bytes) in PANORAMA_FACES.iter().enumerate() {
            std::fs::write(directory.join(format!("panorama_{index}.png")), bytes).unwrap();
        }
        std::fs::write(directory.join("pack.png"), super::PACK_ICON).unwrap();
    }
}
