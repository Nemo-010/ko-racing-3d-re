//! rapier3d vehicle simulation.
//!
//! One `PhysicsWorld` holds the track trimesh and a dynamic chassis per car, so
//! the player and the opponents share a simulation and collide with each other.

use macroquad::prelude::{vec3, Quat, Vec3};
use rapier3d::control::{DynamicRayCastVehicleController, WheelTuning};
use rapier3d::prelude::*;

/// Player and AI controls, normalised to `-1..1`.
#[derive(Clone, Copy, Default)]
pub struct CarControl {
    pub throttle: f32,
    pub steer: f32,
    pub brake: bool,
}

const CHASSIS_MASS: f32 = 1100.0;

/// Per-car handling, derived from the four values a `.car` file carries.
///
/// The names come from the game itself: `aq.a(127 + i)` draws them and
/// `ui/ui.txt` gives those ids as SPEED, ACCELERATION, BRAKING and HANDLING.
/// What the MIDlet *does* with them is not recoverable - its car reads its own
/// tuning from elsewhere, and the 39 bytes that follow the stats in every
/// `.car` are never read by this build - so the mapping below follows those
/// names, and is calibrated so the first car in `ba.a`'s list (`rally.car`,
/// stats 3/5/5/1) lands back on the constants the port was tuned with.
#[derive(Clone, Copy)]
pub struct Tuning {
    pub engine_force: f32,
    pub linear_damping: f32,
    pub steer: f32,
    pub friction: f32,
    pub brake: f32,
}

impl Default for Tuning {
    fn default() -> Tuning {
        Tuning::from_stats([3, 5, 5, 1])
    }
}

impl Tuning {
    /// `stats` are speed, acceleration, braking and handling, each 1..=6.  Speed buys top end by lowering drag; acceleration
    /// buys engine force; braking buys brake force; handling buys steering
    /// angle and the grip the tyres can use.
    pub fn from_stats(stats: [u8; 4]) -> Tuning {
        let [speed, acceleration, braking, handling] =
            stats.map(|value| value.clamp(1, 9) as f32);
        Tuning {
            engine_force: 480.0 + 60.0 * acceleration,
            linear_damping: (0.21 - 0.02 * speed).max(0.05),
            steer: 0.46 + 0.04 * handling,
            friction: 1.25 + 0.15 * handling,
            brake: 6.0 + 1.6 * braking,
        }
    }
}

/// rapier's default tuning assumes a real-world car with ~0.5 m of suspension
/// travel.  This car is only ~0.5 units tall, so the spring has to hold the
/// chassis up over a much shorter travel: equilibrium compression is
/// `g / (4 * stiffness)` regardless of mass, so stiffness ~24 keeps the body
/// off the road instead of dragging along it.
fn wheel_tuning() -> WheelTuning {
    WheelTuning {
        suspension_stiffness: 24.0,
        suspension_compression: 1.6,
        suspension_damping: 1.9,
        max_suspension_travel: 0.25,
        side_friction_stiffness: 1.0,
        friction_slip: 10.5,
        max_suspension_force: 12_000.0,
    }
}

pub struct Car {
    body: RigidBodyHandle,
    controller: DynamicRayCastVehicleController,
    spawn: Vector,
    spawn_yaw: f32,
    tuning: Tuning,
}

pub struct World {
    pub physics: PhysicsWorld,
    pub cars: Vec<Car>,
}

impl World {
    pub fn new(
        track_vertices: Vec<Vector>,
        track_indices: Vec<[u32; 3]>,
        walls: &[(Vec3, Vec3)],
    ) -> World {
        let mut physics = PhysicsWorld::new();
        physics.gravity = Vector::new(0.0, -9.81, 0.0);
        physics.integration_parameters.dt = 1.0 / 60.0;

        let ground = physics.bodies.insert(RigidBodyBuilder::fixed());
        match ColliderBuilder::trimesh(track_vertices, track_indices) {
            Ok(collider) => {
                physics
                    .colliders
                    .insert_with_parent(collider.friction(1.0), ground, &mut physics.bodies);
            }
            Err(error) => eprintln!("track trimesh rejected: {error:?}"),
        }
        for &(centre, half) in walls {
            physics.colliders.insert_with_parent(
                ColliderBuilder::cuboid(half.x, half.y, half.z)
                    .translation(Vector::new(centre.x, centre.y + half.y, centre.z))
                    .friction(0.4),
                ground,
                &mut physics.bodies,
            );
        }

        World {
            physics,
            cars: Vec::new(),
        }
    }

