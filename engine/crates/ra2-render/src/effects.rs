//! The match's particle effects and projected shadows, recovered from the exe.
//!
//! # Particles (`ParticleManager`, RTTI `0x6f53b0`)
//!
//! Every effect is an *emitter* (`FUN_005bab60`, a `0x340`-byte object whose
//! vtable is `0x6af570`) built from a config struct whose defaults are
//! `FUN_005ba7b0`. The fields used here, by config offset:
//!
//! | off | meaning | default |
//! |---|---|---|
//! | `0x00` / `0x10` byte, `0x14` | billboards spin / spin rate (rad/s) | off / 6 |
//! | `0x38` | spawn rate, particles per second | 5 |
//! | `0x48` | most particles alive | 100 |
//! | `0x4c` | fraction of a life after which colour and size start to fade | 0.5 |
//! | `0x54` | gravity (vector) | (0, -9.8, 0) |
//! | `0x70` | texture mode: 0 one, 1 random per particle, 2 animated | 0 |
//! | `0x84` byte | no alpha test | 1 |
//! | `0x90` / `0x94` | life and its spread, seconds | 2 / 0.5 |
//! | `0x98` / `0xa4` | start velocity and its per-axis spread | (0,14,0) / (1,0,1) |
//! | `0xb0` / `0xc0` / `0xd0` | start colour, end colour (RGBA), colour spread | (.2,.7,.2,1) / (.1,.5,.1,0) / (0,.3,0,0) |
//! | `0xe0` / `0xe4` / `0xe8` | start size, end size, size spread | 1 / 5 / 0 |
//! | `0xec` byte | emitting from the start | 1 |
//! | `0xed` / `0xee` byte | collides with the arena / reflects | 0 / 0 |
//! | `0xf0` | elasticity of the reflection | 1 |
//!
//! **Spawn** (`FUN_005bc250`): a particle starts at the emitter, lives
//! `life + spread * (r - 0.5)` seconds, moves at `velocity + spread * (r - 0.5)`
//! per axis and takes the start colour `+ spread * (r - 0.5)` per channel
//! (clamped to 0..1) and the start size `+ spread * (r - 0.5)`; `r` is the
//! engine's own LCG (`seed * 0x41c64e6d + 0x3039`, 31 bits, times `2^-31`).
//!
//! **Update** (`FUN_005bc8f0`): `pos += v dt + g dt^2 / 2`, then `v += g dt`.
//! With `p = 1 - remaining / life`, a particle keeps its start colour and size
//! until `p` passes the fade fraction, then both blend linearly to the end
//! values by `(p - fade) / (1 - fade)`; at `p > 1` it dies.
//!
//! **Draw** (`FUN_005bcf70`): a camera-facing quad, half-extent `size`, corners
//! `+right +up`, `+right -up`, `-right -up`, `-right +up` with `uv (1,0) (1,1)
//! (0,1) (0,0)`, diffuse = the particle colour, modulated with the texture. The
//! material is `FUN_005b24c0`'s: lighting off, alpha blend on, additive
//! (`SRCALPHA, ONE`) or ordinary (`SRCALPHA, INVSRCALPHA`) by effect kind
//! (`FUN_004a3380`: spark and flame additive; smoke, rubber and metal debris
//! ordinary).
//!
//! The effect kinds are `FUN_004a2c10` (sparks), `FUN_004a2e60` (metal
//! debris), `FUN_004a3120` (a smoke puff, `plus.emitSmoke`), `FUN_0048dc50`
//! (a motor's smoke) and `FUN_004b5760` (`plus.AddParticleEmitter`, the flame
//! hazard's flame). A one-shot effect emits for `0.1 s` at `count / 0.1`
//! particles per second (the manager entry's `+8` time, `FUN_004a3380`), so
//! `count` particles in all; the manager holds at most 20.
//!
//! **Not recovered:** the arena collision a particle tests (`FUN_005bdc00`
//! calls through a callback this build does not name). Particles marked
//! `collide` here bounce off the arena's declared shadow receiver triangles
//! (its floor and walls), which is the only arena surface list the scripts give.
//!
//! # Lightning (`LightningField`, RTTI `0x6f3cec`; the bolt is `FUN_0059e840`)
//!
//! `CreateLightning(index, a, b)` (`FUN_004b9ec0` -> `FUN_00403d40`) builds a
//! bolt object (vtable `0x6af08c`) from a config at `+0x248`: start, end, width
//! `0.03`, `5` segments, amplitude `0.4`, refresh `0.1 +- 0.05 s` (`+0x26c`,
//! `+0x270`). Its path (`FUN_0059ea10`) is `start`, four points `start + (end -
//! start) * i / 5` pushed sideways by `0.4 * (r - 0.5)` along the screen-space
//! perpendicular, and `end`; `FUN_0059ed40` redraws the offsets whenever the
//! refresh timer runs out. The bolt's two colours are blue `(0, 0, 1, 1)` and
//! white `(1, 1, 1, 1)` (`FUN_0059e8b0`), and the ribbon renderer
//! (`FUN_005b8090`) draws it camera-facing.
//!
//! **Not recovered:** how the ribbon renderer maps the config's widths (`0.03`
//! times 8, 2 and 2) and the `0.8` factor to the two layers. We draw a blue
//! ribbon of half-width `2 * width` under a white core of half-width `width / 5`
//! (the original's bolts read as saturated blue a pixel or two wide with
//! white only at the heart), both additive and untextured.
//!
//! # Shadows (`ShadowWizard`, RTTI `0x6fe950`)
//!
//! `SetShadowSource(x, y, z)` stores a point light (`FUN_004ee460`), every
//! `AddShadowTriangle` makes a receiver (`FUN_004ee490`, a `ShadowTrigger`
//! with its projection) and `FUN_004ee700` runs once a frame: for each caster
//! it projects the caster's triangles from the light onto every receiver,
//! clips them to the receiver triangle (`FUN_0061cb10`) and draws the result
//! as a triangle fan (`FUN_005986a0`) in `rgba (0, 0, 0, 0.3)` (`0x71d328`,
//! built by `FUN_0061b4b0(0,0,0,0.3)`) with the lit material `0x71eff0`: black
//! diffuse under the global ambient, so the shadow tints the floor with the
//! ambient grey (38/255) at 30 % (measured on `arena-box-f3`: floor (57,53,88)
//! under the shadow (53,48,74) is `0.7 * floor + 0.3 * 38`).

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use glam::{Mat4, Vec3};

use crate::material::{load_image, Image, Material, Stage, StageKind};
use crate::{DrawItem, Vertex};

/// The engine's random numbers: `FUN_005bc250` multiplies the global seed
/// (`0x726d58`) by `0x41c64e6d`, adds `0x3039`, masks to 31 bits and scales by
/// `2^-31`.
#[derive(Clone, Debug)]
pub struct Rng(pub u32);

