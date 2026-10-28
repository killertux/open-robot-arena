//! Differential test of [`super::query`] against Jolt: the geometry queries the
//! backend used to ask a Jolt world (deep-overlap contacts, phantom touches,
//! ray casts) answered natively, on real robots and arenas at random poses.
//!
//! Only built with the `jolt` feature (it needs Jolt as the reference). The
//! shapes are the ones a duel really creates (recorded from `Duel::new`); the
//! poses are the robots' rigid groups scattered at random over the arena, so
//! they overlap the floor, the walls and each other.

use std::cell::RefCell;
use std::collections::HashMap;

use glam::{Quat, Vec3};
use oxijolt::prelude::math as m;
use oxijolt::prelude::{
    Activation, BodyId, BodySettings, PhysicsWorld, QueryFilter, RayCast, Shape, WorldSettings,
};
use oxijolt::CollideShape;

use super::Havok17Backend;
use crate::backend::{BodyDesc, MotionType, PhysicsBackend};
use crate::jolt::JoltBackend;

thread_local! {
    /// Every body a `Havok17Backend` on this thread was asked to create.
    pub(super) static RECORDED: RefCell<Vec<BodyDesc>> = const { RefCell::new(Vec::new()) };
}

fn rv(v: Vec3) -> m::RVec3 {
    m::RVec3::new(v.x as m::Real, v.y as m::Real, v.z as m::Real)
}
fn jv(v: Vec3) -> m::Vec3 {
    m::Vec3::new(v.x, v.y, v.z)
}
#[allow(clippy::unnecessary_cast)]
fn un_rv(v: m::RVec3) -> Vec3 {
    Vec3::new(v.x as f32, v.y as f32, v.z as f32)
}
fn un_v(v: m::Vec3) -> Vec3 {
    Vec3::new(v.x, v.y, v.z)
}
fn jq(q: Quat) -> m::Quat {
    m::Quat::from_xyzw(q.x, q.y, q.z, q.w)
}

/// The Jolt query world the backend used to keep.
struct Mirror {
    world: PhysicsWorld,
    ids: Vec<BodyId>,
    shapes: Vec<Shape>,
    phantoms: Vec<Option<Shape>>,
    lookup: HashMap<u32, usize>,
}

impl Mirror {
    fn new(descs: &[BodyDesc]) -> Mirror {
        let mut world =
            PhysicsWorld::new(WorldSettings::default().gravity(jv(Vec3::new(0.0, -12.0, 0.0))))
                .unwrap();
        let (mut ids, mut shapes, mut phantoms, mut lookup) =
            (Vec::new(), Vec::new(), Vec::new(), HashMap::new());
        for (k, d) in descs.iter().enumerate() {
            let shape = JoltBackend::build_shape(&d.shape).expect("shape");
            let phantom = d
                .phantom
                .as_ref()
                .map(|p| JoltBackend::build_shape(p).expect("phantom"));
            let settings = match d.motion {
                MotionType::Static => BodySettings::new_static(),
                _ => BodySettings::new_kinematic(),
            }
            .position(rv(d.position))
            .rotation(jq(d.rotation))
            .allow_sleeping(false);
            let id = world.create_body(&shape, &settings).unwrap();
            lookup.insert(id.to_raw(), k);
            ids.push(id);
            shapes.push(shape);
            phantoms.push(phantom);
        }
        Mirror {
            world,
            ids,
            shapes,
            phantoms,
            lookup,
        }
    }

    fn place(&mut self, i: usize, origin: Vec3, rotation: Quat) {
        let mut body = self.world.body_mut(self.ids[i]).unwrap();
        body.set_position_and_rotation(rv(origin), jq(rotation), Activation::DontActivate)
            .unwrap();
    }

    /// The old `overlap_contacts`: (depth, normal, point) per hit.
    fn overlap(&self, be: &Havok17Backend, i: usize, j: usize) -> Vec<(f32, Vec3, Vec3)> {
        let b = &be.bodies[i];
        let query = CollideShape::new(&self.shapes[i], rv(b.origin()), jq(b.rotation));
        let Ok(hits) = self
            .world
            .collide_shape(&query, &QueryFilter::new().exclude_body(self.ids[i]))
        else {
            return Vec::new();
        };
        hits.into_iter()
            .filter(|h| h.body == self.ids[j] && h.penetration_depth > 0.0)
            .filter(|h| un_v(h.normal).length_squared() > 0.5)
            .map(|h| {
                (
                    h.penetration_depth,
                    un_v(h.normal).normalize(),
                    (un_rv(h.point_on_shape) + un_rv(h.point_on_body)) * 0.5,
                )
            })
            .collect()
    }