    pub fn add_car(&mut self, spawn: Vec3, yaw: f32, half: Vec3, tuning: Tuning) -> usize {
        let (body, _) = self.physics.insert(
            RigidBodyBuilder::dynamic()
                .translation(Vector::new(spawn.x, spawn.y, spawn.z))
                .rotation(Vector::new(0.0, yaw, 0.0))
                .linear_damping(tuning.linear_damping)
                .angular_damping(2.5)
                .ccd_enabled(true)
                .can_sleep(false),
            ColliderBuilder::cuboid(half.x, half.y, half.z)
                .mass(CHASSIS_MASS)
                // Deliberately slippery: the tyres (see `wheel_tuning` and the
                // controller's friction) carry all the grip, while the body
                // itself must glance off barriers and other cars instead of
                // grinding to a halt against them.  The game drives its cars
                // kinematically and never beaches one, so a body that welds
                // itself to every wall it touches is strictly less faithful -
                // and a peloton of mutually grinding opponents never laps.
                .friction(0.2),
        );

        let mut controller = DynamicRayCastVehicleController::new(body);
        let wheels = wheel_tuning();
        let down = Vector::new(0.0, -1.0, 0.0);
        let axle = Vector::new(1.0, 0.0, 0.0);
        // The wheels hang from the top of the wheel well: the connection point
        // must start *above* the road so the suspension ray can find it.
        let connection_y = half.y * 0.55;
        let suspension = 0.34;
        let radius = (half.y * 0.9).max(0.15);
        // The car's nose points along -Z, so the front wheels are at -Z.
        for (x, z) in [
            (-half.x * 0.85, -half.z * 0.62),
            (half.x * 0.85, -half.z * 0.62),
            (-half.x * 0.85, half.z * 0.62),
            (half.x * 0.85, half.z * 0.62),
        ] {
            controller.add_wheel(
                Vector::new(x, connection_y, z),
                down,
                axle,
                suspension,
                radius,
                &wheels,
            );
        }

        self.cars.push(Car {
            body,
            controller,
            spawn: Vector::new(spawn.x, spawn.y, spawn.z),
            spawn_yaw: yaw,
            tuning,
        });
        self.cars.len() - 1
    }

    pub fn step(&mut self, dt: f32, controls: &[CarControl]) {
        let dt = dt.clamp(1.0 / 240.0, 1.0 / 30.0);
        self.physics.integration_parameters.dt = dt;

        let World { physics, cars } = self;
        for (car, control) in cars.iter_mut().zip(controls.iter()) {
            let tuning = car.tuning;
            for (index, wheel) in car.controller.wheels_mut().iter_mut().enumerate() {
                wheel.steering = if index < 2 {
                    control.steer * tuning.steer
                } else {
                    0.0
                };
                wheel.engine_force = control.throttle * tuning.engine_force;
                wheel.brake = if control.brake { tuning.brake } else { 0.0 };
            }
            let filter = QueryFilter::default().exclude_rigid_body(car.body);
            let queries = physics.broad_phase.as_query_pipeline_mut(
                physics.narrow_phase.query_dispatcher(),
                &mut physics.bodies,
                &mut physics.colliders,
                filter,
            );
            car.controller.update_vehicle(dt, queries);
        }
        physics.step();
    }

    /// Translation and rotation in macroquad space.
    pub fn pose(&self, car: usize) -> (Vec3, Quat) {
        let pose = self.physics.bodies[self.cars[car].body].position();
        let t = pose.translation;
        let r = pose.rotation;
        (vec3(t.x, t.y, t.z), Quat::from_xyzw(r.x, r.y, r.z, r.w))
    }

    pub fn speed(&self, car: usize) -> f32 {
        self.physics.bodies[self.cars[car].body].linvel().length()
    }

