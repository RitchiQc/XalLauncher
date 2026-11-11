//! Rendu hors-ligne du panorama de branding XalLauncher (nébuleuse volumétrique).
//!
//! ```powershell
//! cargo run --release --example bake_branding -- --size 2048 --ss 2
//! ```
//!
//! Les PNG produits dans `assets/branding` sont embarqués dans le binaire du
//! launcher (`include_bytes!`), donc aucun rendu n'a lieu au lancement.

use image::{Rgba, RgbaImage};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

type Vec3 = [f32; 3];

const DEFAULT_SIZE: u32 = 2048;
const DEFAULT_SS: u32 = 2;
const VOLUME_SIZE: usize = 256;
const VOLUME_EXTENT: f32 = 1.8;
const STEPS: usize = 96;
const STAR_COUNT: usize = 7000;
const RAY_START: f32 = 0.52;
const RAY_END: f32 = 1.70;
const EMISSION: f32 = 14.0;
const ABSORPTION: f32 = 3.2;
const DUST_ABSORPTION: f32 = 5.0;

fn main() {
    let mut size = DEFAULT_SIZE;
    let mut ss = DEFAULT_SS;
    let mut out = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("assets")
        .join("branding");
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--size" => {
                size = args
                    .next()
                    .and_then(|value| value.parse().ok())
                    .unwrap_or(size)
            }
            "--ss" => {
                ss = args
                    .next()
                    .and_then(|value| value.parse().ok())
                    .unwrap_or(ss)
            }
            "--out" => out = args.next().map(PathBuf::from).unwrap_or(out),
            _ => {
                eprintln!("argument inconnu : {arg}");
                std::process::exit(2);
            }
        }
    }
    std::fs::create_dir_all(&out).unwrap();
    let preview_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("target")
        .join("panorama-preview");
    std::fs::create_dir_all(&preview_dir).unwrap();

    println!("construction du volume {VOLUME_SIZE}³…");
    let started = Instant::now();
    let volume = Arc::new(Volume::bake(VOLUME_SIZE));
    println!("volume prêt en {:?}", started.elapsed());

    let stars = make_stars();
    let visibility = stars
        .iter()
        .map(|star| transmittance(&volume, star.dir))
        .collect::<Vec<_>>();

    let mut previews = Vec::new();
    for index in 0..6u32 {
        let face_started = Instant::now();
        let face = render_face(index, size, ss, &volume, &stars, &visibility);
        let path = out.join(format!("panorama_{index}.png"));
        face.save(&path).unwrap();
        println!("{path:?} en {:?}", face_started.elapsed());
        let preview =
            image::imageops::resize(&face, 512, 512, image::imageops::FilterType::Lanczos3);
        preview
            .save(preview_dir.join(format!("panorama_{index}.png")))
            .unwrap();
        previews.push(preview);
    }

    pack_icon(&previews[0]).save(out.join("pack.png")).unwrap();
    sheet(&previews)
        .save(preview_dir.join("sheet.png"))
        .unwrap();
    println!("terminé en {:?}", started.elapsed());
}

fn face_basis(index: u32) -> (Vec3, Vec3, Vec3) {
    match index {
        1 => ([1.0, 0.0, 0.0], [0.0, 0.0, -1.0], [0.0, 1.0, 0.0]),
        2 => ([0.0, 0.0, -1.0], [-1.0, 0.0, 0.0], [0.0, 1.0, 0.0]),
        3 => ([-1.0, 0.0, 0.0], [0.0, 0.0, 1.0], [0.0, 1.0, 0.0]),
        4 => ([0.0, 1.0, 0.0], [1.0, 0.0, 0.0], [0.0, 0.0, -1.0]),
        5 => ([0.0, -1.0, 0.0], [1.0, 0.0, 0.0], [0.0, 0.0, 1.0]),
        _ => ([0.0, 0.0, 1.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]),
    }
}