impl Rng {
    /// The next number in `[0, 1)`.
    pub fn unit(&mut self) -> f32 {
        self.0 = self.0.wrapping_mul(0x41c6_4e6d).wrapping_add(0x3039);
        ((self.0 & 0x7fff_ffff) as f64 * 4.656_612_873_077_393e-10) as f32
    }

    /// `base + spread * (r - 0.5)`, the spawn jitter every field uses.
    fn jitter(&mut self, base: f32, spread: f32) -> f32 {
        base + spread * (self.unit() - 0.5)
    }
}

/// How an emitter picks a texture (`cfg + 0x70`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextureMode {
    /// One texture.
    Single,
    /// A random one per particle.
    Random,
    /// The particle steps through them, `frame_period` seconds each.
    Animated,
}

/// One emitter's recovered configuration.
#[derive(Clone, Debug)]
pub struct Config {
    pub rate: f32,
    pub max: usize,
    pub fade_start: f32,
    pub gravity: Vec3,
    pub texture_mode: TextureMode,
    pub frame_period: f32,
    pub life: f32,
    pub life_spread: f32,
    pub velocity: Vec3,
    pub velocity_spread: Vec3,
    pub color_start: [f32; 4],
    pub color_end: [f32; 4],
    pub color_spread: [f32; 4],
    pub size_start: f32,
    pub size_end: f32,
    pub size_spread: f32,
    pub spin: bool,
    pub spin_rate: f32,
    pub collide: bool,
    pub reflect: bool,
    pub elasticity: f32,
    /// `Arenas\particles\<name>` textures.
    pub textures: &'static [&'static str],
    /// `SRCALPHA, ONE` instead of `SRCALPHA, INVSRCALPHA`.
    pub additive: bool,
}

const SMOKE_TEXTURES: &[&str] = &["smoke.tga", "smoke2.tga", "smoke3.tga"];
const DEBRIS_TEXTURES: &[&str] = &["metal.tga", "metalslice.tga", "metal_nutbolt.tga"];

impl Config {
    /// `FUN_005ba7b0`'s defaults.
    pub fn base() -> Config {
        Config {
            rate: 5.0,
            max: 100,
            fade_start: 0.5,
            gravity: Vec3::new(0.0, -9.8, 0.0),
            texture_mode: TextureMode::Single,
            frame_period: 1.0,
            life: 2.0,
            life_spread: 0.5,
            velocity: Vec3::new(0.0, 14.0, 0.0),
            velocity_spread: Vec3::new(1.0, 0.0, 1.0),
            color_start: [0.2, 0.7, 0.2, 1.0],
            color_end: [0.1, 0.5, 0.1, 0.0],
            color_spread: [0.0, 0.3, 0.0, 0.0],
            size_start: 1.0,
            size_end: 5.0,
            size_spread: 0.0,
            spin: false,
            spin_rate: 6.0,
            collide: false,
            reflect: false,
            elasticity: 1.0,
            textures: &[],
            additive: true,
        }
    }

    /// `FUN_004a2c10`: the sparks of a hit. `count` particles over the
    /// emitter's `0.1 s`; `velocity` and `spread` are the contact normal times
    /// two and `(1, 1, 1)` in `FUN_004a2a50`.
    pub fn spark(count: i32, velocity: Vec3, spread: Vec3) -> Config {
        Config {
            rate: count as f32 / 0.1,
            life: 0.7,
            velocity,
            velocity_spread: spread,
            color_start: [1.0; 4],
            color_end: [1.0, 1.0, 1.0, 0.0],
            size_start: 0.05,
            size_end: 0.05,
            collide: true,
            reflect: true,
            textures: &["spark.tga"],
            additive: true,
            ..Config::base()
        }
    }

    /// `FUN_004a2e60`: the metal bits of a part that came off (`4a2a50` with
    /// its last argument set; velocity is the normal times three).
    pub fn debris(count: i32, velocity: Vec3, spread: Vec3) -> Config {
        Config {
            rate: count as f32 / 0.1,
            life: 1.0,
            velocity,
            velocity_spread: spread,
            gravity: Vec3::new(0.0, -13.0, 0.0),
            texture_mode: TextureMode::Random,
            color_start: [1.0; 4],
            color_end: [1.0, 1.0, 1.0, 0.0],
            size_start: 0.08,
            size_end: 0.08,
            size_spread: 0.02,
            spin: true,
            collide: true,
            reflect: true,
            textures: DEBRIS_TEXTURES,
            additive: false,
            ..Config::base()
        }
    }

    /// `FUN_004a3120` (`plus.emitSmoke`): a puff that grows from `0.3` to `1`
    /// and does not fade.
    pub fn smoke_puff(count: i32, velocity: Vec3, spread: Vec3) -> Config {
        Config {
            rate: count as f32 / 0.1,
            life: 1.0,
            velocity,
            velocity_spread: spread,
            gravity: Vec3::ZERO,
            texture_mode: TextureMode::Random,
            color_start: [1.0; 4],
            color_end: [1.0; 4],
            size_start: 0.3,
            size_end: 1.0,
            textures: SMOKE_TEXTURES,
            additive: false,
            ..Config::base()
        }
    }

    /// `FUN_004f6a10`: the smoke a car-steering unit under half its hitpoints
    /// gives off, a thinner cousin of [`Config::motor_smoke`]: one texture
    /// (`smoke.tga`), rising at `0.7` with a spread of `0.3`.
    pub fn steering_smoke() -> Config {
        Config {
            velocity: Vec3::new(0.0, 0.7, 0.0),
            velocity_spread: Vec3::new(0.3, 0.0, 0.3),
            texture_mode: TextureMode::Single,
            textures: &["smoke.tga"],
            ..Config::motor_smoke()
        }
    }

    /// `FUN_0048dc50`: the smoke a motor under half its hitpoints gives off.
    pub fn motor_smoke() -> Config {
        Config {
            rate: 5.0,
            life: 0.7,
            velocity: Vec3::new(0.0, 2.0, 0.0),
            velocity_spread: Vec3::new(0.5, 0.0, 0.5),
            gravity: Vec3::ZERO,
            texture_mode: TextureMode::Random,
            color_start: [1.0; 4],
            color_end: [1.0, 1.0, 1.0, 0.0],
            size_start: 0.2,
            size_end: 0.6,
            textures: SMOKE_TEXTURES,
            additive: false,
            ..Config::base()
        }
    }

    /// `FUN_004b5760` (`plus.AddParticleEmitter(location, velocity, variance)`):
    /// the flame hazard. The colour runs from blue to white over the second
    /// half of a life, so the orange texture comes up only as it fades in.
    pub fn flame(velocity: Vec3, variance: Vec3) -> Config {
        Config {
            rate: 80.0,
            life: 0.3,
            life_spread: 0.15,
            velocity,
            velocity_spread: variance,
            gravity: Vec3::ZERO,
            color_start: [0.0, 0.0, 1.0, 1.0],
            color_end: [1.0; 4],
            size_start: 0.2,
            size_end: 0.4,
            textures: &["flame.tga"],
            additive: true,
            ..Config::base()
        }
    }
}