    /// Every hit of body `i`'s phantom the old filter keeps: (body, depth, normal).
    fn phantom_hits(&self, be: &Havok17Backend, i: usize) -> Vec<(usize, f32, Vec3)> {
        let b = &be.bodies[i];
        let Some(phantom) = self.phantoms[i].as_ref() else {
            return Vec::new();
        };
        let query = CollideShape::new(phantom, rv(b.origin()), jq(b.rotation));
        let hits = self
            .world
            .collide_shape(&query, &QueryFilter::new().exclude_body(self.ids[i]))
            .unwrap_or_default();
        hits.into_iter()
            .filter_map(|h| {
                let j = *self.lookup.get(&h.body.to_raw())?;
                (j != i && be.bodies[j].alive && !be.bodies[j].sensor && !be.same_group(i, j))
                    .then(|| (j, h.penetration_depth, un_v(h.normal)))
            })
            .collect()
    }

    /// The old `detect_phantoms` for body `i`.
    fn phantom(&self, be: &Havok17Backend, i: usize) -> Option<Vec3> {
        let b = &be.bodies[i];
        if !b.alive {
            return None;
        }
        let phantom = self.phantoms[i].as_ref()?;
        let query = CollideShape::new(phantom, rv(b.origin()), jq(b.rotation));
        let hits = self
            .world
            .collide_shape(&query, &QueryFilter::new().exclude_body(self.ids[i]))
            .ok()?;
        let mut best: Option<(f32, Vec3)> = None;
        for hit in hits {
            let Some(&j) = self.lookup.get(&hit.body.to_raw()) else {
                continue;
            };
            if j == i || !be.bodies[j].alive || be.bodies[j].sensor || be.same_group(i, j) {
                continue;
            }
            let n = un_v(hit.normal);
            if n.length_squared() < 0.5 || hit.penetration_depth < 0.0 {
                continue;
            }
            if best.is_none_or(|(d, _)| hit.penetration_depth > d) {
                best = Some((hit.penetration_depth, n.normalize()));
            }
        }
        best.map(|(_, n)| n)
    }

    /// The old `ray_cast_excluding`: (body, distance, normal).
    fn ray(
        &self,
        origin: Vec3,
        direction: Vec3,
        max: f32,
        exclude: usize,
    ) -> Option<(usize, f32, Vec3)> {
        let dir = direction.normalize_or_zero() * max;
        let ray = RayCast::new(rv(origin), jv(dir));
        let filter = match self.ids.get(exclude) {
            Some(id) => QueryFilter::new().exclude_body(*id),
            None => QueryFilter::default(),
        };
        let hit = self.world.cast_ray(&ray, &filter).ok()??;
        Some((
            self.lookup[&hit.body.to_raw()],
            hit.distance,
            un_v(hit.normal),
        ))
    }
}

/// Deterministic generator (xorshift).
struct Rng(u64);
impl Rng {
    fn next(&mut self) -> f32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        ((self.0 >> 40) as f32) / (1u64 << 24) as f32
    }
    fn range(&mut self, lo: f32, hi: f32) -> f32 {
        lo + (hi - lo) * self.next()
    }
    fn unit(&mut self) -> Vec3 {
        loop {
            let v = Vec3::new(
                self.range(-1.0, 1.0),
                self.range(-1.0, 1.0),
                self.range(-1.0, 1.0),
            );
            if v.length_squared() > 0.01 && v.length_squared() <= 1.0 {
                return v.normalize();
            }
        }
    }
    fn rotation(&mut self, full: bool) -> Quat {
        let yaw = Quat::from_rotation_y(self.range(0.0, std::f32::consts::TAU));
        if full {
            Quat::from_axis_angle(self.unit(), self.range(0.0, std::f32::consts::PI)) * yaw
        } else {
            Quat::from_axis_angle(self.unit(), self.range(0.0, 0.5)) * yaw
        }
    }
}

#[derive(Default)]
struct Stats {
    name: &'static str,
    pairs: usize,
    both: usize,
    only_native: usize,
    only_jolt: usize,
    /// Hit lists of the same length / of different lengths, for pairs with hits.
    same_count: usize,
    diff_count: usize,
    depth: Vec<f32>,
    angle: Vec<f32>,
    point: Vec<f32>,
    unmatched: usize,
    extras: usize,
    jolt_hits: usize,
    native_hits: usize,
}

