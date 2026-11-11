use crate::error::Result;
use crate::models::Instance;
use crate::options;
use crate::store::Store;
use image::{ImageFormat, Rgba, RgbaImage};
use std::fs;
use std::io::{Cursor, Write};
use std::path::Path;
use zip::write::SimpleFileOptions;

const PACK_FILE: &str = "XalLauncher-Branding.zip";
const PACK_ENTRY: &str = "file/XalLauncher-Branding.zip";
const MARKER: &str = ".xal-branding-version";

const SPLASHES: &str = "XalLauncher, par Xaltar Studio !\nPret pour l'aventure ?\nDes nebuleuses plein les yeux !\nCape officielle, style garanti\nUn launcher, mille mondes\nLa performance, sans compromis\nXaltar Studio veille sur vous\nEspace infini, possibilites infinies\nJouez plus vite, jouez plus loin\nVotre monde, votre launcher\n";

pub fn ensure(store: &Store, instance: &Instance) -> Result<()> {
    let game_dir = store.game_dir(&instance.id);
    let directory = game_dir.join("resourcepacks");
    fs::create_dir_all(&directory)?;
    let path = directory.join(PACK_FILE);
    let marker = directory.join(MARKER);
    let version = env!("CARGO_PKG_VERSION");
    let current = fs::read_to_string(&marker).unwrap_or_default();
    if !path.exists() || current.trim() != version {
        write_pack(&path, &instance.version_id, version)?;
        fs::write(&marker, version)?;
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
    let deflate = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);

    let meta = format!(
        r#"{{"pack":{{"pack_format":{},"description":"XalLauncher Branding {}","supported_formats":{{"min_inclusive":1,"max_inclusive":999}}}}}}"#,
        pack_format(version_id),
        launcher_version
    );
    zip.start_file("pack.mcmeta", deflate)?;
    zip.write_all(meta.as_bytes())?;
    zip.start_file("pack.png", deflate)?;
    zip.write_all(&png_bytes(pack_icon())?)?;
    zip.start_file("assets/minecraft/texts/splashes.txt", deflate)?;
    zip.write_all(SPLASHES.as_bytes())?;
    for index in 0..6u32 {
        zip.start_file(
            format!("assets/minecraft/textures/gui/title/background/panorama_{index}.png"),
            deflate,
        )?;
        zip.write_all(&png_bytes(panorama(index))?)?;
    }
    zip.start_file(
        "assets/minecraft/textures/gui/title/background/panorama.png",
        deflate,
    )?;
    zip.write_all(&png_bytes(panorama(0))?)?;
    zip.finish()?;

    if path.exists() {
        fs::remove_file(path)?;
    }
    fs::rename(&temp, path)?;
    Ok(())
}

fn png_bytes(image: RgbaImage) -> Result<Vec<u8>> {
    let mut cursor = Cursor::new(Vec::new());
    image::DynamicImage::ImageRgba8(image)
        .write_to(&mut cursor, ImageFormat::Png)
        .map_err(|error| {
            crate::error::AppError::new(format!("Encodage PNG impossible : {error}"))
        })?;
    Ok(cursor.into_inner())
}

fn pack_format(version_id: &str) -> u32 {
    let mut parts = version_id.split('.').filter_map(|part| part.parse::<u32>().ok());
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

fn pack_icon() -> RgbaImage {
    let mut image = RgbaImage::from_pixel(128, 128, Rgba([9, 11, 18, 255]));
    for y in 0..128u32 {
        for x in 0..128u32 {
            let border = x < 5 || y < 5 || x > 122 || y > 122;
            let dx = (x as i32 - y as i32).abs();
            let dy = (x as i32 + y as i32 - 127).abs();
            let pixel = if border {
                Rgba([139, 92, 246, 255])
            } else if dx < 10 || dy < 10 {
                Rgba([255, 255, 255, 255])
            } else {
                let glow = (x + y) as f32 / 254.0;
                Rgba([(10.0 + glow * 20.0) as u8, (12.0 + glow * 14.0) as u8, (28.0 + glow * 42.0) as u8, 255])
            };
            image.put_pixel(x, y, pixel);
        }
    }
    image
}

fn panorama(index: u32) -> RgbaImage {
    let size = 256u32;
    let mut image = RgbaImage::new(size, size);
    let mut seed = 0x9E37_79B9_7F4A_7C15u64 ^ ((index as u64 + 1).wrapping_mul(0xBF58_476D_1CE4_E5B9));
    for y in 0..size {
        for x in 0..size {
            let fx = x as f32 / size as f32;
            let fy = y as f32 / size as f32;
            let angle = index as f32 * 1.047;
            let wave = ((fx * 6.0 + angle).sin()
                + (fy * 5.0 - angle).cos()
                + ((fx + fy) * 3.0).sin())
                / 3.0;
            let glow = (wave * 0.5 + 0.5).powf(2.0);
            let red = (5.0 + glow * 38.0) as u8;
            let green = (7.0 + glow * 22.0) as u8;
            let blue = (18.0 + glow * 70.0) as u8;
            image.put_pixel(x, y, Rgba([red, green, blue, 255]));
        }
    }
    for _ in 0..900 {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        let x = (seed % size as u64) as u32;
        let y = ((seed >> 8) % size as u64) as u32;
        let brightness = 170 + ((seed >> 16) % 86) as u8;
        image.put_pixel(x, y, Rgba([brightness, brightness, 255, 255]));
    }
    image
}

#[cfg(test)]
mod tests {
    use super::pack_format;

    #[test]
    fn maps_known_pack_formats() {
        assert_eq!(pack_format("1.20.1"), 15);
        assert_eq!(pack_format("1.21.1"), 34);
        assert_eq!(pack_format("1.21.5"), 55);
        assert_eq!(pack_format("1.8.9"), 4);
    }
}