#[derive(Clone, Debug)]
struct Particle {
    pos: Vec3,
    vel: Vec3,
    total: f32,
    remaining: f32,
    color: [f32; 4],
    color_start: [f32; 4],
    size: f32,
    size_start: f32,
    angle: f32,
    texture: usize,
    anim: f32,
    alive: bool,
}

/// One live emitter.
#[derive(Clone, Debug)]
pub struct Emitter {
    cfg: Config,
    pos: Vec3,
    particles: Vec<Particle>,
    accumulator: f32,
    emitting: bool,
    /// Seconds of emission left (`f32::INFINITY` for a standing emitter).
    emit_left: f32,
}

impl Emitter {
    fn new(cfg: Config, pos: Vec3, emitting: bool, emit_left: f32) -> Emitter {
        Emitter {
            cfg,
            pos,
            particles: Vec::new(),
            accumulator: 0.0,
            emitting,
            emit_left,
        }
    }

    /// Particles alive.
    pub fn alive(&self) -> usize {
        self.particles.iter().filter(|p| p.alive).count()
    }

    fn spawn(&mut self, n: usize, rng: &mut Rng, textures: usize) -> usize {
        let c = &self.cfg;
        let room = c.max.saturating_sub(self.alive());
        let n = n.min(room);
        for _ in 0..n {
            let life = rng.jitter(c.life, c.life_spread);
            let mut color = [0.0; 4];
            // The original draws alpha, blue, green, red in that order; the
            // channels are independent so the order only matters to the seed.
            for k in [3, 2, 1, 0] {
                color[k] = rng
                    .jitter(c.color_start[k], c.color_spread[k])
                    .clamp(0.0, 1.0);
            }
            let size = rng.jitter(c.size_start, c.size_spread);
            let vel = Vec3::new(
                rng.jitter(c.velocity.x, c.velocity_spread.x),
                rng.jitter(c.velocity.y, c.velocity_spread.y),
                rng.jitter(c.velocity.z, c.velocity_spread.z),
            );
            let texture = match c.texture_mode {
                TextureMode::Single => 0,
                TextureMode::Random | TextureMode::Animated if textures == 0 => 0,
                TextureMode::Random => ((rng.unit() * textures as f32) as usize).min(textures - 1),
                TextureMode::Animated => 0,
            };
            self.particles.push(Particle {
                pos: self.pos,
                vel,
                total: life,
                remaining: life,
                color,
                color_start: color,
                size,
                size_start: size,
                angle: 0.0,
                texture,
                anim: 0.0,
                alive: true,
            });
        }
        n
    }

    fn step(&mut self, dt: f32, rng: &mut Rng, textures: usize, collider: &Collider) {
        if self.emitting {
            if self.emit_left.is_finite() {
                self.emit_left -= dt;
                if self.emit_left <= 0.0 {
                    self.emitting = false;
                }
            }
            if self.emitting {
                self.accumulator += self.cfg.rate * dt;
                let n = self.accumulator as usize;
                if n > 0 {
                    let spawned = self.spawn(n, rng, textures);
                    self.accumulator -= spawned as f32;
                }
            }
        }
        let c = self.cfg.clone();
        for p in self.particles.iter_mut().filter(|p| p.alive) {
            if c.spin {
                p.angle += c.spin_rate * dt;
            }
            let from = p.pos;
            p.pos += p.vel * dt + 0.5 * c.gravity * dt * dt;
            p.vel += c.gravity * dt;
            if c.collide && c.reflect {
                if let Some((hit, normal)) = collider.hit(from, p.pos) {
                    let reflected = p.vel - 2.0 * p.vel.dot(normal) * normal;
                    p.vel = reflected * c.elasticity;
                    p.pos = hit + normal * 0.002;
                }
            }
            if c.life > 0.0 {
                p.remaining -= dt;
                let progress = 1.0 - p.remaining / p.total;
                if progress > c.fade_start {
                    let f = if progress <= 1.0 {
                        (progress - c.fade_start) / (1.0 - c.fade_start)
                    } else {
                        p.alive = false;
                        1.0
                    };
                    for k in 0..4 {
                        p.color[k] = p.color_start[k] + (c.color_end[k] - p.color_start[k]) * f;
                    }
                    p.size = p.size_start + (c.size_end - p.size_start) * f;
                }
            }
            if c.texture_mode == TextureMode::Animated && textures > 0 {
                p.anim += dt;
                while p.anim >= c.frame_period {
                    p.anim -= c.frame_period;
                    p.texture = (p.texture + 1) % textures;
                }
            }
        }
        self.particles.retain(|p| p.alive);
    }

    fn finished(&self) -> bool {
        !self.emitting && self.particles.is_empty()
    }
}

/// The arena surfaces particles bounce off.
#[derive(Clone, Debug, Default)]
pub struct Collider {
    triangles: Vec<[Vec3; 3]>,
}

impl Collider {
    /// The first triangle the segment `from -> to` crosses, as `(point, normal)`
    /// with the normal facing the side `from` is on.
    pub fn hit(&self, from: Vec3, to: Vec3) -> Option<(Vec3, Vec3)> {
        let d = to - from;
        let mut best: Option<(f32, Vec3)> = None;
        for t in &self.triangles {
            let e1 = t[1] - t[0];
            let e2 = t[2] - t[0];
            let n = e1.cross(e2);
            let len = n.length();
            if len < 1e-12 {
                continue;
            }
            let n = n / len;
            let denom = n.dot(d);
            if denom.abs() < 1e-9 {
                continue;
            }
            let s = n.dot(t[0] - from) / denom;
            if !(0.0..=1.0).contains(&s) {
                continue;
            }
            let p = from + d * s;
            if point_in_triangle(p, t) && best.is_none_or(|(b, _)| s < b) {
                let facing = if n.dot(from - t[0]) >= 0.0 { n } else { -n };
                best = Some((s, facing));
            }
        }
        best.map(|(s, n)| (from + d * s, n))
    }
}

fn point_in_triangle(p: Vec3, t: &[Vec3; 3]) -> bool {
    let v0 = t[2] - t[0];
    let v1 = t[1] - t[0];
    let v2 = p - t[0];
    let (d00, d01, d02) = (v0.dot(v0), v0.dot(v1), v0.dot(v2));
    let (d11, d12) = (v1.dot(v1), v1.dot(v2));
    let inv = d00 * d11 - d01 * d01;
    if inv.abs() < 1e-18 {
        return false;
    }
    let u = (d11 * d02 - d01 * d12) / inv;
    let v = (d00 * d12 - d01 * d02) / inv;
    u >= -1e-4 && v >= -1e-4 && u + v <= 1.0 + 1e-4
}