impl Stats {
    fn new(name: &'static str) -> Self {
        Stats {
            name,
            ..Default::default()
        }
    }
    fn pct(v: &mut [f32], p: f32) -> f32 {
        if v.is_empty() {
            return 0.0;
        }
        v.sort_by(|a, b| a.total_cmp(b));
        v[((v.len() - 1) as f32 * p) as usize]
    }
    fn report(&mut self) {
        eprintln!(
            "{:9} pairs {:6} | hit both {:5} native-only {:4} jolt-only {:4} | counts equal {:5} differ {:4}",
            self.name, self.pairs, self.both, self.only_native, self.only_jolt, self.same_count, self.diff_count
        );
        if self.jolt_hits + self.native_hits > 0 {
            eprintln!(
                "          hits: jolt {} native {} | jolt hits unmatched {} native extras {}",
                self.jolt_hits, self.native_hits, self.unmatched, self.extras
            );
        }
        for (what, v) in [
            ("depth", &mut self.depth),
            ("angle(deg)", &mut self.angle),
            ("point", &mut self.point),
        ] {
            eprintln!(
                "          {what:10} n {:6} p50 {:.5} p90 {:.5} p99 {:.5} max {:.5}",
                v.len(),
                Self::pct(v, 0.5),
                Self::pct(v, 0.9),
                Self::pct(v, 0.99),
                Self::pct(v, 1.0)
            );
        }
    }
}

fn angle_deg(a: Vec3, b: Vec3) -> f32 {
    a.dot(b).clamp(-1.0, 1.0).acos().to_degrees()
}

/// The game folder, or `None` without a local copy.
fn game_dir() -> Option<std::path::PathBuf> {
    ra2fmt::saves::isolate_for_tests();
    if let Some(dir) = std::env::var_os("RA2_GAME") {
        return Some(dir.into());
    }
    let mut dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    loop {
        let candidate = dir.join("local/game");
        if candidate.is_dir() {
            return Some(candidate);
        }
        if !dir.pop() {
            return None;
        }
    }
}

