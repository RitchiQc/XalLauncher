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
const PACK_REVISION: &str = "2";
const PANORAMA_SIZE: u32 = 1024;
const STAR_COUNT: usize = 2600;

const SPLASHES: &str = "XalLauncher, par Xaltar Studio !\nPret pour l'aventure ?\nDes nebuleuses plein les yeux !\nCape officielle, style garanti\nUn launcher, mille mondes\nLa performance, sans compromis\nXaltar Studio veille sur vous\nEspace infini, possibilites infinies\nJouez plus vite, jouez plus loin\nVotre monde, votre launcher\n";

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
    write_pack_sized(path, version_id, launcher_version, PANORAMA_SIZE)
}

fn write_pack_sized(
    path: &Path,
    version_id: &str,
    launcher_version: &str,
    size: u32,
) -> Result<()> {
    let temp = path.with_extension("zip.tmp");
    let file = fs::File::create(&temp)?;
    let mut zip = zip::ZipWriter::new(file);
    let deflate = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    let stars = make_stars();
    let faces: Vec<RgbaImage> = (0..6u32)
        .map(|index| panorama_sized(index, size, &stars))
        .collect();

    let meta = format!(
        r#"{{"pack":{{"pack_format":{},"description":"XalLauncher Branding {}","supported_formats":{{"min_inclusive":1,"max_inclusive":999}}}}}}"#,
        pack_format(version_id),
        launcher_version
    );
    zip.start_file("pack.mcmeta", deflate)?;
    zip.write_all(meta.as_bytes())?;
    zip.start_file("pack.png", deflate)?;
    zip.write_all(&png_bytes(pack_icon(&faces[0]))?)?;
    zip.start_file("assets/minecraft/texts/splashes.txt", deflate)?;
    zip.write_all(SPLASHES.as_bytes())?;
    let legacy = png_bytes(faces[0].clone())?;
    for (index, face) in faces.into_iter().enumerate() {
        zip.start_file(
            format!("assets/minecraft/textures/gui/title/background/panorama_{index}.png"),
            deflate,
        )?;
        zip.write_all(&png_bytes(face)?)?;
    }
    zip.start_file(
        "assets/minecraft/textures/gui/title/background/panorama.png",
        deflate,
    )?;
    zip.write_all(&legacy)?;
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

fn pack_icon(background: &RgbaImage) -> RgbaImage {
    let mut icon =
        image::imageops::resize(background, 128, 128, image::imageops::FilterType::Triangle);
    for y in 0..128u32 {
        for x in 0..128u32 {
            let edge = (x as i32 - y as i32)
                .abs()
                .min((x as i32 + y as i32 - 127).abs()) as f32;
            let alpha = (1.0 - (edge - 4.0) / 4.0).clamp(0.0, 1.0) * 0.85;
            if alpha <= 0.0 {
                continue;
            }
            let pixel = icon.get_pixel_mut(x, y);
            for (channel, mark) in [0.84f32, 0.80, 1.0].iter().enumerate() {
                let base = pixel.0[channel] as f32 / 255.0;
                pixel.0[channel] = (lerp(base, *mark, alpha) * 255.0).round() as u8;
            }
        }
    }
    icon
}

type Vec3 = [f32; 3];

#[derive(Clone, Copy)]
struct FaceBasis {
    forward: Vec3,
    right: Vec3,
    up: Vec3,
}

struct Star {
    dir: Vec3,
    intensity: f32,
    sigma: f32,
    tint: Vec3,
}

struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u32 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        (x >> 32) as u32
    }

    fn unit(&mut self) -> f32 {
        self.next() as f32 / u32::MAX as f32
    }
}

fn panorama_sized(index: u32, size: u32, stars: &[Star]) -> RgbaImage {
    let basis = face_basis(index);
    let mut image = RgbaImage::new(size, size);
    for y in 0..size {
        let v = 1.0 - 2.0 * (y as f32 + 0.5) / size as f32;
        for x in 0..size {
            let u = 2.0 * (x as f32 + 0.5) / size as f32 - 1.0;
            let color = nebula_color(direction(&basis, u, v));
            image.put_pixel(x, y, to_pixel(color));
        }
    }
    draw_stars(&mut image, &basis, stars);
    image
}