/// Segments of a lightning bolt (`FUN_00403d40`'s config `+0x1c`).
pub const BOLT_SEGMENTS: usize = 5;
/// Sideways reach of a bolt's inner points (config `+0x20`).
pub const BOLT_AMPLITUDE: f32 = 0.4;
/// A bolt's width (config `+0x18`).
pub const BOLT_WIDTH: f32 = 0.03;
/// Mean and spread of the seconds between a bolt's redraws (`+0x26c`, `+0x270`).
pub const BOLT_REFRESH: (f32, f32) = (0.1, 0.1);

#[derive(Clone, Debug)]
struct Bolt {
    start: Vec3,
    end: Vec3,
    visible: bool,
    timer: f32,
    offsets: [f32; BOLT_SEGMENTS - 1],
}

/// How many particles a hit of `damage` throws (`FUN_0045f9f0` over
/// `FUN_004194f0`): `damage` from `10..100` maps linearly to `3..hi` and is
/// clamped, `hi` being `20` at `GraphicDetail = 2` and `10` below it. Live: damage
/// 12.34, 16.70, 38.39, 71.06 and 115.1 gave 3.44, 4.27, 8.36, 14.53 and 20.
pub fn hit_count(damage: f32, detail: u32) -> f32 {
    let hi = if detail >= 2 { 20.0 } else { 10.0 };
    let t = ((damage - 10.0) / 90.0).clamp(0.0, 1.0);
    (hi - 3.0) * t + 3.0
}

/// The most one-shot effects the manager keeps (`FUN_004a3380`: `< 0x14`).
pub const MAX_EFFECTS: usize = 20;
/// How long a one-shot effect emits (`FUN_004a3380`'s `0x3dcccccd`).
pub const EMIT_SECONDS: f32 = 0.1;

/// The live particle effects of a match.
pub struct Effects {
    game: Option<PathBuf>,
    rng: Rng,
    /// `GraphicEffects` (`FUN_0046e5e0() == 1`).
    pub enabled: bool,
    /// `GraphicDetail` (`0x7164f8 + 0x54`), which caps the spark count.
    pub detail: u32,
    bursts: Vec<Emitter>,
    standing: Vec<(u64, Emitter)>,
    textures: Mutex<HashMap<&'static str, Arc<Image>>>,
    collider: Collider,
    bolts: Vec<(i32, Bolt)>,
}

impl Effects {
    /// Effects reading their textures from `<game>/Arenas/particles`. Without a
    /// game directory nothing is drawn.
    pub fn new(game: Option<&Path>) -> Effects {
        Effects {
            game: game.map(Path::to_path_buf),
            rng: Rng(0x1234_5678),
            enabled: true,
            detail: 2,
            bursts: Vec::new(),
            standing: Vec::new(),
            textures: Mutex::new(HashMap::new()),
            collider: Collider::default(),
            bolts: Vec::new(),
        }
    }

    /// The game directory the textures come from, if any.
    pub fn game_dir(&self) -> Option<&Path> {
        self.game.as_deref()
    }

    /// A car-steering unit's smoke, `key` naming the unit.
    pub fn steering_smoke(&mut self, key: u64, point: Vec3, on: bool) {
        self.standing_emitter(key, Config::steering_smoke, point, on);
    }

    /// The surfaces sparks and debris bounce off (the arena's receivers).
    pub fn set_collision(&mut self, triangles: Vec<[Vec3; 3]>) {
        self.collider = Collider { triangles };
    }

    /// Whether anything is alive.
    pub fn is_empty(&self) -> bool {
        self.bursts.is_empty()
            && self.standing.is_empty()
            && !self.bolts.iter().any(|(_, b)| b.visible)
    }

    fn texture(&self, name: &'static str) -> Option<Arc<Image>> {
        let mut cache = self.textures.lock().ok()?;
        if let Some(t) = cache.get(name) {
            return Some(t.clone());
        }
        let root = self.game.as_ref()?;
        let rel = format!("Arenas/particles/{name}");
        let path = crate::resolve_ci(root, &rel).unwrap_or_else(|| root.join(rel));
        let image = load_image(&path)?;
        cache.insert(name, image.clone());
        Some(image)
    }

    fn burst(&mut self, cfg: Config, pos: Vec3) {
        if !self.enabled {
            return;
        }
        if self.bursts.len() >= MAX_EFFECTS {
            // A full manager reuses an entry whose time ran out.
            if let Some(i) = self.bursts.iter().position(|e| !e.emitting) {
                self.bursts.remove(i);
            } else {
                return;
            }
        }
        self.bursts.push(Emitter::new(cfg, pos, true, EMIT_SECONDS));
    }

    /// The sparks of a hit of `damage > 10` at `point` (`FUN_004a2a50`):
    /// [`hit_count`] particles flying along `normal * 2`, where `normal` is the
    /// hit record's own (into the struck component).
    pub fn sparks(&mut self, damage: f32, point: Vec3, normal: Vec3) {
        let count = hit_count(damage, self.detail) as i32;
        if count > 0 {
            self.burst(Config::spark(count, normal * 2.0, Vec3::ONE), point);
        }
    }

    /// The metal bits a chassis hit throws besides the sparks (`FUN_0044ee70` calls
    /// `FUN_004a2a50` a second time with its debris flag): a quarter of the
    /// spark count (`0x6a54c0`), truncated, flying along `normal * 6` (the chooser
    /// scales its vector in place by `2` and then by `3`).
    pub fn debris(&mut self, damage: f32, point: Vec3, normal: Vec3) {
        let count = (hit_count(damage, self.detail) * 0.25) as i32;
        if count > 0 {
            self.burst(Config::debris(count, normal * 6.0, Vec3::ONE), point);
        }
    }

    /// `plus.emitSmoke`: a puff of `count` particles.
    pub fn smoke_puff(&mut self, count: i32, point: Vec3, velocity: Vec3, spread: Vec3) {
        if count > 0 {
            self.burst(Config::smoke_puff(count, velocity, spread), point);
        }
    }

    /// A motor's smoke: `key` names the motor, `on` is `Robot::smoking`.
    pub fn motor_smoke(&mut self, key: u64, point: Vec3, on: bool) {
        self.standing_emitter(key, Config::motor_smoke, point, on);
    }

    /// A flame hazard's emitter (`plus.AddParticleEmitter` + `SetEmitting`).
    pub fn flame(&mut self, key: u64, point: Vec3, velocity: Vec3, variance: Vec3, on: bool) {
        self.standing_emitter(key, || Config::flame(velocity, variance), point, on);
    }

    fn standing_emitter(&mut self, key: u64, cfg: impl FnOnce() -> Config, point: Vec3, on: bool) {
        if !self.enabled {
            return;
        }
        if let Some((_, e)) = self.standing.iter_mut().find(|(k, _)| *k == key) {
            e.pos = point;
            e.emitting = on;
            return;
        }
        if on {
            self.standing
                .push((key, Emitter::new(cfg(), point, true, f32::INFINITY)));
        }
    }

