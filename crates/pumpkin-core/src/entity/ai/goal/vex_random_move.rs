use std::sync::Weak;

use pumpkin_util::math::vector3::Vector3;
use rand::RngExt;

use super::{Controls, Goal, to_goal_ticks};
use crate::entity::mob::Mob;
use crate::entity::mob::vex::VexEntity;

/// Vanilla `Vex.VexRandomMoveGoal`: picks an empty block near the bound origin and flies to it.
pub struct VexRandomMoveGoal {
    vex: Weak<VexEntity>,
}

impl VexRandomMoveGoal {
    #[must_use]
    pub const fn new(vex: Weak<VexEntity>) -> Self {
        Self { vex }
    }
}

impl Goal for VexRandomMoveGoal {
    fn can_start(&mut self, mob: &dyn Mob) -> bool {
        !mob.get_mob_entity().steered().move_control().has_wanted()
            && mob.get_random().random_range(0..to_goal_ticks(7)) == 0
    }

    fn should_continue(&mut self, _mob: &dyn Mob) -> bool {
        false
    }

    fn tick(&mut self, mob: &dyn Mob) {
        let entity = mob.get_entity();
        let bound_origin = self
            .vex
            .upgrade()
            .and_then(|vex| vex.get_bound_origin())
            .unwrap_or_else(|| entity.block_pos.load());
        let world = entity.world.load();
        let mob_entity = mob.get_mob_entity();
        let mut random = mob.get_random();

        for _ in 0..3 {
            let test_pos = bound_origin.offset(Vector3::new(
                random.random_range(0..15) - 7,
                random.random_range(0..11) - 5,
                random.random_range(0..15) - 7,
            ));
            if world.get_block_state(&test_pos).is_air() {
                let x = f64::from(test_pos.0.x) + 0.5;
                let y = f64::from(test_pos.0.y) + 0.5;
                let z = f64::from(test_pos.0.z) + 0.5;
                mob_entity
                    .steered()
                    .move_control()
                    .set_wanted_position(x, y, z, 0.25);
                if mob_entity.get_target().is_none() {
                    mob_entity
                        .look_control
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner)
                        .look_at_with_range(x, y, z, 180.0, 20.0);
                }
                break;
            }
        }
    }

    fn controls(&self) -> Controls {
        Controls::MOVE
    }
}
