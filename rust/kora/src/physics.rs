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

const MAX_ENGINE_FORCE: f32 = 780.0;
const MAX_BRAKE: f32 = 14.0;
const MAX_STEER: f32 = 0.5;
const CHASSIS_MASS: f32 = 1100.0;

/// rapier's default tuning assumes a real-world car with ~0.5 m of suspension
/// travel.  This car is only ~0.5 units tall, so the spring has to hold the
/// chassis up over a much shorter travel: equilibrium compression is
/// `g / (4 * stiffness)` regardless of mass, so stiffness ~24 keeps the body
/// off the road instead of dragging along it.
fn tuning() -> WheelTuning {
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

    pub fn add_car(&mut self, spawn: Vec3, yaw: f32, half: Vec3) -> usize {
        let (body, _) = self.physics.insert(
            RigidBodyBuilder::dynamic()
                .translation(Vector::new(spawn.x, spawn.y, spawn.z))
                .rotation(Vector::new(0.0, yaw, 0.0))
                .linear_damping(0.15)
                .angular_damping(2.5)
                .ccd_enabled(true)
                .can_sleep(false),
            ColliderBuilder::cuboid(half.x, half.y, half.z)
                .mass(CHASSIS_MASS)
                .friction(1.4),
        );

        let mut controller = DynamicRayCastVehicleController::new(body);
        let tuning = tuning();
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
                &tuning,
            );
        }

        self.cars.push(Car {
            body,
            controller,
            spawn: Vector::new(spawn.x, spawn.y, spawn.z),
            spawn_yaw: yaw,
        });
        self.cars.len() - 1
    }

    pub fn step(&mut self, dt: f32, controls: &[CarControl]) {
        let dt = dt.clamp(1.0 / 240.0, 1.0 / 30.0);
        self.physics.integration_parameters.dt = dt;

        let World { physics, cars } = self;
        for (car, control) in cars.iter_mut().zip(controls.iter()) {
            for (index, wheel) in car.controller.wheels_mut().iter_mut().enumerate() {
                wheel.steering = if index < 2 { control.steer * MAX_STEER } else { 0.0 };
                wheel.engine_force = control.throttle * MAX_ENGINE_FORCE;
                wheel.brake = if control.brake { MAX_BRAKE } else { 0.0 };
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

    /// Lift a car back onto the road surface if it has sunk below it.
    ///
    /// The MIDlet drives its car kinematically: every frame it *sets* the
    /// height from the track's collision mesh, so a step between two tiles is
    /// something it climbs rather than a wall.  A physics chassis has no such
    /// luxury, so once a step has been hit the car would be trapped against
    /// it.  Raising the body to the surface (and dropping any downward
    /// velocity) reproduces the original behaviour without giving up contact
    /// for the wheels.  A car in the air is left alone, so jumps still work.
    pub fn lift_to(&mut self, car: usize, height: f32) {
        let body = &mut self.physics.bodies[self.cars[car].body];
        let translation = body.translation();
        if translation.y >= height {
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