    /// Set the arena's lightning bolts (`CreateLightning`, `SetLightningStartEnd`,
    /// `SetLightningVisible`): `(index, start, end, visible)`. A bolt keeps its
    /// jitter and refresh timer while its index lives.
    pub fn set_lightning(&mut self, bolts: impl IntoIterator<Item = (i32, Vec3, Vec3, bool)>) {
        let old = std::mem::take(&mut self.bolts);
        for (index, start, end, visible) in bolts {
            let mut bolt = old
                .iter()
                .find(|(i, _)| *i == index)
                .map(|(_, b)| b.clone())
                .unwrap_or(Bolt {
                    start,
                    end,
                    visible,
                    timer: 0.0,
                    offsets: [0.0; BOLT_SEGMENTS - 1],
                });
            bolt.start = start;
            bolt.end = end;
            bolt.visible = visible;
            self.bolts.push((index, bolt));
        }
    }

    /// Advance every emitter by `dt` seconds.
    pub fn step(&mut self, dt: f32) {
        if dt <= 0.0 {
            return;
        }
        for (_, b) in self.bolts.iter_mut().filter(|(_, b)| b.visible) {
            b.timer -= dt;
            if b.timer <= 0.0 {
                for o in &mut b.offsets {
                    *o = BOLT_AMPLITUDE * (self.rng.unit() - 0.5);
                }
                b.timer = self.rng.jitter(BOLT_REFRESH.0, BOLT_REFRESH.1);
            }
        }
        let collider = self.collider.clone();
        let rng = &mut self.rng;
        for e in &mut self.bursts {
            let n = e.cfg.textures.len();
            e.step(dt, rng, n, &collider);
        }
        self.bursts.retain(|e| !e.finished());
        for (_, e) in &mut self.standing {
            let n = e.cfg.textures.len();
            e.step(dt, rng, n, &collider);
        }
    }

    /// The draw items of this frame, billboarded to `right` and `up` (the
    /// camera's axes in the world).
    pub fn items(&self, right: Vec3, up: Vec3) -> Vec<DrawItem> {
        let mut out = Vec::new();
        for e in self
            .bursts
            .iter()
            .chain(self.standing.iter().map(|(_, e)| e))
        {
            if e.particles.is_empty() {
                continue;
            }
            for (index, name) in e.cfg.textures.iter().enumerate() {
                let Some(image) = self.texture(name) else {
                    continue;
                };
                let mut vertices = Vec::new();
                for p in e.particles.iter().filter(|p| p.alive && p.texture == index) {
                    quad(&mut vertices, p, right, up);
                }
                if vertices.is_empty() {
                    continue;
                }
                out.push(DrawItem::new(
                    vertices,
                    particle_material(image, e.cfg.additive),
                    format!("particles:{name}"),
                ));
            }
        }
        out.extend(self.bolt_items(right, up));
        out
    }

    /// The visible bolts as camera-facing ribbons: blue under white.
    fn bolt_items(&self, right: Vec3, up: Vec3) -> Option<DrawItem> {
        let forward = right.cross(up).normalize_or_zero();
        let mut vertices = Vec::new();
        for (_, b) in self.bolts.iter().filter(|(_, b)| b.visible) {
            let path = b.path(forward);
            for (colour, half) in [
                ([0.0, 0.0, 1.0, 1.0], 2.0 * BOLT_WIDTH),
                ([1.0; 4], 0.2 * BOLT_WIDTH),
            ] {
                ribbon(&mut vertices, &path, forward, half, colour);
            }
        }
        if vertices.is_empty() {
            return None;
        }
        let mut material = particle_material(Image::solid([255; 4]), true);
        material.stages[0].linear = false;
        Some(DrawItem::new(vertices, material, "lightning"))
    }
}

/// The right and up axes of a camera in the world, from its view-projection
/// (`proj * view`, left-handed): the first two rows of the product are the
/// camera's axes scaled by the lens.
pub fn camera_axes(view_proj: Mat4) -> (Vec3, Vec3) {
    let m = view_proj;
    let row = |r: usize| Vec3::new(m.col(0)[r], m.col(1)[r], m.col(2)[r]).normalize_or_zero();
    (row(0), row(1))
}

impl Bolt {
    /// `FUN_0059ea10`: start, the inner points pushed along the screen-space
    /// perpendicular, end.
    fn path(&self, forward: Vec3) -> Vec<Vec3> {
        let along = self.end - self.start;
        let perp = along.cross(forward).normalize_or_zero();
        let mut pts = vec![self.start];
        for (i, o) in self.offsets.iter().enumerate() {
            let t = (i + 1) as f32 / BOLT_SEGMENTS as f32;
            pts.push(self.start + along * t + perp * *o);
        }
        pts.push(self.end);
        pts
    }
}

/// A camera-facing strip along `path`, `half` either side of it.
fn ribbon(out: &mut Vec<Vertex>, path: &[Vec3], forward: Vec3, half: f32, colour: [f32; 4]) {
    let side = |k: usize| {
        let a = path[k.saturating_sub(1)];
        let b = path[(k + 1).min(path.len() - 1)];
        (b - a).cross(forward).normalize_or_zero() * half
    };
    let v = |pos: Vec3, uv: [f32; 2]| {
        let mut vx = Vertex::new(pos, -forward, uv);
        vx.color = colour;
        vx
    };
    for k in 0..path.len() - 1 {
        let (s0, s1) = (side(k), side(k + 1));
        let a = v(path[k] + s0, [0.0, 0.0]);
        let b = v(path[k] - s0, [0.0, 1.0]);
        let c = v(path[k + 1] - s1, [1.0, 1.0]);
        let d = v(path[k + 1] + s1, [1.0, 0.0]);
        out.extend([a, b, c, a, c, d]);
    }
}

fn particle_material(image: Arc<Image>, additive: bool) -> Material {
    Material {
        stages: vec![Stage {
            image,
            kind: StageKind::Modulate,
            linear: true,
            uv_set: 0,
        }],
        transparent: true,
        no_alpha_test: true,
        additive,
        has_vertex_colors: true,
        lighting: false,
        z_write: false,
        cull: false,
        ..Material::default()
    }
}

/// `FUN_005bcf70`'s quad: half-extent `size`, uv `(1,0) (1,1) (0,1) (0,0)` at
/// `+right+up`, `+right-up`, `-right-up`, `-right+up`. A spinning emitter turns
/// the axes by the particle's angle.
fn quad(out: &mut Vec<Vertex>, p: &Particle, right: Vec3, up: Vec3) {
    let (right, up) = if p.angle != 0.0 {
        let (s, c) = p.angle.sin_cos();
        (right * c + up * s, up * c - right * s)
    } else {
        (right, up)
    };
    let r = right * p.size;
    let u = up * p.size;
    let n = right.cross(up);
    let v = |pos: Vec3, uv: [f32; 2]| {
        let mut vx = Vertex::new(pos, n, uv);
        vx.color = p.color;
        vx
    };
    let a = v(p.pos + r + u, [1.0, 0.0]);
    let b = v(p.pos + r - u, [1.0, 1.0]);
    let c = v(p.pos - r - u, [0.0, 1.0]);
    let d = v(p.pos - r + u, [0.0, 0.0]);
    out.extend([a, b, c, a, c, d]);
}

