use std::sync::atomic::{AtomicBool, AtomicI32, Ordering};
use std::sync::{Arc, Weak};

use crossbeam::atomic::AtomicCell;
use pumpkin_data::damage::DamageType;
use pumpkin_data::entity::EntityType;
use pumpkin_data::tracked_data;
use pumpkin_nbt::compound::NbtCompound;
use pumpkin_util::math::position::BlockPos;
use uuid::Uuid;

use crate::entity::ai::behavior::hunting::{self, Hunt};
use crate::entity::ai::brain::memory::codec::{block_pos_from_tag, block_pos_to_tag};
use crate::entity::ai::control::vex_move_control::VexMoveControl;
use crate::entity::ai::target_match::TargetMatch;
use crate::entity::mob::raider;
use crate::entity::{
    Entity, EntityBase,
    ai::goal::{
        look_at_entity::LookAtEntityGoal, swim::SwimGoal, vex_charge_attack::VexChargeAttackGoal,
        vex_copy_owner_target::VexCopyOwnerTargetGoal, vex_random_move::VexRandomMoveGoal,
    },
    mob::{Mob, MobEntity},
};

const VEX_HUNTS: &[Hunt] = &[Hunt::new(3, TargetMatch::Type(&EntityType::PLAYER), true)];

/// Vanilla `Vex.TICKS_PER_FLAP`, `Mth.ceil(PI * 5 / 4)`.
const TICKS_PER_FLAP: i32 = 4;
/// Vanilla `Vex.FLAG_IS_CHARGING`.
const FLAG_IS_CHARGING: i8 = 1;

pub struct VexEntity {
    pub mob_entity: MobEntity,
    owner: AtomicCell<Option<Uuid>>,
    bound_origin: AtomicCell<Option<BlockPos>>,
    has_limited_life: AtomicBool,
    limited_life_ticks: AtomicI32,
    is_charging: AtomicBool,
}

impl VexEntity {
    pub fn new(entity: Entity) -> Arc<Self> {
        let mut mob_entity = MobEntity::new(entity);
        *mob_entity
            .move_control
            .get_mut()
            .unwrap_or_else(std::sync::PoisonError::into_inner) =
            Box::new(VexMoveControl::default());
        let vex = Self {
            mob_entity,
            owner: AtomicCell::new(None),
            bound_origin: AtomicCell::new(None),
            has_limited_life: AtomicBool::new(false),
            limited_life_ticks: AtomicI32::new(0),
            is_charging: AtomicBool::new(false),
        };
        let mob_arc = Arc::new(vex);
        let mob_weak: Weak<dyn Mob> = {
            let mob_arc: Arc<dyn Mob> = mob_arc.clone();
            Arc::downgrade(&mob_arc)
        };

        {
            let goal_selector = &mob_arc.mob_entity.goals_selector;

            goal_selector.add_goal(0, Box::new(SwimGoal::default()));
            goal_selector.add_goal(
                4,
                Box::new(VexChargeAttackGoal::new(Arc::downgrade(&mob_arc))),
            );
            goal_selector.add_goal(
                8,
                Box::new(VexRandomMoveGoal::new(Arc::downgrade(&mob_arc))),
            );
            goal_selector.add_goal(
                9,
                Box::new(LookAtEntityGoal::new(
                    mob_weak,
                    &EntityType::PLAYER,
                    3.0,
                    1.0,
                    false,
                )),
            );

            let target_selector = &mob_arc.mob_entity.target_selector;
            target_selector.add_goal(1, raider::revenge_goal());
            target_selector.add_goal(2, VexCopyOwnerTargetGoal::new(Arc::downgrade(&mob_arc)));
            hunting::apply(target_selector, &mob_arc.mob_entity, VEX_HUNTS);
        };

        mob_arc
    }

    pub fn get_owner(&self) -> Option<Uuid> {
        self.owner.load()
    }

    pub fn set_owner(&self, owner: Uuid) {
        self.owner.store(Some(owner));
    }

    pub fn get_bound_origin(&self) -> Option<BlockPos> {
        self.bound_origin.load()
    }

    pub fn set_bound_origin(&self, bound_origin: Option<BlockPos>) {
        self.bound_origin.store(bound_origin);
    }

    pub fn set_limited_life(&self, life_ticks: i32) {
        self.has_limited_life.store(true, Ordering::Relaxed);
        self.limited_life_ticks.store(life_ticks, Ordering::Relaxed);
    }

    pub fn is_charging(&self) -> bool {
        self.is_charging.load(Ordering::Relaxed)
    }

    pub fn set_is_charging(&self, charging: bool) {
        self.is_charging.store(charging, Ordering::Relaxed);
        let flags = if charging { FLAG_IS_CHARGING } else { 0 };
        self.mob_entity
            .living_entity
            .entity
            .set_synced_data(tracked_data::vex::DATA_FLAGS_ID, flags);
    }
}

impl Mob for VexEntity {
    fn get_mob_entity(&self) -> &MobEntity {
        &self.mob_entity
    }

    fn get_owner_uuid(&self) -> Option<Uuid> {
        self.get_owner()
    }

    fn mob_is_flapping(&self) -> bool {
        self.get_entity().tick_count.load(Ordering::Relaxed) % TICKS_PER_FLAP == 0
    }

    /// Vanilla `Vex.tick` runs `super.tick()` with `noPhysics` set, so the vex flies through blocks.
    fn mob_tick(&self, _caller: &dyn EntityBase) {
        self.get_entity().no_physics.store(true, Ordering::Relaxed);
    }

    fn post_tick(&self) {
        let entity = self.get_entity();
        entity.no_physics.store(false, Ordering::Relaxed);
        entity.set_has_no_gravity(true);
        if self.has_limited_life.load(Ordering::Relaxed)
            && self.limited_life_ticks.fetch_sub(1, Ordering::Relaxed) - 1 <= 0
        {
            self.limited_life_ticks.store(20, Ordering::Relaxed);
            let _ = self.damage(self, 1.0, DamageType::STARVE);
        }
    }

    fn mob_write_nbt(&self, nbt: &mut NbtCompound) {
        if let Some(bound_origin) = self.get_bound_origin() {
            nbt.put("bound_pos", block_pos_to_tag(bound_origin));
        }
        if self.has_limited_life.load(Ordering::Relaxed) {
            nbt.put_int(
                "life_ticks",
                self.limited_life_ticks.load(Ordering::Relaxed),
            );
        }
        if let Some(owner) = self.get_owner() {
            nbt.put_uuid("owner", owner);
        }
    }

    fn mob_read_nbt(&self, nbt: &NbtCompound) {
        self.set_bound_origin(nbt.get("bound_pos").and_then(block_pos_from_tag));
        match nbt.get_int("life_ticks") {
            Some(life_ticks) => self.set_limited_life(life_ticks),
            None => self.has_limited_life.store(false, Ordering::Relaxed),
        }
        self.owner.store(nbt.get_uuid("owner"));
    }
}
