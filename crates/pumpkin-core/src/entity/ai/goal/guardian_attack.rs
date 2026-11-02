use pumpkin_data::damage::DamageType;
use pumpkin_data::entity::EntityStatus;
use pumpkin_protocol::bedrock::server::ActorEventID;
use pumpkin_util::Difficulty;

use super::{Controls, Goal};
use crate::entity::mob::Mob;
use crate::entity::mob::guardian::set_active_attack_target;

/// Vanilla `Guardian.GuardianAttackGoal`.
pub struct GuardianAttackGoal {
    attack_time: i32,
    attack_duration: i32,
    elder: bool,
}

impl GuardianAttackGoal {
    #[must_use]
    pub const fn new(attack_duration: i32, elder: bool) -> Self {
        Self {
            attack_time: 0,
            attack_duration,
            elder,
        }
    }
}

impl Goal for GuardianAttackGoal {
    fn can_start(&mut self, mob: &dyn Mob) -> bool {
        mob.get_mob_entity()
            .get_target()
            .is_some_and(|target| target.get_entity().is_alive())
    }

    fn should_continue(&mut self, mob: &dyn Mob) -> bool {
        if !self.can_start(mob) {
            return false;
        }
        if self.elder {
            return true;
        }
        mob.get_mob_entity().get_target().is_some_and(|target| {
            mob.get_entity()
                .pos
                .load()
                .squared_distance_to_vec(&target.get_entity().pos.load())
                > 9.0
        })
    }

    fn start(&mut self, mob: &dyn Mob) {
        self.attack_time = -10;
        let mob_entity = mob.get_mob_entity();
        mob_entity.steered().navigator().stop();
        if let Some(target) = mob_entity.get_target() {
            mob_entity
                .look_control
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .look_at_entity_with_range(&target, 90.0, 90.0);
        }
    }

    fn stop(&mut self, mob: &dyn Mob) {
        set_active_attack_target(mob.get_entity(), 0);
        mob.get_mob_entity().set_target(None);
    }

    fn should_run_every_tick(&self) -> bool {
        true
    }

    fn tick(&mut self, mob: &dyn Mob) {
        let mob_entity = mob.get_mob_entity();
        let Some(target) = mob_entity.get_target() else {
            return;
        };

        mob_entity.steered().navigator().stop();
        mob_entity
            .look_control
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .look_at_entity_with_range(&target, 90.0, 90.0);

        if !mob.has_line_of_sight(target.get_entity()) {
            mob_entity.set_target(None);
            return;
        }

        self.attack_time += 1;
        let entity = mob.get_entity();
        if self.attack_time == 0 {
            set_active_attack_target(entity, target.get_entity().entity_id);
            if !entity.is_silent() {
                entity.world.load().send_entity_status(
                    entity,
                    EntityStatus::GuardianAttackSound,
                    ActorEventID::GuardianAttackSound,
                );
            }
        } else if self.attack_time >= self.attack_duration {
            let mut magic_damage = 1.0;
            if entity.world.load().level_info.load().difficulty == Difficulty::Hard {
                magic_damage += 2.0;
            }
            if self.elder {
                magic_damage += 2.0;
            }

            let _ = target.damage_with_context(
                target.as_ref(),
                magic_damage,
                DamageType::INDIRECT_MAGIC,
                None,
                Some(entity),
                Some(entity),
            );
            let _ = mob_entity.try_attack(entity, target.as_ref());
            mob_entity.set_target(None);
        }
    }

    fn controls(&self) -> Controls {
        Controls::MOVE | Controls::LOOK
    }
}