// ---------------------------------------------------------------------------
// shadows
// ---------------------------------------------------------------------------

/// The shadow colour the original draws: black with `alpha 0.3`.
pub const SHADOW_RGBA: [f32; 4] = [0.0, 0.0, 0.0, 0.3];
/// How far a shadow is lifted off its receiver to keep clear of z-fighting.
const SHADOW_LIFT: f32 = 0.01;

/// Project `casters` (world-space point clouds, one per robot) from `light`
/// onto every receiver triangle, clip to the receiver, and return the shadow
/// as one blended item (`None` when nothing falls on a receiver).
pub fn shadow_item(
    light: Vec3,
    receivers: &[[Vec3; 3]],
    casters: &[Vec<Vec3>],
) -> Option<DrawItem> {
    let mut vertices = Vec::new();
    for tri in receivers {
        let centre = (tri[0] + tri[1] + tri[2]) / 3.0;
        let mut n = (tri[1] - tri[0]).cross(tri[2] - tri[0]).normalize_or_zero();
        if n == Vec3::ZERO {
            continue;
        }
        // A receiver faces the light whatever way it was wound.
        if n.dot(light - centre) < 0.0 {
            n = -n;
        }
        // Plane basis for the 2D work.
        let helper = if n.x.abs() < 0.9 { Vec3::X } else { Vec3::Y };
        let bu = n.cross(helper).normalize();
        let bv = n.cross(bu);
        let to2 = |p: Vec3| [bu.dot(p - centre), bv.dot(p - centre)];
        let tri2 = [to2(tri[0]), to2(tri[1]), to2(tri[2])];
        for caster in casters {
            let mut pts = Vec::new();
            for &p in caster {
                // The light-to-plane parameter: the point must lie between.
                let d = p - light;
                let denom = n.dot(d);
                if denom >= -1e-6 {
                    continue;
                }
                let t = n.dot(centre - light) / denom;
                if t < 1.0 {
                    continue; // the point is behind the receiver
                }
                pts.push(to2(light + d * t));
            }
            if pts.len() < 3 {
                continue;
            }
            let hull = convex_hull(pts);
            let clipped = clip_to_triangle(&hull, &tri2);
            if clipped.len() < 3 {
                continue;
            }
            let lift = |q: [f32; 2]| centre + bu * q[0] + bv * q[1] + n * SHADOW_LIFT;
            let first = lift(clipped[0]);
            for w in 1..clipped.len() - 1 {
                for pos in [first, lift(clipped[w]), lift(clipped[w + 1])] {
                    let mut v = Vertex::new(pos, n, [0.0, 0.0]);
                    v.color = SHADOW_RGBA;
                    vertices.push(v);
                }
            }
        }
    }
    if vertices.is_empty() {
        return None;
    }
    let material = Material {
        stages: vec![Stage {
            image: Image::solid([255; 4]),
            kind: StageKind::Modulate,
            linear: false,
            uv_set: 0,
        }],
        transparent: true,
        no_alpha_test: true,
        has_vertex_colors: true,
        lighting: true,
        z_write: false,
        cull: false,
        ..Material::default()
    };
    Some(DrawItem::new(vertices, material, "shadows"))
}

/// Andrew's monotone chain, counter-clockwise.
fn convex_hull(mut pts: Vec<[f32; 2]>) -> Vec<[f32; 2]> {
    pts.sort_by(|a, b| a[0].total_cmp(&b[0]).then(a[1].total_cmp(&b[1])));
    pts.dedup();
    if pts.len() < 3 {
        return pts;
    }
    let cross = |o: [f32; 2], a: [f32; 2], b: [f32; 2]| {
        (a[0] - o[0]) * (b[1] - o[1]) - (a[1] - o[1]) * (b[0] - o[0])
    };
    let mut hull: Vec<[f32; 2]> = Vec::new();
    for &p in &pts {
        while hull.len() >= 2 && cross(hull[hull.len() - 2], hull[hull.len() - 1], p) <= 0.0 {
            hull.pop();
        }
        hull.push(p);
    }
    let lower = hull.len() + 1;
    for &p in pts.iter().rev().skip(1) {
        while hull.len() >= lower && cross(hull[hull.len() - 2], hull[hull.len() - 1], p) <= 0.0 {
            hull.pop();
        }
        hull.push(p);
    }
    hull.pop();
    hull
}