    pub fn reset(&mut self, car: usize) {
        let entry = &self.cars[car];
        let (spawn, yaw) = (entry.spawn, entry.spawn_yaw);
        let body = &mut self.physics.bodies[entry.body];
        body.set_translation(spawn, true);
        body.set_rotation(Rotation::from_rotation_y(yaw), true);
        body.set_linvel(Vector::ZERO, true);
        body.set_angvel(Vector::ZERO, true);
    }

    /// Right a car that has ended up on its side or roof, and reseat it on
    /// the surface.  Returns true when it intervened.
    ///
    /// The MIDlet drives its car kinematically, so turtling is not a state the
    /// game has: a car that lands upside down off a crest, or gets beached
    /// sideways on a kerb, would otherwise sit there spinning its wheels until
    /// the race ends.  Only slow cars are touched, so a car that is still
    /// tumbling through the air is left alone; `height` is the surface below
    /// it (see `support_height`), or any fallback when there is none.
    pub fn upright(&mut self, car: usize, height: f32) -> bool {
        let body = &self.physics.bodies[self.cars[car].body];
        let up = body.rotation() * Vector::Y;
        if up.y >= 0.5 || body.linvel().length() >= 2.0 {
            return false;
        }
        let forward = body.rotation() * -Vector::Z;
        let yaw = (-forward.x).atan2(-forward.z);
        let body = &mut self.physics.bodies[self.cars[car].body];
        let y = body.translation().y.max(height);
        let position = Vector::new(body.translation().x, y, body.translation().z);
        body.set_translation(position, true);
        body.set_rotation(Rotation::from_rotation_y(yaw), true);
        body.set_angvel(Vector::ZERO, true);
        true
    }

    /// Hold a car to the road surface it is roughly on when it has sunk
    /// below it.
    ///
    /// The MIDlet drives its car kinematically: every frame it *sets* the
    /// height from the track's collision mesh, so a step between two tiles is
    /// something it climbs rather than a wall.  A physics chassis has no such
    /// luxury, so without help it grinds against steps.  Raising the body to
    /// the surface (and dropping any downward velocity) reproduces the
    /// original behaviour without giving up contact for the wheels.  A car in
    /// the air is left alone, so jumps and falls off the world still work.
    ///
    /// Deliberately one-sided: snapping a body that is *above* the surface
    /// down to it suspends cars past cliff edges, where the nearest-cell
    /// lookup still reports the old cell and the nose still sees it from
    /// behind - the car then hovers there forever instead of falling onto the
    /// road below.
    ///
    /// The lift has a dead zone underneath: it only fires when the body is
    /// more than a suspension travel below where it belongs.  Pinning the body
    /// at exactly ride height every frame holds the wheels dangling just short
    /// of the surface with no load on them, and the car hangs there forever -
    /// which is what a car teleported onto a slope (a grid slot on a ramp,
    /// say) does without it.  Inside the band the suspension rules, so the car
    /// settles onto its wheels and drives.
    pub fn conform(&mut self, car: usize, height: f32) {
        // Wider than the suspension travel (0.25), so a car resting on its
        // wheels never trips the lift; narrower than any real step.
        const BELOW: f32 = 0.3;
        let body = &mut self.physics.bodies[self.cars[car].body];
        let translation = body.translation();
        if translation.y >= height - BELOW {
            return;
        }
        body.set_translation(Vector::new(translation.x, height, translation.z), true);
        let velocity = body.linvel();
        if velocity.y < 0.0 {
            body.set_linvel(Vector::new(velocity.x, 0.0, velocity.z), true);
        }
    }

    /// Teleport a car onto the given point (used to rejoin after a fall).
    pub fn replace(&mut self, car: usize, position: Vec3, yaw: f32) {
        let body = &mut self.physics.bodies[self.cars[car].body];
        body.set_translation(Vector::new(position.x, position.y + 1.0, position.z), true);
        body.set_rotation(Rotation::from_rotation_y(yaw), true);
        body.set_linvel(Vector::ZERO, true);
        body.set_angvel(Vector::ZERO, true);
    }

    pub fn position(&self, car: usize) -> Vec3 {
        self.pose(car).0
    }
}
