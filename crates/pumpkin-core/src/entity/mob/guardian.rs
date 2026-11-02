use std::sync::atomic::Ordering;
use std::sync::{Arc, Weak};

use pumpkin_data::entity::EntityType;
use pumpkin_data::{damage::DamageType, sound::Sound};

use crate::entity::ai::behavior::hunting::{self, Hunt};
use crate::entity::ai::target_match::TargetMatch;
use crate::entity::custom_sound::CustomSound;
use crate::entity::living::LivingEntity;
use crate::entity::{
    Entity, EntityBase,
    ai::goal::{
        guardian_attack::GuardianAttackGoal, look_around::RandomLookAroundGoal,
        look_at_entity::LookAtEntityGoal, swim::SwimGoal, wander_around::WanderAroundGoal,
    },
    mob::{Mob, MobEntity},
};
use crate::world::World;
use pumpkin_data::tracked_data;
use pumpkin_protocol::codec::var_int::VarInt;

const GUARDIAN_PREY: TargetMatch = TargetMatch::AnyOf(&[
    &EntityType::PLAYER,
    &EntityType::SQUID,
    &EntityType::GLOW_SQUID,
    &EntityType::AXOLOTL,
]);

/// Vanilla `GuardianAttackSelector`: nothing within 3 blocks.
fn outside_melee_range(hunter: &dyn Mob, target: &LivingEntity, _world: &World) -> bool {
    let hunter_pos = hunter.get_mob_entity().living_entity.entity.pos.load();
    target
        .entity
        .pos
        .load()
        .squared_distance_to_vec(&hunter_pos)
        > 9.0
}

pub const GUARDIAN_HUNTS: &[Hunt] =
    &[Hunt::new(1, GUARDIAN_PREY, true).matching(outside_melee_range)];

/// Vanilla `Guardian.setActiveAttackTarget`; the client draws the beam to this entity id.
pub fn set_active_attack_target(entity: &Entity, entity_id: i32) {
    entity.set_synced_data(
        tracked_data::guardian::DATA_ID_ATTACK_TARGET,
        VarInt(entity_id),
    );
}

pub struct GuardianEntity {
    pub mob_entity: MobEntity,
}

impl GuardianEntity {
    /// Vanilla `Guardian.getAttackDuration`.
    pub const ATTACK_DURATION: i32 = 80;

    pub fn new(entity: Entity) -> Arc<Self> {
        let mob_entity = MobEntity::new(entity);
        let guardian = Self { mob_entity };
        let mob_arc = Arc::new(guardian);
        let mob_weak: Weak<dyn Mob> = {
            let mob_arc: Arc<dyn Mob> = mob_arc.clone();
            Arc::downgrade(&mob_arc)
        };

        {
            let goal_selector = &mob_arc.mob_entity.goals_selector;

            goal_selector.add_goal(0, Box::new(SwimGoal::default()));
            goal_selector.add_goal(
                4,
                Box::new(GuardianAttackGoal::new(Self::ATTACK_DURATION, false)),
            );
            goal_selector.add_goal(7, Box::new(WanderAroundGoal::new(1.0)));
            goal_selector.add_goal(
                8,
                LookAtEntityGoal::with_default(mob_weak.clone(), &EntityType::PLAYER, 8.0),
            );
            goal_selector.add_goal(9, Box::new(RandomLookAroundGoal::default()));

            let target_selector = &mob_arc.mob_entity.target_selector;
            hunting::apply(target_selector, &mob_arc.mob_entity, GUARDIAN_HUNTS);
        };

        mob_arc
    }
}

impl CustomSound for GuardianEntity {
    fn hurt_sound(&self) -> Option<Sound> {
        let entity = self.get_entity();
        let is_water = entity.touching_water.load(Ordering::Relaxed);
        Some(if is_water {
            Sound::EntityGuardianHurt
        } else {
            Sound::EntityGuardianHurtLand
        })
    }

    fn death_sound(&self) -> Option<Sound> {
        let entity = self.get_entity();
        let is_water = entity.touching_water.load(Ordering::Relaxed);
        Some(if is_water {
            Sound::EntityGuardianDeath
        } else {
            Sound::EntityGuardianDeathLand
        })
    }
}

impl Mob for GuardianEntity {
    fn get_mob_entity(&self) -> &MobEntity {
        &self.mob_entity
    }

    fn on_damage(&self, _damage_type: DamageType, source: Option<&dyn EntityBase>) {
        if let Some(src) = source
            && let Some(living) = src.get_living_entity()
        {
            let _ = living.damage(src, 2.0, DamageType::THORNS);
        }
    }
}
