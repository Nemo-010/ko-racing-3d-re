//! rapier3d vehicle: a dynamic chassis driven by a raycast vehicle controller
//! over a trimesh built from the track tiles.

use macroquad::prelude::{vec3, Quat, Vec3};
use rapier3d::control::{DynamicRayCastVehicleController, WheelTuning};
use rapier3d::prelude::*;

/// Player controls, normalised to `-1..1`.
pub struct Input {
    pub throttle: f32,
    pub steer: f32,
    pub brake: bool,
}

impl Default for Input {
    fn default() -> Self {
        Input {
            throttle: 0.0,
            steer: 0.0,
            brake: false,
        }
    }
}

const MAX_ENGINE_FORCE: f32 = 780.0;
const MAX_BRAKE: f32 = 14.0;
const MAX_STEER: f32 = 0.5;

pub struct Vehicle {
    pub world: PhysicsWorld,
    pub controller: DynamicRayCastVehicleController,
    pub body: RigidBodyHandle,
    pub spawn: Vector,
    pub spawn_yaw: f32,
}

impl Vehicle {
    pub fn new(
        track_vertices: Vec<Vector>,
        track_indices: Vec<[u32; 3]>,
        spawn: Vec3,
        yaw: f32,
        half: Vec3,
    ) -> Vehicle {
        let mut world = PhysicsWorld::new();
        world.gravity = Vector::new(0.0, -9.81, 0.0);

        let ground = world.bodies.insert(RigidBodyBuilder::fixed());
        match ColliderBuilder::trimesh(track_vertices, track_indices) {
            Ok(collider) => {
                world
                    .colliders
                    .insert_with_parent(collider.friction(1.0), ground, &mut world.bodies);
            }
            Err(error) => eprintln!("track trimesh rejected: {error:?}"),
        }

        let (body, _collider) = world.insert(
            RigidBodyBuilder::dynamic()
                .translation(Vector::new(spawn.x, spawn.y, spawn.z))
                .rotation(Vector::new(0.0, yaw, 0.0))
                .linear_damping(0.15)
                .angular_damping(2.5)
                .ccd_enabled(true)
                .can_sleep(false),
            ColliderBuilder::cuboid(half.x, half.y, half.z)
                .mass(1100.0)
                .friction(1.4),
        );

        let mut controller = DynamicRayCastVehicleController::new(body);
        // rapier's default tuning assumes a real-world car (0.5 m suspension
        // travel).  This car is only ~0.5 units tall, so the stiff spring has
        // to hold the chassis up on a much shorter travel: at equilibrium the
        // compression is g / (4 * stiffness), independent of mass, so raising
        // stiffness to ~24 keeps the body off the road instead of dragging
        // along it.
        let tuning = WheelTuning {
            suspension_stiffness: 24.0,
            suspension_compression: 1.6,
            suspension_damping: 1.9,
            max_suspension_travel: 0.25,
            side_friction_stiffness: 1.0,
            friction_slip: 10.5,
            max_suspension_force: 12_000.0,
        };
        let down = Vector::new(0.0, -1.0, 0.0);
        let axle = Vector::new(1.0, 0.0, 0.0);
        // The wheels hang from the top of the wheel well: the connection point
        // must start *above* the road so the suspension ray can find it.  A
        // connection at the chassis floor (y = -half.y) begins under the
        // trimesh and the controller then never sees any ground.
        let connection_y = half.y * 0.55;
        let suspension = 0.34;
        let radius = (half.y * 0.9).max(0.15);
        // The car's nose points along -Z, so the two front wheels are at -Z.
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

        Vehicle {
            world,
            controller,
            body,
            spawn: Vector::new(spawn.x, spawn.y, spawn.z),
            spawn_yaw: yaw,
        }
    }

    pub fn step(&mut self, dt: f32, input: &Input) {
        for (index, wheel) in self.controller.wheels_mut().iter_mut().enumerate() {
            wheel.steering = if index < 2 {
                input.steer * MAX_STEER
            } else {
                0.0
            };
            wheel.engine_force = input.throttle * MAX_ENGINE_FORCE;
            wheel.brake = if input.brake { MAX_BRAKE } else { 0.0 };
        }

        self.world.integration_parameters.dt = dt.clamp(1.0 / 240.0, 1.0 / 30.0);
        let filter = QueryFilter::default().exclude_rigid_body(self.body);
        let queries = self.world.broad_phase.as_query_pipeline_mut(
            self.world.narrow_phase.query_dispatcher(),
            &mut self.world.bodies,
            &mut self.world.colliders,
            filter,
        );
        self.controller.update_vehicle(self.world.integration_parameters.dt, queries);
        self.world.step();
    }

    /// Translation and rotation in macroquad space.
    pub fn pose(&self) -> (Vec3, Quat) {
        let pose = self.world.bodies[self.body].position();
        let t = pose.translation;
        let r = pose.rotation;
        (
            vec3(t.x, t.y, t.z),
            Quat::from_xyzw(r.x, r.y, r.z, r.w),
        )
    }

    pub fn speed(&self) -> f32 {
        self.world.bodies[self.body].linvel().length()
    }

    pub fn reset(&mut self) {
        let body = &mut self.world.bodies[self.body];
        body.set_translation(self.spawn, true);
        body.set_rotation(Rotation::from_rotation_y(self.spawn_yaw), true);
        body.set_linvel(Vector::ZERO, true);
        body.set_angvel(Vector::ZERO, true);
    }
}
