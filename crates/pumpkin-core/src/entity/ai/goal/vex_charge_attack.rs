use std::sync::Weak;

use pumpkin_data::sound::Sound;
use rand::RngExt;

use super::{Controls, Goal, to_goal_ticks};
use crate::entity::mob::Mob;
use crate::entity::mob::vex::VexEntity;

/// Vanilla `Vex.VexChargeAttackGoal`: flies straight at the target's eyes and hits on contact.
pub struct VexChargeAttackGoal {
    vex: Weak<VexEntity>,
}

impl VexChargeAttackGoal {
    #[must_use]
    pub const fn new(vex: Weak<VexEntity>) -> Self {
        Self { vex }
    }
}

impl Goal for VexChargeAttackGoal {
    fn can_start(&mut self, mob: &dyn Mob) -> bool {
        let mob_entity = mob.get_mob_entity();
        let Some(target) = mob_entity.get_target() else {
            return false;
        };
        if !target.get_entity().is_alive()
            || mob_entity.steered().move_control().has_wanted()
            || mob.get_random().random_range(0..to_goal_ticks(7)) != 0
        {
            return false;
        }
        mob.get_entity()
            .pos
            .load()
            .squared_distance_to_vec(&target.get_entity().pos.load())
            > 4.0
    }

    fn should_continue(&mut self, mob: &dyn Mob) -> bool {
        let mob_entity = mob.get_mob_entity();
        mob_entity.steered().move_control().has_wanted()
            && self.vex.upgrade().is_some_and(|vex| vex.is_charging())
            && mob_entity
                .get_target()
                .is_some_and(|target| target.get_entity().is_alive())
    }

    fn start(&mut self, mob: &dyn Mob) {
        let mob_entity = mob.get_mob_entity();
        if let Some(target) = mob_entity.get_target() {
            let eye = target.get_entity().get_eye_pos();
            mob_entity
                .steered()
                .move_control()
                .set_wanted_position(eye.x, eye.y, eye.z, 1.0);
        }

        if let Some(vex) = self.vex.upgrade() {
            vex.set_is_charging(true);
        }
        mob.play_entity_sound(Sound::EntityVexCharge, 1.0, 1.0);
    }

    fn stop(&mut self, _mob: &dyn Mob) {
        if let Some(vex) = self.vex.upgrade() {
            vex.set_is_charging(false);
        }
    }

    fn should_run_every_tick(&self) -> bool {
        true
    }

    fn tick(&mut self, mob: &dyn Mob) {
        let mob_entity = mob.get_mob_entity();
        let Some(target) = mob_entity.get_target() else {
            return;
        };
        let entity = mob.get_entity();
        let target_entity = target.get_entity();

        if entity
            .bounding_box
            .load()
            .intersects(&target_entity.bounding_box.load())
        {
            let _ = mob_entity.try_attack(entity, target.as_ref());
            if let Some(vex) = self.vex.upgrade() {
                vex.set_is_charging(false);
            }
        } else if entity
            .pos
            .load()
            .squared_distance_to_vec(&target_entity.pos.load())
            < 9.0
        {
            let eye = target_entity.get_eye_pos();
            mob_entity
                .steered()
                .move_control()
                .set_wanted_position(eye.x, eye.y, eye.z, 1.0);
        }
    }

    fn controls(&self) -> Controls {
        Controls::MOVE
    }
}