/// Sutherland-Hodgman against a triangle (any winding).
fn clip_to_triangle(poly: &[[f32; 2]], tri: &[[f32; 2]; 3]) -> Vec<[f32; 2]> {
    let area = (tri[1][0] - tri[0][0]) * (tri[2][1] - tri[0][1])
        - (tri[1][1] - tri[0][1]) * (tri[2][0] - tri[0][0]);
    let sign = if area >= 0.0 { 1.0 } else { -1.0 };
    let mut out: Vec<[f32; 2]> = poly.to_vec();
    for e in 0..3 {
        let a = tri[e];
        let b = tri[(e + 1) % 3];
        let side =
            |p: [f32; 2]| sign * ((b[0] - a[0]) * (p[1] - a[1]) - (b[1] - a[1]) * (p[0] - a[0]));
        let input = std::mem::take(&mut out);
        for i in 0..input.len() {
            let (p, q) = (input[i], input[(i + 1) % input.len()]);
            let (sp, sq) = (side(p), side(q));
            if sp >= 0.0 {
                out.push(p);
            }
            if (sp >= 0.0) != (sq >= 0.0) {
                let t = sp / (sp - sq);
                out.push([p[0] + (q[0] - p[0]) * t, p[1] + (q[1] - p[1]) * t]);
            }
        }
        if out.is_empty() {
            break;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Live (`tools/oracle/frida/ra2_fx.js`, `0x4a2a50` argument 1 over hit damage,
    /// `GraphicDetail = 2`): the spark count's rule.
    #[test]
    fn the_hit_count_follows_the_live_log() {
        for (damage, count) in [
            (12.33672, 3.44138),
            (16.70277, 4.26608),
            (38.39037, 8.36262),
            (71.05929, 14.53342),
            (115.14941, 20.0),
            (133.62216, 20.0),
        ] {
            assert!(
                (hit_count(damage, 2) - count).abs() < 1e-3,
                "{damage}: {} vs {count}",
                hit_count(damage, 2)
            );
        }
        // Below `GraphicDetail 2` the ceiling is 10.
        assert!((hit_count(200.0, 1) - 10.0).abs() < 1e-6);
        assert!((hit_count(55.0, 0) - 6.5).abs() < 1e-4);
        // The debris of a chassis hit is a quarter of it: 0..=5.
        assert_eq!((hit_count(12.3, 2) * 0.25) as i32, 0);
        assert_eq!((hit_count(200.0, 2) * 0.25) as i32, 5);
    }

    fn run(cfg: Config, seconds: f32) -> (Emitter, usize) {
        let mut e = Emitter::new(cfg, Vec3::ZERO, true, EMIT_SECONDS);
        let mut rng = Rng(7);
        let collider = Collider::default();
        let mut spawned_peak = 0;
        let steps = (seconds / (1.0 / 60.0)) as usize;
        for _ in 0..steps {
            e.step(1.0 / 60.0, &mut rng, 1, &collider);
            spawned_peak = spawned_peak.max(e.alive());
        }
        (e, spawned_peak)
    }

    #[test]
    fn the_rng_is_the_engines_lcg() {
        let mut r = Rng(0);
        let first = r.unit();
        let expect = (0x3039u32 & 0x7fff_ffff) as f64 * 4.656_612_873_077_393e-10;
        assert!((first as f64 - expect).abs() < 1e-9);
        for _ in 0..1000 {
            let v = r.unit();
            assert!((0.0..1.0).contains(&v));
        }
    }

    #[test]
    fn a_spark_burst_emits_about_count_particles_in_a_tenth_of_a_second() {
        let (e, peak) = run(Config::spark(20, Vec3::Y * 2.0, Vec3::ONE), 0.2);
        assert!((18..=21).contains(&peak), "peak {peak}");
        assert!(e.alive() <= peak);
    }

    #[test]
    fn sparks_live_about_0_7_seconds_and_fade_to_nothing() {
        // After 0.2 s they are all there at full alpha; after 1.3 s all gone.
        let (e, _) = run(Config::spark(10, Vec3::Y, Vec3::ONE), 0.2);
        assert!(e.particles.iter().all(|p| p.color[3] > 0.99));
        let (e, _) = run(Config::spark(10, Vec3::Y, Vec3::ONE), 1.3);
        assert_eq!(e.alive(), 0);
        assert!(e.finished());
    }

    #[test]
    fn colour_holds_until_the_fade_fraction_then_blends() {
        let mut cfg = Config::base();
        cfg.rate = 0.0;
        cfg.life = 1.0;
        cfg.life_spread = 0.0;
        cfg.color_start = [1.0, 0.0, 0.0, 1.0];
        cfg.color_end = [0.0, 0.0, 1.0, 0.0];
        cfg.color_spread = [0.0; 4];
        cfg.size_start = 1.0;
        cfg.size_end = 3.0;
        let mut e = Emitter::new(cfg, Vec3::ZERO, false, 0.0);
        let mut rng = Rng(3);
        e.spawn(1, &mut rng, 1);
        let collider = Collider::default();
        for _ in 0..30 {
            e.step(1.0 / 60.0, &mut rng, 1, &collider);
        }
        // p = 0.5: still the start colour and size.
        assert_eq!(e.particles[0].color, [1.0, 0.0, 0.0, 1.0]);
        assert_eq!(e.particles[0].size, 1.0);
        for _ in 0..15 {
            e.step(1.0 / 60.0, &mut rng, 1, &collider);
        }
        // p = 0.75: halfway through the fade.
        let p = &e.particles[0];
        assert!((p.color[0] - 0.5).abs() < 0.02, "{:?}", p.color);
        assert!((p.color[3] - 0.5).abs() < 0.02);
        assert!((p.size - 2.0).abs() < 0.05);
    }

    #[test]
    fn gravity_integrates_as_half_g_t_squared() {
        let mut cfg = Config::base();
        cfg.rate = 0.0;
        cfg.velocity = Vec3::ZERO;
        cfg.velocity_spread = Vec3::ZERO;
        cfg.life = 10.0;
        cfg.life_spread = 0.0;
        let mut e = Emitter::new(cfg, Vec3::ZERO, false, 0.0);
        let mut rng = Rng(3);
        e.spawn(1, &mut rng, 1);
        let collider = Collider::default();
        for _ in 0..60 {
            e.step(1.0 / 60.0, &mut rng, 1, &collider);
        }
        // y = -0.5 g t^2 (+ the discrete-step excess g dt t / 2).
        let y = e.particles[0].pos.y;
        assert!((y + 4.9).abs() < 0.1, "{y}");
        assert!((e.particles[0].vel.y + 9.8).abs() < 1e-3);
    }

    #[test]
    fn sparks_bounce_off_a_declared_floor() {
        let floor = [
            Vec3::new(-5.0, 0.0, -5.0),
            Vec3::new(5.0, 0.0, -5.0),
            Vec3::new(0.0, 0.0, 5.0),
        ];
        let collider = Collider {
            triangles: vec![floor],
        };
        let (hit, n) = collider
            .hit(Vec3::new(0.0, 0.1, 0.0), Vec3::new(0.0, -0.1, 0.0))
            .expect("crosses the floor");
        assert!(hit.y.abs() < 1e-5);
        assert!((n - Vec3::Y).length() < 1e-5);
        assert!(collider
            .hit(Vec3::new(9.0, 0.1, 0.0), Vec3::new(9.0, -0.1, 0.0))
            .is_none());
    }

    #[test]
    fn the_quad_follows_the_originals_corners_and_uvs() {
        let p = Particle {
            pos: Vec3::new(1.0, 2.0, 3.0),
            vel: Vec3::ZERO,
            total: 1.0,
            remaining: 1.0,
            color: [1.0, 0.5, 0.25, 0.75],
            color_start: [1.0; 4],
            size: 0.5,
            size_start: 0.5,
            angle: 0.0,
            texture: 0,
            anim: 0.0,
            alive: true,
        };
        let mut v = Vec::new();
        quad(&mut v, &p, Vec3::X, Vec3::Y);
        assert_eq!(v.len(), 6);
        let at = |i: usize| (Vec3::from(v[i].pos), v[i].uv);
        assert_eq!(at(0), (Vec3::new(1.5, 2.5, 3.0), [1.0, 0.0]));
        assert_eq!(at(1), (Vec3::new(1.5, 1.5, 3.0), [1.0, 1.0]));
        assert_eq!(at(2), (Vec3::new(0.5, 1.5, 3.0), [0.0, 1.0]));
        assert_eq!(at(5), (Vec3::new(0.5, 2.5, 3.0), [0.0, 0.0]));
        assert_eq!(v[0].color, [1.0, 0.5, 0.25, 0.75]);
    }

    #[test]
    fn the_camera_axes_come_out_of_the_view_projection() {
        let eye = Vec3::new(3.0, 4.0, -7.0);
        let view = crate::look_at_lh(eye, Vec3::new(0.0, 0.0, 0.0), Vec3::Y);
        let proj = crate::perspective_lh(1.0, 1.33, 0.1, 100.0);
        let (right, up) = camera_axes(proj * view);
        let fwd = (Vec3::ZERO - eye).normalize();
        assert!(right.dot(fwd).abs() < 1e-4 && up.dot(fwd).abs() < 1e-4);
        assert!(right.dot(up).abs() < 1e-4);
        // Left-handed: right is up x forward.
        assert!((up.cross(fwd) - right).length() < 1e-3, "{right:?} {up:?}");
        assert!(up.y > 0.0);
    }

    #[test]
    fn a_box_above_the_floor_casts_its_projection_from_the_light() {
        let floor = [
            Vec3::new(-10.0, 0.0, -10.0),
            Vec3::new(10.0, 0.0, -10.0),
            Vec3::new(0.0, 0.0, 10.0),
        ];
        let light = Vec3::new(0.0, 10.0, 0.0);
        // A unit cube centred 1 m up.
        let mut cube = Vec::new();
        for x in [-0.5, 0.5] {
            for y in [0.5, 1.5] {
                for z in [-0.5, 0.5] {
                    cube.push(Vec3::new(x, y, z));
                }
            }
        }
        let item = shadow_item(light, &[floor], &[cube]).expect("a shadow");
        assert!(!item.vertices.is_empty());
        assert!(item.material.blended());
        for v in &item.vertices {
            assert!((v.pos[1] - SHADOW_LIFT).abs() < 1e-4);
            assert_eq!(v.color, SHADOW_RGBA);
        }
        // The top face (y=1.5) projects x1 = 0.5 * 10 / 8.5, the bottom face
        // (y=0.5) 0.5 * 10 / 9.5; the largest reach is the first.
        let reach = item
            .vertices
            .iter()
            .map(|v| v.pos[0].abs())
            .fold(0.0, f32::max);
        assert!((reach - 0.5 * 10.0 / 8.5).abs() < 1e-3, "{reach}");
        // Nothing falls on a receiver the caster is behind.
        let behind = [
            Vec3::new(-10.0, 20.0, -10.0),
            Vec3::new(10.0, 20.0, -10.0),
            Vec3::new(0.0, 20.0, 10.0),
        ];
        assert!(shadow_item(light, &[behind], &[vec![Vec3::new(0.0, 1.0, 0.0); 4]]).is_none());
    }

    #[test]
    fn a_shadow_is_clipped_to_its_receiver() {
        let small = [
            Vec3::new(0.0, 0.0, 0.0),
            Vec3::new(0.2, 0.0, 0.0),
            Vec3::new(0.0, 0.0, 0.2),
        ];
        let light = Vec3::new(0.0, 10.0, 0.0);
        let big: Vec<Vec3> = [-2.0, 2.0]
            .iter()
            .flat_map(|&x| [-2.0, 2.0].iter().map(move |&z| Vec3::new(x, 3.0, z)))
            .collect();
        let item = shadow_item(light, &[small], &[big]).expect("covered");
        for v in &item.vertices {
            assert!(v.pos[0] >= -1e-3 && v.pos[0] <= 0.2 + 1e-3);
            assert!(v.pos[2] >= -1e-3 && v.pos[2] <= 0.2 + 1e-3);
        }
    }

    #[test]
    fn a_bolt_redraws_its_jitter_when_its_timer_runs_out() {
        let mut fx = Effects::new(None);
        fx.set_lightning([(0, Vec3::ZERO, Vec3::new(4.0, 0.0, 0.0), true)]);
        assert!(!fx.is_empty());
        fx.step(1.0 / 60.0);
        let first = fx.bolts[0].1.offsets;
        assert!(first.iter().any(|o| *o != 0.0));
        assert!(first.iter().all(|o| o.abs() <= BOLT_AMPLITUDE / 2.0 + 1e-6));
        for _ in 0..3 {
            fx.step(1.0 / 60.0);
        }
        assert_eq!(
            fx.bolts[0].1.offsets, first,
            "still inside the refresh time"
        );
        for _ in 0..12 {
            fx.step(1.0 / 60.0);
        }
        assert_ne!(
            fx.bolts[0].1.offsets, first,
            "0.15 s at most between redraws"
        );
        // Moving it keeps the jitter; hiding it stops it drawing.
        let kept = fx.bolts[0].1.offsets;
        fx.set_lightning([(0, Vec3::Y, Vec3::new(4.0, 1.0, 0.0), true)]);
        assert_eq!(fx.bolts[0].1.offsets, kept);
        fx.set_lightning([(0, Vec3::Y, Vec3::new(4.0, 1.0, 0.0), false)]);
        assert!(fx.is_empty());
        assert!(fx.bolt_items(Vec3::X, Vec3::Y).is_none());
    }

    #[test]
    fn a_bolt_is_a_ribbon_of_five_segments_from_start_to_end() {
        let mut fx = Effects::new(None);
        fx.set_lightning([(0, Vec3::new(0.0, 0.0, 5.0), Vec3::new(4.0, 0.0, 5.0), true)]);
        fx.step(1.0 / 60.0);
        let item = fx.bolt_items(Vec3::X, Vec3::Y).expect("a bolt");
        // Two layers, five segments, six vertices a segment.
        assert_eq!(item.vertices.len(), 2 * BOLT_SEGMENTS * 6);
        assert!(item.material.additive && item.material.blended());
        let blue = item
            .vertices
            .iter()
            .filter(|v| v.color == [0.0, 0.0, 1.0, 1.0])
            .count();
        assert_eq!(blue, BOLT_SEGMENTS * 6);
        let xs: Vec<f32> = item.vertices.iter().map(|v| v.pos[0]).collect();
        assert!(xs.iter().cloned().fold(f32::MAX, f32::min).abs() < 0.1);
        assert!((xs.iter().cloned().fold(f32::MIN, f32::max) - 4.0).abs() < 0.1);
        // The jitter and the width are sideways to the view (y), never along z.
        assert!(item.vertices.iter().all(|v| (v.pos[2] - 5.0).abs() < 1e-4));
    }

    #[test]
    fn the_manager_holds_twenty_one_shot_effects() {
        let mut fx = Effects::new(None);
        for _ in 0..30 {
            fx.sparks(15.0, Vec3::ZERO, Vec3::Y);
        }
        assert_eq!(fx.bursts.len(), MAX_EFFECTS);
        fx.enabled = false;
        fx.bursts.clear();
        fx.sparks(15.0, Vec3::ZERO, Vec3::Y);
        assert!(fx.is_empty(), "GraphicEffects off draws nothing");
    }

    #[test]
    fn a_standing_emitter_follows_its_switch() {
        let mut fx = Effects::new(None);
        fx.motor_smoke(1, Vec3::ZERO, false);
        assert!(fx.is_empty(), "off and never built");
        fx.motor_smoke(1, Vec3::ZERO, true);
        for _ in 0..60 {
            fx.step(1.0 / 60.0);
        }
        let alive = fx.standing[0].1.alive();
        assert!((3..=8).contains(&alive), "5/s for a second: {alive}");
        fx.motor_smoke(1, Vec3::new(0.0, 1.0, 0.0), false);
        for _ in 0..120 {
            fx.step(1.0 / 60.0);
        }
        assert_eq!(fx.standing[0].1.alive(), 0, "stopped, then burnt out");
    }
}