struct Scenario {
    arena: &'static str,
    bots: [&'static str; 2],
}

const SCENARIOS: [Scenario; 4] = [
    Scenario {
        arena: "box.py",
        bots: ["Team0/Bot0", "Team0/Bot1"],
    },
    Scenario {
        arena: "octagon.py",
        bots: ["Team10/Bot1", "Team8/Bot0"],
    },
    Scenario {
        arena: "compressor.py",
        bots: ["Team0/Bot2", "Team8/Bot2"],
    },
    Scenario {
        arena: "hilltop.py",
        bots: ["Team1/Bot2", "Team5/Bot1"],
    },
];

fn run(sc: &Scenario, game: &std::path::Path, seed: u64, trials: usize, all: &mut [Stats; 3]) {
    let arena = game.join("Arenas").join(sc.arena);
    let setups: Vec<crate::BotSetup> = sc
        .bots
        .iter()
        .map(|b| crate::BotSetup {
            path: game
                .join("AI")
                .join(format!("{b}.bot"))
                .to_string_lossy()
                .to_string(),
            yaw_deg: None,
            spawn: None,
        })
        .collect();
    RECORDED.with(|r| r.borrow_mut().clear());
    {
        let _scripts = ra2script::lock();
        let _duel = crate::Duel::new(&arena.to_string_lossy(), &setups).expect("duel");
    }
    let descs: Vec<BodyDesc> = RECORDED.with(|r| r.borrow().clone());
    // Only the Havok backend of this thread records: the duel's own world is
    // the first batch, ours the replay below.
    assert!(!descs.is_empty());
    let mut be = Havok17Backend::new().unwrap();
    RECORDED.with(|r| r.borrow_mut().clear());
    for d in &descs {
        be.create_body(d).unwrap();
    }
    let mut mirror = Mirror::new(&descs);
    let n = descs.len();

    // Robot groups: bodies sharing a collision group move rigidly together.
    let mut groups: HashMap<u32, Vec<usize>> = HashMap::new();
    for (k, d) in descs.iter().enumerate() {
        if let (MotionType::Dynamic, Some(g)) = (d.motion, d.collision_group) {
            groups.entry(g.group).or_default().push(k);
        }
    }
    let mut rng = Rng(seed | 1);
    for _trial in 0..trials {
        let mut keys: Vec<u32> = groups.keys().copied().collect();
        keys.sort();
        for members in keys.iter().map(|k| &groups[k]) {
            let pivot = descs[members[0]].position;
            let full = rng.next() < 0.3;
            let rot = rng.rotation(full);
            let to = Vec3::new(
                rng.range(-9.0, 9.0),
                pivot.y + rng.range(-0.5, 0.9),
                rng.range(-9.0, 9.0),
            );
            for &k in members {
                let p = to + rot * (descs[k].position - pivot);
                let q = rot * descs[k].rotation;
                be.set_transform(k as u32, p, q);
                mirror.place(k, p, q);
            }
        }
        be.detect_phantoms();
        // Overlaps.
        for i in 0..n {
            if descs[i].motion != MotionType::Dynamic {
                continue;
            }
            for j in 0..n {
                if i == j || be.same_group(i, j) || be.bodies[j].sensor || be.bodies[i].sensor {
                    continue;
                }
                // Hits under a millimetre are noise on both sides (touching
                // triangles; each engine's tolerance decides which it lists).
                let native: Vec<_> = be
                    .overlap_contacts(i, j)
                    .into_iter()
                    .map(|c| (-c.distance, c.normal, c.point))
                    .filter(|h| h.0 >= 1e-3)
                    .collect();
                let jolt: Vec<_> = mirror
                    .overlap(&be, i, j)
                    .into_iter()
                    .filter(|h| h.0 >= 1e-3)
                    .collect();
                let st = &mut all[0];
                st.pairs += 1;
                match (native.is_empty(), jolt.is_empty()) {
                    (true, true) => continue,
                    (false, true) => {
                        st.only_native += 1;
                        continue;
                    }
                    (true, false) => {
                        st.only_jolt += 1;
                        continue;
                    }
                    _ => st.both += 1,
                }
                if native.len() == jolt.len() {
                    st.same_count += 1;
                } else {
                    st.diff_count += 1;
                }
                if std::env::var_os("RA2_DIFF_VERBOSE").is_some() && {
                    let worst = jolt
                        .iter()
                        .map(|jh| {
                            let nh = native
                                .iter()
                                .min_by(|a, b| a.2.distance(jh.2).total_cmp(&b.2.distance(jh.2)))
                                .unwrap();
                            angle_deg(nh.1, jh.1)
                        })
                        .fold(0.0f32, f32::max);
                    native.len() != jolt.len() || worst > 10.0
                } {
                    let mesh = be.geometry[j]
                        .as_ref()
                        .unwrap()
                        .pieces
                        .iter()
                        .any(|p| matches!(p.shape, super::narrow::PieceShape::Mesh(_)));
                    let qmesh = be.geometry[i]
                        .as_ref()
                        .unwrap()
                        .pieces
                        .iter()
                        .any(|p| matches!(p.shape, super::narrow::PieceShape::Mesh(_)));
                    eprintln!("OVERLAP {i}->{j} (obstacle has mesh piece: {mesh}, query has: {qmesh}) native {} jolt {}", native.len(), jolt.len());
                    for h in &native {
                        eprintln!("   native d {:.4} n {:?} p {:?}", h.0, h.1, h.2);
                    }
                    for h in &jolt {
                        eprintln!("   jolt   d {:.4} n {:?} p {:?}", h.0, h.1, h.2);
                    }
                }
                // Match each Jolt hit to the native hit closest in depth and
                // direction; a Jolt hit with none within 1 mm / 1 degree is
                // unmatched, a native hit nothing matched is an extra.
                let score = |a: &(f32, Vec3, Vec3), b: &(f32, Vec3, Vec3)| {
                    (a.0 - b.0).abs() + angle_deg(a.1, b.1) * 0.01
                };
                let mut used = vec![false; native.len()];
                for jh in &jolt {
                    let (k, nh) = native
                        .iter()
                        .enumerate()
                        .min_by(|a, b| score(a.1, jh).total_cmp(&score(b.1, jh)))
                        .unwrap();
                    if (nh.0 - jh.0).abs() < 1e-3 && angle_deg(nh.1, jh.1) < 1.0 {
                        used[k] = true;
                        st.depth.push((nh.0 - jh.0).abs());
                        st.angle.push(angle_deg(nh.1, jh.1));
                        st.point.push(nh.2.distance(jh.2));
                    } else {
                        st.unmatched += 1;
                        if std::env::var_os("RA2_DIFF_VERBOSE").is_some() {
                            eprintln!("UNMATCHED {i}->{j} jolt d {:.4} n {:?} p {:?} | nearest native d {:.4} n {:?} p {:?} ({} native)", jh.0, jh.1, jh.2, nh.0, nh.1, nh.2, native.len());
                        }
                    }
                }
                st.extras += used.iter().filter(|u| !**u).count();
                st.jolt_hits += jolt.len();
                st.native_hits += native.len();
            }
        }
        // Phantoms.
        for i in 0..n {
            if descs[i].phantom.is_none() {
                continue;
            }
            let native = be.phantom_touch[i];
            let jolt = mirror.phantom(&be, i);
            let st = &mut all[1];
            st.pairs += 1;
            match (native, jolt) {
                (None, None) => {}
                (Some(_), None) => st.only_native += 1,
                (None, Some(_)) => st.only_jolt += 1,
                (Some(a), Some(b)) => {
                    st.both += 1;
                    st.same_count += 1;
                    st.angle.push(angle_deg(a, b));
                    if angle_deg(a, b) > 5.0 && std::env::var_os("RA2_DIFF_VERBOSE").is_some() {
                        explain_phantom(&mirror, &be, i);
                    }
                }
            }
        }
        // Rays.
        for _ in 0..150 {
            let origin = Vec3::new(
                rng.range(-10.0, 10.0),
                rng.range(-0.5, 4.0),
                rng.range(-10.0, 10.0),
            );
            let dir = if rng.next() < 0.5 {
                Vec3::new(rng.range(-0.3, 0.3), -1.0, rng.range(-0.3, 0.3))
            } else {
                rng.unit()
            };
            let exclude = if rng.next() < 0.5 {
                (rng.next() * n as f32) as usize % n
            } else {
                usize::MAX
            };
            let native = be.ray_cast_excluding(origin, dir, 30.0, exclude as u32);
            let jolt = mirror.ray(origin, dir, 30.0, exclude);
            let st = &mut all[2];
            st.pairs += 1;
            match (native, jolt) {
                (None, None) => {}
                (Some(_), None) => st.only_native += 1,
                (None, Some(_)) => st.only_jolt += 1,
                (Some(a), Some(b)) => {
                    st.both += 1;
                    if a.body as usize == b.0 {
                        st.same_count += 1;
                    } else {
                        st.diff_count += 1;
                    }
                    st.depth.push((a.distance - b.1).abs());
                    // Jolt reports a back face's own normal: compare lines.
                    st.angle
                        .push(angle_deg(a.normal, b.2).min(angle_deg(a.normal, -b.2)));
                }
            }
        }
    }
}

#[test]
fn the_native_queries_agree_with_jolt() {
    let Some(game) = game_dir() else { return };
    let mut all = [
        Stats::new("overlap"),
        Stats::new("phantom"),
        Stats::new("ray"),
    ];
    let trials: usize = std::env::var("RA2_DIFF_TRIALS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(40);
    for (k, sc) in SCENARIOS.iter().enumerate() {
        run(
            sc,
            &game,
            0x9E37_79B9_7F4A_7C15 ^ (k as u64 * 7919),
            trials,
            &mut all,
        );
    }
    for st in &mut all {
        st.report();
    }
    let [overlap, phantom, ray] = &mut all;
    // Rays: the same hit or miss, on the same body, to a tenth of a millimetre.
    assert_eq!(ray.only_native + ray.only_jolt + ray.diff_count, 0);
    assert!(Stats::pct(&mut ray.depth, 1.0) < 1e-3);
    // Phantoms: the same touching wheels and, to a degree, the same normals,
    // except where the phantom sinks into another robot's hull (Jolt's own axis
    // there is not a separating one, docs/29 section 12ac).
    assert_eq!(phantom.only_native + phantom.only_jolt, 0);
    assert!(Stats::pct(&mut phantom.angle, 0.9) < 1.0);
    // Overlaps: nothing Jolt finds is missed, and what matches matches to a
    // millimetre and a degree. Native-only pairs are touching-depth noise.
    assert_eq!(overlap.only_jolt, 0);
    assert!(overlap.only_native * 50 <= overlap.both.max(1));
    assert!(Stats::pct(&mut overlap.depth, 1.0) < 1e-3);
    assert!(Stats::pct(&mut overlap.angle, 1.0) < 1.0);
}

/// The phantom check run inside real fights: with `LIVE` set, every
/// `detect_phantoms` of a backend on this thread is compared with the Jolt
/// world the backend used to keep.
#[derive(Default)]
pub(super) struct Live {
    pub on: bool,
    mirror: Option<Mirror>,
    bodies: usize,
    calls: usize,
    touching: usize,
    status_mismatch: usize,
    angle_over_1: usize,
    max_angle: f32,
    ov_calls: usize,
    ov_native: usize,
    ov_jolt: usize,
    ov_count_differs: usize,
    ov_depth_differs: usize,
}

thread_local! {
    pub(super) static LIVE: RefCell<Live> = RefCell::new(Live::default());
}

/// Bring the live mirror in step with the backend (building it from the
/// recorded descriptions on first use); `None` when the check is off.
fn live_mirror(l: &mut Live, be: &Havok17Backend) -> Option<Mirror> {
    if !l.on {
        return None;
    }
    let descs: Vec<BodyDesc> = RECORDED.with(|r| r.borrow().clone());
    if descs.len() != be.bodies.len() {
        return None;
    }
    if l.mirror.is_none() || l.bodies != descs.len() {
        l.mirror = Some(Mirror::new(&descs));
        l.bodies = descs.len();
    }
    let mut mirror = l.mirror.take().unwrap();
    for (i, b) in be.bodies.iter().enumerate() {
        if b.moves() {
            mirror.place(i, b.origin(), b.rotation);
        } else if !b.alive {
            mirror.place(i, Vec3::new(0.0, -5000.0, 0.0), Quat::IDENTITY);
        }
    }
    Some(mirror)
}

/// Called at the end of `Havok17Backend::detect_phantoms`.
pub(super) fn live_check(be: &Havok17Backend) {
    LIVE.with(|l| {
        let mut l = l.borrow_mut();
        let Some(mirror) = live_mirror(&mut l, be) else {
            return;
        };
        let descs: Vec<bool> = (0..be.bodies.len())
            .map(|i| {
                be.geometry[i]
                    .as_ref()
                    .is_some_and(|g| !g.phantom_pieces.is_empty())
            })
            .collect();
        for i in 0..be.bodies.len() {
            if !descs[i] || !be.bodies[i].alive {
                continue;
            }
            l.calls += 1;
            match (be.phantom_touch[i], mirror.phantom(be, i)) {
                (None, None) => {}
                (Some(_), None) | (None, Some(_)) => {
                    l.status_mismatch += 1;
                    if std::env::var_os("RA2_DIFF_VERBOSE").is_some() {
                        explain_phantom(&mirror, be, i);
                    }
                }
                (Some(a), Some(b)) => {
                    l.touching += 1;
                    let ang = angle_deg(a, b);
                    l.max_angle = l.max_angle.max(ang);
                    if ang > 1.0 {
                        l.angle_over_1 += 1;
                        if std::env::var_os("RA2_DIFF_VERBOSE").is_some() {
                            explain_phantom(&mirror, be, i);
                        }
                    }
                }
            }
        }
        l.mirror = Some(mirror);
    });
}

/// Called with the contacts `overlap_contacts(i, j)` returned.
pub(super) fn live_overlap(be: &Havok17Backend, i: usize, j: usize, native: &[super::Contact]) {
    LIVE.with(|l| {
        let mut l = l.borrow_mut();
        let Some(mirror) = live_mirror(&mut l, be) else {
            return;
        };
        let jolt = mirror.overlap(be, i, j);
        l.ov_calls += 1;
        let big = |d: f32| d >= 1e-3;
        let (n, jn) = (
            native.iter().filter(|c| big(-c.distance)).count(),
            jolt.iter().filter(|h| big(h.0)).count(),
        );
        l.ov_native += n;
        l.ov_jolt += jn;
        if n != jn {
            l.ov_count_differs += 1;
        }
        let deepest = |it: &mut dyn Iterator<Item = f32>| it.fold(0.0f32, f32::max);
        let (dn, dj) = (
            deepest(&mut native.iter().map(|c| -c.distance)),
            deepest(&mut jolt.iter().map(|h| h.0)),
        );
        if (dn - dj).abs() > 1e-3 {
            l.ov_depth_differs += 1;
        }
        if std::env::var_os("RA2_DIFF_VERBOSE").is_some() && (n != jn || (dn - dj).abs() > 1e-3) {
            eprintln!(
                "LIVE overlap {i}->{j}: native {n} (deepest {dn:.4}) jolt {jn} (deepest {dj:.4})"
            );
        }
        l.mirror = Some(mirror);
    });
}

/// Two shipped AIs fight on the box arena and on the Octagon; every phantom
/// check of the fights is compared with Jolt.
#[test]
fn phantoms_in_real_fights_agree_with_jolt() {
    let Some(game) = game_dir() else { return };
    let seconds: f32 = std::env::var("RA2_DIFF_FIGHT_SECONDS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(12.0);
    for (arena, a, b) in [
        ("box.py", "Team0/Bot0", "Team0/Bot1"),
        ("octagon.py", "Team10/Bot1", "Team8/Bot0"),
        ("compressor.py", "Team0/Bot2", "Team8/Bot2"),
    ] {
        let _scripts = ra2script::lock();
        RECORDED.with(|r| r.borrow_mut().clear());
        LIVE.with(|l| *l.borrow_mut() = Live::default());
        let cfg = crate::MatchConfig::new(
            game.join("Arenas")
                .join(arena)
                .to_string_lossy()
                .to_string(),
            crate::GameType::Deathmatch,
            vec![
                crate::RobotEntry::ai(
                    0,
                    game.join("AI")
                        .join(format!("{a}.bot"))
                        .to_string_lossy()
                        .to_string(),
                ),
                crate::RobotEntry::ai(
                    1,
                    game.join("AI")
                        .join(format!("{b}.bot"))
                        .to_string_lossy()
                        .to_string(),
                ),
            ],
        )
        .with_duration(Some(seconds));
        let mut m = crate::Match::new(cfg).expect("match");
        LIVE.with(|l| l.borrow_mut().on = true);
        while m.state() != crate::MatchState::Results {
            m.step();
        }
        LIVE.with(|l| {
            let l = l.borrow();
            eprintln!(
                "{arena:14} phantom checks {:7} touching {:6} status mismatches {} | normals over 1 degree {} (max {:.2})",
                l.calls, l.touching, l.status_mismatch, l.angle_over_1, l.max_angle
            );
            eprintln!(
                "{:14} overlap calls {:6} contacts native {} jolt {} | calls with different counts {} deepest differing by over 1 mm {}",
                "", l.ov_calls, l.ov_native, l.ov_jolt, l.ov_count_differs, l.ov_depth_differs
            );
            // Jolt ignores a triangle's back face and so lists nothing for a
            // ring that is mostly under a thin floor, and answers the push-out
            // the other way when the plane cuts the ring near its middle
            // (docs/29 section 12ac): a few percent of the touching frames.
            if std::env::var_os("RA2_DIFF_NOASSERT").is_some() {
                return;
            }
            assert!(l.calls > 1000);
            assert!(l.status_mismatch * 12 <= l.touching.max(1));
            assert!(l.angle_over_1 * 20 <= l.touching.max(1));
        });
        LIVE.with(|l| l.borrow_mut().on = false);
    }
}

/// Every hit of body `i`'s phantom, ours and Jolt's, for a disagreement.
fn explain_phantom(mirror: &Mirror, be: &Havok17Backend, i: usize) {
    eprintln!(
        "PHANTOM body {i} native {:?} jolt {:?}",
        be.phantom_touch[i],
        mirror.phantom(be, i)
    );
    for (j, d, nrm) in mirror.phantom_hits(be, i) {
        eprintln!("   jolt hit body {j} depth {d:.4} normal {nrm:?}");
    }
    let g = be.geometry[i].as_ref().unwrap();
    for ph in &g.phantom_pieces {
        let qpose = Havok17Backend::piece_pose(&be.bodies[i], ph);
        for (j, other) in be.bodies.iter().enumerate() {
            if j == i || !other.alive || other.sensor || be.same_group(i, j) {
                continue;
            }
            let Some(gj) = &be.geometry[j] else { continue };
            for (pk, pj) in gj.pieces.iter().enumerate() {
                let opose = Havok17Backend::piece_pose(other, pj);
                let mut hits = Vec::new();
                super::query::collide(ph, &qpose, pj, &opose, &mut Vec::new(), &mut hits);
                for h in hits {
                    eprintln!(
                        "   native hit body {j} piece {pk} depth {:.4} normal {:?}",
                        h.depth, h.normal
                    );
                }
            }
        }
    }
}