fn face_basis(index: u32) -> FaceBasis {
    match index {
        1 => FaceBasis {
            forward: [1.0, 0.0, 0.0],
            right: [0.0, 0.0, -1.0],
            up: [0.0, 1.0, 0.0],
        },
        2 => FaceBasis {
            forward: [0.0, 0.0, -1.0],
            right: [-1.0, 0.0, 0.0],
            up: [0.0, 1.0, 0.0],
        },
        3 => FaceBasis {
            forward: [-1.0, 0.0, 0.0],
            right: [0.0, 0.0, 1.0],
            up: [0.0, 1.0, 0.0],
        },
        4 => FaceBasis {
            forward: [0.0, 1.0, 0.0],
            right: [1.0, 0.0, 0.0],
            up: [0.0, 0.0, -1.0],
        },
        5 => FaceBasis {
            forward: [0.0, -1.0, 0.0],
            right: [1.0, 0.0, 0.0],
            up: [0.0, 0.0, 1.0],
        },
        _ => FaceBasis {
            forward: [0.0, 0.0, 1.0],
            right: [1.0, 0.0, 0.0],
            up: [0.0, 1.0, 0.0],
        },
    }
}

fn direction(basis: &FaceBasis, u: f32, v: f32) -> Vec3 {
    normalize([
        basis.forward[0] + u * basis.right[0] + v * basis.up[0],
        basis.forward[1] + u * basis.right[1] + v * basis.up[1],
        basis.forward[2] + u * basis.right[2] + v * basis.up[2],
    ])
}

fn normalize(value: Vec3) -> Vec3 {
    let length = dot(value, value).sqrt();
    if length <= f32::EPSILON {
        return [0.0, 0.0, 1.0];
    }
    [value[0] / length, value[1] / length, value[2] / length]
}

fn dot(a: Vec3, b: Vec3) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

fn lerp3(a: Vec3, b: Vec3, t: f32) -> Vec3 {
    [
        lerp(a[0], b[0], t),
        lerp(a[1], b[1], t),
        lerp(a[2], b[2], t),
    ]
}