fn direction(basis: (Vec3, Vec3, Vec3), u: f32, v: f32) -> Vec3 {
    let (forward, right, up) = basis;
    normalize([
        forward[0] + u * right[0] + v * up[0],
        forward[1] + u * right[1] + v * up[1],
        forward[2] + u * right[2] + v * up[2],
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
        sum += noise3([
            point[0] * frequency,
            point[1] * frequency,
            point[2] * frequency,
        ]) * amplitude;
        total += amplitude;
        amplitude *= 0.5;
        frequency *= 2.03;
    }
    sum / total
}

fn ridged(point: Vec3, octaves: u32) -> f32 {
    let mut sum = 0.0;
    let mut total = 0.0;
    let mut amplitude = 1.0;
    let mut frequency = 1.0;
    for _ in 0..octaves {
        let value = noise3([
            point[0] * frequency,
            point[1] * frequency,
            point[2] * frequency,
        ]);
        let ridge = 1.0 - (2.0 * value - 1.0).abs();
        sum += ridge * ridge * amplitude;
        total += amplitude;
        amplitude *= 0.5;
        frequency *= 2.11;
    }
    sum / total
}

struct Volume {
    size: usize,
    density: Vec<u8>,
    dust: Vec<u8>,
}

impl Volume {
    fn bake(size: usize) -> Self {
        let mut density = vec![0u8; size * size * size];
        let mut dust = vec![0u8; size * size * size];
        let scale = 2.0 * VOLUME_EXTENT / (size as f32 - 1.0);
        for z in 0..size {
            for y in 0..size {
                for x in 0..size {
                    let point = [
                        x as f32 * scale - VOLUME_EXTENT,
                        y as f32 * scale - VOLUME_EXTENT,
                        z as f32 * scale - VOLUME_EXTENT,
                    ];
                    let index = (z * size + y) * size + x;
                    density[index] = (nebula_field(point).clamp(0.0, 1.0) * 255.0).round() as u8;
                    dust[index] = (dust_field(point).clamp(0.0, 1.0) * 255.0).round() as u8;
                }
            }
        }
        Self {
            size,
            density,
            dust,
        }
    }

    fn sample_density(&self, point: Vec3) -> f32 {
        self.sample(&self.density, point)
    }

    fn sample_dust(&self, point: Vec3) -> f32 {
        self.sample(&self.dust, point)
    }

    fn sample(&self, data: &[u8], point: Vec3) -> f32 {
        let last = self.size as f32 - 1.0;
        let x = ((point[0] / VOLUME_EXTENT + 1.0) * 0.5 * last).clamp(0.0, last);
        let y = ((point[1] / VOLUME_EXTENT + 1.0) * 0.5 * last).clamp(0.0, last);
        let z = ((point[2] / VOLUME_EXTENT + 1.0) * 0.5 * last).clamp(0.0, last);
        let (x0, y0, z0) = (x.floor(), y.floor(), z.floor());
        let (tx, ty, tz) = (x - x0, y - y0, z - z0);
        let (x0, y0, z0) = (x0 as usize, y0 as usize, z0 as usize);
        let (x1, y1, z1) = (
            (x0 + 1).min(self.size - 1),
            (y0 + 1).min(self.size - 1),
            (z0 + 1).min(self.size - 1),
        );
        let at = |xi: usize, yi: usize, zi: usize| {
            data[(zi * self.size + yi) * self.size + xi] as f32 / 255.0
        };
        let x00 = lerp(at(x0, y0, z0), at(x1, y0, z0), tx);
        let x10 = lerp(at(x0, y1, z0), at(x1, y1, z0), tx);
        let x01 = lerp(at(x0, y0, z1), at(x1, y0, z1), tx);
        let x11 = lerp(at(x0, y1, z1), at(x1, y1, z1), tx);
        lerp(lerp(x00, x10, ty), lerp(x01, x11, ty), tz)
    }
}

fn nebula_field(point: Vec3) -> f32 {
    let warp = [
        fbm(
            [
                point[0] * 1.05 + 4.3,
                point[1] * 1.05 + 1.7,
                point[2] * 1.05 + 8.9,
            ],
            3,
        ),
        fbm(
            [
                point[0] * 1.05 + 12.1,
                point[1] * 1.05 + 7.4,
                point[2] * 1.05 + 2.6,
            ],
            3,
        ),
        fbm(
            [
                point[0] * 1.05 + 3.4,
                point[1] * 1.05 + 15.2,
                point[2] * 1.05 + 11.3,
            ],
            3,
        ),
    ];
    let q = [
        point[0] + (warp[0] - 0.5) * 0.95,
        point[1] + (warp[1] - 0.5) * 0.95,
        point[2] + (warp[2] - 0.5) * 0.95,
    ];
    let cloud = fbm([q[0] * 1.25, q[1] * 1.25, q[2] * 1.25], 4);
    let ridge = ridged([q[0] * 2.4 + 7.7, q[1] * 2.4 + 3.1, q[2] * 2.4 + 18.4], 4);
    let value = cloud * 0.42 + ridge * 0.58;
    let mask = smoothstep(
        0.26,
        0.62,
        fbm(
            [
                point[0] * 0.7 + 21.0,
                point[1] * 0.7 + 9.0,
                point[2] * 0.7 + 13.0,
            ],
            2,
        ),
    );
    let radius = dot(point, point).sqrt();
    let depth = smoothstep(0.62, 0.88, radius) * (1.0 - smoothstep(1.28, 1.66, radius));
    smoothstep(0.46, 0.80, value) * (0.12 + 0.88 * mask) * depth
}

fn dust_field(point: Vec3) -> f32 {
    let ridge = ridged(
        [
            point[0] * 1.1 + 40.0,
            point[1] * 1.1 + 8.0,
            point[2] * 1.1 + 26.0,
        ],
        3,
    );
    let mask = smoothstep(0.42, 0.72, ridge);
    let radius = dot(point, point).sqrt();
    let depth = smoothstep(0.45, 0.78, radius) * (1.0 - smoothstep(1.24, 1.62, radius));
    mask * depth
}

fn sky_tint(dir: Vec3) -> Vec3 {
    let hue = fbm(
        [
            dir[0] * 1.4 + 31.0,
            dir[1] * 1.4 + 17.0,
            dir[2] * 1.4 + 23.0,
        ],
        3,
    );
    let warm = smoothstep(
        0.52,
        0.86,
        fbm(
            [dir[0] * 2.2 + 5.0, dir[1] * 2.2 + 27.0, dir[2] * 2.2 + 11.0],
            2,
        ),
    );
    let cool = smoothstep(
        0.55,
        0.90,
        fbm(
            [dir[0] * 1.7 + 19.0, dir[1] * 1.7 + 3.0, dir[2] * 1.7 + 29.0],
            2,
        ),
    );
    let tint = lerp3(
        [0.42, 0.14, 0.82],
        [0.10, 0.26, 0.70],
        smoothstep(0.32, 0.76, hue),
    );
    let tint = lerp3(tint, [0.78, 0.20, 0.52], warm * 0.5);
    lerp3(tint, [0.10, 0.46, 0.46], cool * 0.3)
}

fn trace_sample(dir: Vec3, volume: &Volume, tint: Vec3) -> Vec3 {
    let step = (RAY_END - RAY_START) / STEPS as f32;
    let mut color = [0.0f32; 3];
    let mut trans = 1.0;
    for index in 0..STEPS {
        let distance = RAY_START + (index as f32 + 0.5) * step;
        let point = [dir[0] * distance, dir[1] * distance, dir[2] * distance];
        let density = volume.sample_density(point);
        let dust = volume.sample_dust(point);
        if density > 0.02 {
            let core = smoothstep(0.45, 0.95, density);
            let hot = core * core;
            let shape = 0.5 + 0.9 * core;
            let emission = [
                tint[0] * shape + 0.55 * hot,
                tint[1] * shape + 0.13 * hot,
                tint[2] * shape + 0.42 * hot,
            ];
            let weight = trans * density.powf(2.2) * step * EMISSION;
            color[0] += emission[0] * weight;
            color[1] += emission[1] * weight;
            color[2] += emission[2] * weight;
        }
        trans *= (-(density * ABSORPTION + dust * DUST_ABSORPTION) * step).exp();
        if trans < 0.015 {
            break;
        }
    }
    color
}

fn transmittance(volume: &Volume, dir: Vec3) -> f32 {
    let step = (RAY_END - RAY_START) / STEPS as f32;
    let mut trans = 1.0;
    for index in 0..STEPS {
        let distance = RAY_START + (index as f32 + 0.5) * step;
        let point = [dir[0] * distance, dir[1] * distance, dir[2] * distance];
        let density = volume.sample_density(point);
        let dust = volume.sample_dust(point);
        trans *= (-(density * ABSORPTION + dust * DUST_ABSORPTION) * step).exp();
    }
    trans.max(0.08)
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

fn make_stars() -> Vec<Star> {
    let mut rng = Rng(0x5A11_A110_C0FF_EE5B);
    let mut stars = Vec::with_capacity(STAR_COUNT);
    for _ in 0..STAR_COUNT {
        let z = rng.unit() * 2.0 - 1.0;
        let phi = rng.unit() * std::f32::consts::TAU;
        let ring = (1.0 - z * z).max(0.0).sqrt();
        let dir = [ring * phi.cos(), z, ring * phi.sin()];
        let magnitude = rng.unit();
        let intensity = 0.08 + magnitude.powf(3.6) * 1.1;
        let sigma = 0.00040 + magnitude.powf(3.0) * 0.0009;
        let tint = match rng.unit() {
            value if value > 0.88 => [1.0, 0.80, 0.62],
            value if value < 0.24 => [0.72, 0.82, 1.0],
            _ => [1.0, 0.97, 0.94],
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

fn draw_stars(
    buffer: &mut [Vec3],
    width: usize,
    basis: (Vec3, Vec3, Vec3),
    stars: &[Star],
    visibility: &[f32],
) {
    let size = width as f32;
    let radians_per_pixel = std::f32::consts::FRAC_PI_2 / size;
    let (forward, right, up) = basis;
    for (star, visible) in stars.iter().zip(visibility) {
        if *visible < 0.02 {
            continue;
        }
        let depth = dot(star.dir, forward);
        if depth <= 0.05 {
            continue;
        }
        let su = dot(star.dir, right) / depth;
        let sv = dot(star.dir, up) / depth;
        if su.abs() > 1.15 || sv.abs() > 1.15 {
            continue;
        }
        let center_x = (su * 0.5 + 0.5) * size;
        let center_y = (0.5 - sv * 0.5) * size;
        let reach = (star.sigma / radians_per_pixel) * 6.0 + 1.0;
        let x_start = ((center_x - reach).floor() as i32).max(0);
        let x_end = ((center_x + reach).ceil() as i32).min(width as i32 - 1);
        let y_start = ((center_y - reach).floor() as i32).max(0);
        let y_end = ((center_y + reach).ceil() as i32).min(width as i32 - 1);
        if x_start > x_end || y_start > y_end {
            continue;
        }
        let halo_sigma = star.sigma * 1.6;
        let gain = star.intensity * visible * 3.0;
        for py in y_start..=y_end {
            let v = 1.0 - 2.0 * (py as f32 + 0.5) / size;
            for px in x_start..=x_end {
                let u = 2.0 * (px as f32 + 0.5) / size - 1.0;
                let angle = dot(direction(basis, u, v), star.dir)
                    .clamp(-1.0, 1.0)
                    .acos();
                let core = (-0.5 * (angle / star.sigma).powi(2)).exp();
                let halo = (-0.5 * (angle / halo_sigma).powi(2)).exp() * 0.08;
                let value = gain * (core + halo);
                if value < 0.002 {
                    continue;
                }
                let pixel = &mut buffer[py as usize * width + px as usize];
                pixel[0] += star.tint[0] * value;
                pixel[1] += star.tint[1] * value;
                pixel[2] += star.tint[2] * value;
            }
        }
    }
}

fn trace_pixel(
    basis: (Vec3, Vec3, Vec3),
    x: u32,
    y: u32,
    size: u32,
    ss: u32,
    volume: &Volume,
) -> Vec3 {
    let mut color = [0.0f32; 3];
    for sy in 0..ss {
        for sx in 0..ss {
            let px = x as f32 + (sx as f32 + 0.5) / ss as f32;
            let py = y as f32 + (sy as f32 + 0.5) / ss as f32;
            let u = 2.0 * px / size as f32 - 1.0;
            let v = 1.0 - 2.0 * py / size as f32;
            let dir = direction(basis, u, v);
            let sample = trace_sample(dir, volume, sky_tint(dir));
            let detail = (0.60
                + 0.80
                    * fbm(
                        [
                            dir[0] * 19.0 + 3.0,
                            dir[1] * 19.0 + 11.0,
                            dir[2] * 19.0 + 7.0,
                        ],
                        3,
                    ))
                * (0.80
                    + 0.40
                        * fbm(
                            [
                                dir[0] * 47.0 + 13.0,
                                dir[1] * 47.0 + 5.0,
                                dir[2] * 47.0 + 29.0,
                            ],
                            2,
                        ));
            color[0] += sample[0] * detail;
            color[1] += sample[1] * detail;
            color[2] += sample[2] * detail;
        }
    }
    let count = (ss * ss) as f32;
    [color[0] / count, color[1] / count, color[2] / count]
}

fn render_face(
    index: u32,
    size: u32,
    ss: u32,
    volume: &Arc<Volume>,
    stars: &[Star],
    visibility: &[f32],
) -> RgbaImage {
    let basis = face_basis(index);
    let width = size as usize;
    let mut buffer = vec![[0.0f32; 3]; width * width];
    let threads = std::thread::available_parallelism()
        .map(|value| value.get())
        .unwrap_or(4)
        .min(24);
    let rows = width.div_ceil(threads);
    std::thread::scope(|scope| {
        for (band, chunk) in buffer.chunks_mut(rows * width).enumerate() {
            let volume = Arc::clone(volume);
            scope.spawn(move || {
                for (row, pixels) in chunk.chunks_mut(width).enumerate() {
                    let y = (band * rows + row) as u32;
                    for (x, pixel) in pixels.iter_mut().enumerate() {
                        *pixel = trace_pixel(basis, x as u32, y, size, ss, &volume);
                    }
                }
            });
        }
    });
    bloom(&mut buffer, width);
    draw_stars(&mut buffer, width, basis, stars, visibility);
    encode(&buffer, width)
}

fn bloom(buffer: &mut [Vec3], width: usize) {
    let scale = 4usize;
    let small = width / scale;
    let mut bright = vec![[0.0f32; 3]; small * small];
    for y in 0..small {
        for x in 0..small {
            let mut sum = [0.0f32; 3];
            for dy in 0..scale {
                for dx in 0..scale {
                    let pixel = buffer[(y * scale + dy) * width + x * scale + dx];
                    let luma = 0.2126 * pixel[0] + 0.7152 * pixel[1] + 0.0722 * pixel[2];
                    let keep = smoothstep(0.75, 1.8, luma);
                    sum[0] += pixel[0] * keep;
                    sum[1] += pixel[1] * keep;
                    sum[2] += pixel[2] * keep;
                }
            }
            let norm = (scale * scale) as f32;
            bright[y * small + x] = [sum[0] / norm, sum[1] / norm, sum[2] / norm];
        }
    }
    blur(&mut bright, small, 2);
    blur(&mut bright, small, 3);
    for y in 0..width {
        for x in 0..width {
            let color = sample2d(&bright, small, x, y, scale);
            let pixel = &mut buffer[y * width + x];
            pixel[0] += color[0] * 0.42;
            pixel[1] += color[1] * 0.42;
            pixel[2] += color[2] * 0.42;
        }
    }
}

fn blur(buffer: &mut [[f32; 3]], size: usize, radius: usize) {
    let mut temp = vec![[0.0f32; 3]; buffer.len()];
    let last = size as i32 - 1;
    for y in 0..size {
        for x in 0..size {
            let mut sum = [0.0f32; 3];
            for offset in -(radius as i32)..=(radius as i32) {
                let source = (x as i32 + offset).clamp(0, last) as usize;
                let pixel = buffer[y * size + source];
                sum[0] += pixel[0];
                sum[1] += pixel[1];
                sum[2] += pixel[2];
            }
            let count = (radius * 2 + 1) as f32;
            temp[y * size + x] = [sum[0] / count, sum[1] / count, sum[2] / count];
        }
    }
    for y in 0..size {
        for x in 0..size {
            let mut sum = [0.0f32; 3];
            for offset in -(radius as i32)..=(radius as i32) {
                let source = (y as i32 + offset).clamp(0, last) as usize;
                let pixel = temp[source * size + x];
                sum[0] += pixel[0];
                sum[1] += pixel[1];
                sum[2] += pixel[2];
            }
            let count = (radius * 2 + 1) as f32;
            buffer[y * size + x] = [sum[0] / count, sum[1] / count, sum[2] / count];
        }
    }
}

fn sample2d(buffer: &[[f32; 3]], size: usize, x: usize, y: usize, scale: usize) -> Vec3 {
    let last = size as f32 - 1.0;
    let sx = ((x as f32 + 0.5) / scale as f32 - 0.5).clamp(0.0, last);
    let sy = ((y as f32 + 0.5) / scale as f32 - 0.5).clamp(0.0, last);
    let x0 = sx.floor() as usize;
    let y0 = sy.floor() as usize;
    let x1 = (x0 + 1).min(size - 1);
    let y1 = (y0 + 1).min(size - 1);
    let tx = sx - x0 as f32;
    let ty = sy - y0 as f32;
    let top = lerp3(buffer[y0 * size + x0], buffer[y0 * size + x1], tx);
    let bottom = lerp3(buffer[y1 * size + x0], buffer[y1 * size + x1], tx);
    lerp3(top, bottom, ty)
}

fn encode(buffer: &[Vec3], width: usize) -> RgbaImage {
    let mut image = RgbaImage::new(width as u32, width as u32);
    for y in 0..width {
        for x in 0..width {
            let pixel = buffer[y * width + x];
            let dither = (bayer(x, y) - 0.5) / 255.0;
            let mut channels = [0u8; 3];
            for channel in 0..3 {
                let tone = (1.0 - (-pixel[channel] * 0.9).exp()).powf(1.15);
                let srgb = if tone <= 0.0031308 {
                    12.92 * tone
                } else {
                    1.055 * tone.powf(1.0 / 2.4) - 0.055
                };
                channels[channel] = ((srgb + dither).clamp(0.0, 1.0) * 255.0).round() as u8;
            }
            image.put_pixel(
                x as u32,
                y as u32,
                Rgba([channels[0], channels[1], channels[2], 255]),
            );
        }
    }
    image
}

fn bayer(x: usize, y: usize) -> f32 {
    const MATRIX: [[f32; 4]; 4] = [
        [0.0, 8.0, 2.0, 10.0],
        [12.0, 4.0, 14.0, 6.0],
        [3.0, 11.0, 1.0, 9.0],
        [15.0, 7.0, 13.0, 5.0],
    ];
    (MATRIX[y % 4][x % 4] + 0.5) / 16.0
}

fn pack_icon(background: &RgbaImage) -> RgbaImage {
    let mut icon =
        image::imageops::resize(background, 128, 128, image::imageops::FilterType::Lanczos3);
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

fn sheet(previews: &[RgbaImage]) -> RgbaImage {
    let size = previews[0].width();
    let mut sheet = RgbaImage::new(size * 4, size * 3);
    let layout = [
        (1usize, 0usize, 4usize),
        (0, 1, 3),
        (1, 1, 0),
        (2, 1, 1),
        (3, 1, 2),
        (1, 2, 5),
    ];
    for (column, row, index) in layout {
        image::imageops::overlay(
            &mut sheet,
            &previews[index],
            (column * size as usize) as i64,
            (row * size as usize) as i64,
        );
    }
    sheet
}