fn smoothstep(edge0: f32, edge1: f32, value: f32) -> f32 {
    let t = ((value - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn smooth(value: f32) -> f32 {
    value * value * (3.0 - 2.0 * value)
}

fn hash3(x: i32, y: i32, z: i32) -> f32 {
    let mut h = (x as u32).wrapping_mul(0x8DA6_B343)
        ^ (y as u32).wrapping_mul(0xD816_3841)
        ^ (z as u32).wrapping_mul(0xCB1A_B31F);
    h ^= h >> 13;
    h = h.wrapping_mul(0x5BD1_E995);
    h ^= h >> 15;
    (h >> 8) as f32 / 16_777_216.0
}

fn noise3(point: Vec3) -> f32 {
    let xi = point[0].floor();
    let yi = point[1].floor();
    let zi = point[2].floor();
    let tx = smooth(point[0] - xi);
    let ty = smooth(point[1] - yi);
    let tz = smooth(point[2] - zi);
    let (x, y, z) = (xi as i32, yi as i32, zi as i32);
    let x00 = lerp(hash3(x, y, z), hash3(x + 1, y, z), tx);
    let x10 = lerp(hash3(x, y + 1, z), hash3(x + 1, y + 1, z), tx);
    let x01 = lerp(hash3(x, y, z + 1), hash3(x + 1, y, z + 1), tx);
    let x11 = lerp(hash3(x, y + 1, z + 1), hash3(x + 1, y + 1, z + 1), tx);
    lerp(lerp(x00, x10, ty), lerp(x01, x11, ty), tz)
}

fn fbm(point: Vec3, octaves: u32) -> f32 {
    let mut sum = 0.0;
    let mut total = 0.0;
    let mut amplitude = 1.0;
    let mut frequency = 1.0;
    for _ in 0..octaves {
        let sample = [
            point[0] * frequency,
            point[1] * frequency,
            point[2] * frequency,
        ];
        sum += noise3(sample) * amplitude;
        total += amplitude;
        amplitude *= 0.5;
        frequency *= 2.03;
    }
    sum / total
}

fn nebula_color(dir: Vec3) -> Vec3 {
    let base = [dir[0] * 1.7, dir[1] * 1.7, dir[2] * 1.7];
    let warp = [
        fbm(
            [
                base[0] * 1.1 + 4.3,
                base[1] * 1.1 + 1.7,
                base[2] * 1.1 + 8.9,
            ],
            2,
        ),
        fbm(
            [
                base[0] * 1.1 + 12.1,
                base[1] * 1.1 + 7.4,
                base[2] * 1.1 + 2.6,
            ],
            2,
        ),
        fbm(
            [
                base[0] * 1.1 + 3.4,
                base[1] * 1.1 + 15.2,
                base[2] * 1.1 + 11.3,
            ],
            2,
        ),
    ];
    let warped = [
        base[0] + (warp[0] - 0.5) * 0.9,
        base[1] + (warp[1] - 0.5) * 0.9,
        base[2] + (warp[2] - 0.5) * 0.9,
    ];
    let cloud = fbm([warped[0] * 1.3, warped[1] * 1.3, warped[2] * 1.3], 4);
    let wisp = fbm(
        [
            warped[0] * 3.1 + 21.7,
            warped[1] * 3.1 + 6.4,
            warped[2] * 3.1 + 14.1,
        ],
        2,
    );
    let shape = cloud * 0.85 + wisp * 0.15;
    let density = smoothstep(0.36, 0.74, shape);
    let core = smoothstep(0.62, 0.88, shape).powi(2);
    let hue = fbm(
        [
            base[0] * 0.85 + 31.0,
            base[1] * 0.85 + 17.0,
            base[2] * 0.85 + 23.0,
        ],
        2,
    );
    let band = dot(dir, [0.42, 0.28, 0.86]).abs();
    let band_glow = (1.0 - (band / 0.55).min(1.0)).powi(2);
    let inner = 0.5 + 0.5 * core;
    let tint = lerp3(
        [0.38, 0.12, 0.60],
        [0.08, 0.21, 0.49],
        smoothstep(0.35, 0.75, hue),
    );

    let mut color = [0.006, 0.009, 0.022];
    color[0] += 0.018 * band_glow;
    color[1] += 0.026 * band_glow;
    color[2] += 0.068 * band_glow;
    color[0] += tint[0] * density * inner;
    color[1] += tint[1] * density * inner;
    color[2] += tint[2] * density * inner;
    color[0] += 0.12 * core;
    color[1] += 0.15 * core;
    color[2] += 0.28 * core;
    color
}

fn to_pixel(color: Vec3) -> Rgba<u8> {
    Rgba([
        (color[0].clamp(0.0, 1.0) * 255.0).round() as u8,
        (color[1].clamp(0.0, 1.0) * 255.0).round() as u8,
        (color[2].clamp(0.0, 1.0) * 255.0).round() as u8,
        255,
    ])
}

fn make_stars() -> Vec<Star> {
    let mut rng = Rng(0x5A11_A110_C0FF_EE5B);
    let mut stars = Vec::with_capacity(STAR_COUNT);
    for _ in 0..STAR_COUNT {
        let z = rng.unit() * 2.0 - 1.0;
        let phi = rng.unit() * std::f32::consts::TAU;
        let ring = (1.0 - z * z).max(0.0).sqrt();
        let dir = [ring * phi.cos(), z, ring * phi.sin()];
        let magnitude = rng.unit();
        let intensity = 0.16 + magnitude.powf(2.6) * 0.9;
        let sigma = 0.00072 + magnitude.powf(3.0) * 0.0012;
        let tint = match rng.unit() {
            value if value > 0.88 => [1.0, 0.87, 0.72],
            value if value < 0.22 => [0.76, 0.86, 1.0],
            _ => [1.0, 0.98, 0.95],
        };
        stars.push(Star {
            dir,
            intensity,
            sigma,
            tint,
        });
    }
    stars
}

fn draw_stars(image: &mut RgbaImage, basis: &FaceBasis, stars: &[Star]) {
    let size = image.width();
    let scale = size as f32;
    let radians_per_pixel = std::f32::consts::FRAC_PI_2 / scale;
    for star in stars {
        let depth = dot(star.dir, basis.forward);
        if depth <= 0.05 {
            continue;
        }
        let su = dot(star.dir, basis.right) / depth;
        let sv = dot(star.dir, basis.up) / depth;
        if su.abs() > 1.15 || sv.abs() > 1.15 {
            continue;
        }
        let center_x = (su * 0.5 + 0.5) * scale;
        let center_y = (0.5 - sv * 0.5) * scale;
        let reach = (star.sigma / radians_per_pixel) * 6.0 + 1.0;
        let x_start = ((center_x - reach).floor() as i32).max(0);
        let x_end = ((center_x + reach).ceil() as i32).min(size as i32 - 1);
        let y_start = ((center_y - reach).floor() as i32).max(0);
        let y_end = ((center_y + reach).ceil() as i32).min(size as i32 - 1);
        if x_start > x_end || y_start > y_end {
            continue;
        }
        let halo_sigma = star.sigma * 2.0;
        for py in y_start..=y_end {
            let v = 1.0 - 2.0 * (py as f32 + 0.5) / scale;
            for px in x_start..=x_end {
                let u = 2.0 * (px as f32 + 0.5) / scale - 1.0;
                let angle = dot(direction(basis, u, v), star.dir)
                    .clamp(-1.0, 1.0)
                    .acos();
                let core = (-0.5 * (angle / star.sigma).powi(2)).exp();
                let halo = (-0.5 * (angle / halo_sigma).powi(2)).exp() * 0.14;
                let value = star.intensity * (core + halo);
                if value < 0.003 {
                    continue;
                }
                add_pixel(
                    image,
                    px as u32,
                    py as u32,
                    [
                        star.tint[0] * value,
                        star.tint[1] * value,
                        star.tint[2] * value,
                    ],
                );
            }
        }
    }
}

fn add_pixel(image: &mut RgbaImage, x: u32, y: u32, color: Vec3) {
    let pixel = image.get_pixel_mut(x, y);
    for (channel, light) in color.iter().enumerate() {
        let value = pixel.0[channel] as f32 / 255.0 + light;
        pixel.0[channel] = (value.clamp(0.0, 1.0) * 255.0).round() as u8;
    }
}

#[cfg(test)]
mod tests {
    use super::{
        direction, face_basis, make_stars, nebula_color, pack_format, pack_icon, panorama_sized,
        to_pixel, write_pack_sized, Star, PANORAMA_SIZE,
    };

    #[test]
    fn maps_known_pack_formats() {
        assert_eq!(pack_format("1.20.1"), 15);
        assert_eq!(pack_format("1.21.1"), 34);
        assert_eq!(pack_format("1.21.5"), 55);
        assert_eq!(pack_format("1.8.9"), 4);
    }

    #[test]
    fn panorama_faces_are_seamless() {
        let size = 256;
        let stars: Vec<Star> = Vec::new();
        let faces: Vec<_> = (0..6)
            .map(|index| panorama_sized(index, size, &stars))
            .collect();
        for (index, image) in faces.iter().enumerate() {
            let basis = face_basis(index as u32);
            for (x, y) in border_pixels(size) {
                let u = 2.0 * (x as f32 + 0.5) / size as f32 - 1.0;
                let v = 1.0 - 2.0 * (y as f32 + 0.5) / size as f32;
                let expected = to_pixel(nebula_color(direction(&basis, u, v)));
                assert_close(expected, *image.get_pixel(x, y));
            }
        }
    }

    fn border_pixels(size: u32) -> Vec<(u32, u32)> {
        let last = size - 1;
        let mut pixels = Vec::with_capacity((size * 4) as usize);
        for offset in 0..size {
            pixels.push((offset, 0));
            pixels.push((offset, last));
            pixels.push((0, offset));
            pixels.push((last, offset));
        }
        pixels
    }

    #[test]
    fn panorama_edges_share_the_same_directions() {
        let ring = [(0u32, 1u32), (1, 2), (2, 3), (3, 0)];
        for (left, right) in ring {
            let (a, b) = (face_basis(left), face_basis(right));
            for step in 0..=8 {
                let t = -1.0 + step as f32 / 4.0;
                assert_direction(direction(&a, 1.0, t), direction(&b, -1.0, t));
            }
        }
        let front = face_basis(0);
        let top = face_basis(4);
        let bottom = face_basis(5);
        for step in 0..=8 {
            let t = -1.0 + step as f32 / 4.0;
            assert_direction(direction(&front, t, 1.0), direction(&top, t, -1.0));
            assert_direction(direction(&front, t, -1.0), direction(&bottom, t, 1.0));
        }
    }

    fn assert_direction(left: [f32; 3], right: [f32; 3]) {
        for (index, channel) in left.iter().enumerate() {
            assert!(
                (channel - right[index]).abs() < 1e-5,
                "les faces ne pointent pas dans la même direction : {left:?} contre {right:?}"
            );
        }
    }

    fn assert_close(expected: image::Rgba<u8>, actual: image::Rgba<u8>) {
        for channel in 0..3 {
            let delta = expected.0[channel].abs_diff(actual.0[channel]);
            assert!(
                delta <= 8,
                "couture visible : {expected:?} attendu, {actual:?} rendu"
            );
        }
    }

    #[test]
    fn panorama_is_deterministic_and_starry() {
        let stars = make_stars();
        let first = panorama_sized(0, 96, &stars);
        let second = panorama_sized(0, 96, &stars);
        assert_eq!(first.as_raw(), second.as_raw());
        let brightest = first.pixels().map(|pixel| pixel.0[2]).max().unwrap_or(0);
        assert!(
            brightest > 200,
            "le panorama doit contenir des étoiles lumineuses"
        );
    }

    #[test]
    fn pack_contains_every_branding_asset() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("pack.zip");
        write_pack_sized(&path, "1.21.5", "1.0.0", 96).unwrap();
        let archive = std::fs::File::open(&path).unwrap();
        let mut zip = zip::ZipArchive::new(archive).unwrap();
        let names: Vec<String> = (0..zip.len())
            .map(|index| zip.by_index(index).unwrap().name().unwrap().to_string())
            .collect();
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
    }

    #[test]
    #[ignore = "écrit un aperçu des six faces dans src-tauri/target/panorama-preview"]
    fn panorama_preview() {
        let directory = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("target")
            .join("panorama-preview");
        std::fs::create_dir_all(&directory).unwrap();
        let stars = make_stars();
        for index in 0..6 {
            panorama_sized(index, PANORAMA_SIZE, &stars)
                .save(directory.join(format!("panorama_{index}.png")))
                .unwrap();
        }
        let size = 384;
        let mut sheet = image::RgbaImage::new(size * 4, size * 3);
        let layout = [
            (1usize, 0usize, 4u32),
            (0, 1, 3),
            (1, 1, 0),
            (2, 1, 1),
            (3, 1, 2),
            (1, 2, 5),
        ];
        for (column, row, index) in layout {
            let face = panorama_sized(index, size, &stars);
            image::imageops::overlay(
                &mut sheet,
                &face,
                (column * size as usize) as i64,
                (row * size as usize) as i64,
            );
        }
        sheet.save(directory.join("sheet.png")).unwrap();
        pack_icon(&panorama_sized(0, size, &stars))
            .save(directory.join("pack.png"))
            .unwrap();
    }
}
