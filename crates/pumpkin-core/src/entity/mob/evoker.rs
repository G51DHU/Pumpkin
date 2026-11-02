use std::sync::Mutex;
use std::sync::atomic::{AtomicI32, AtomicU8, Ordering};
use std::sync::{Arc, Weak};
use uuid::Uuid;

use pumpkin_data::BlockDirection;
use pumpkin_data::entity::EntityType;
use pumpkin_data::sound::Sound;
use pumpkin_nbt::compound::NbtCompound;
use pumpkin_util::math::position::BlockPos;
use pumpkin_util::math::vector3::Vector3;
use rand::RngExt;

use crate::entity::ai::behavior::fleeing::{self, Flee};
use crate::entity::ai::behavior::hunting::{self, Hunt};
use crate::entity::ai::target_match::TargetMatch;
use crate::entity::{
    Entity, EntityBase,
    ai::goal::{
        Controls, Goal, look_around::RandomLookAroundGoal, look_at_entity::LookAtEntityGoal,
        swim::SwimGoal, to_goal_ticks, wander_around::WanderAroundGoal,
    },
    mob::{
        Mob, MobEntity,
        patrol::{PatrolData, PatrollingMonster},
        raider::{
            self, ObtainRaidLeaderBannerGoal, PathfindToRaidGoal, Raider, RaiderCelebrationGoal,
            RaiderData, RaiderMoveThroughVillageGoal,
        },
    },
    passive::sheep::SheepEntity,
    projectile::evoker_fangs::EvokerFangsEntity,
    r#type::from_type,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum IllagerSpell {
    None = 0,
    SummonVex = 1,
    Fangs = 2,
    Wololo = 3,
    Disappear = 4,
    Blindness = 5,
}

impl IllagerSpell {
    #[must_use]
    pub const fn from_u8(val: u8) -> Self {
        match val {
            1 => Self::SummonVex,
            2 => Self::Fangs,
            3 => Self::Wololo,
            4 => Self::Disappear,
            5 => Self::Blindness,
            _ => Self::None,
        }
    }
}

const EVOKER_FLEES: &[Flee] = &[
    Flee::new(2, TargetMatch::Type(&EntityType::PLAYER), 8.0, 0.6, 1.0),
    raider::flee_creaking(3, 0.6, 1.0),
];

const EVOKER_HUNTS: &[Hunt] = &[
    Hunt::new(2, TargetMatch::Type(&EntityType::PLAYER), true).forget_unseen_after(300),
    Hunt::new(3, TargetMatch::VILLAGER_LIKE, false).forget_unseen_after(300),
    Hunt::new(3, TargetMatch::Type(&EntityType::IRON_GOLEM), false),
];

pub struct EvokerEntity {
    pub mob_entity: MobEntity,
    pub raider_data: RaiderData,
    spell_casting_tick_count: AtomicI32,
    current_spell: AtomicU8,
    wololo_target_id: Arc<Mutex<Option<i32>>>,
}

impl EvokerEntity {
    pub fn new(entity: Entity) -> Arc<Self> {
        let mob_entity = MobEntity::new(entity);
        let evoker = Self {
            mob_entity,
            raider_data: RaiderData::default(),
            spell_casting_tick_count: AtomicI32::new(0),
            current_spell: AtomicU8::new(IllagerSpell::None as u8),
            wololo_target_id: Arc::new(Mutex::new(None)),
        };
        let mob_arc = Arc::new(evoker);
        let mob_weak: Weak<Self> = Arc::downgrade(&mob_arc);
        let mob_interface_weak: Weak<dyn Mob> = {
            let mob_dyn: Arc<dyn Mob> = mob_arc.clone();
            Arc::downgrade(&mob_dyn)
        };

        {
            let goal_selector = &mob_arc.mob_entity.goals_selector;

            goal_selector.add_goal(0, Box::new(SwimGoal::default()));
            goal_selector.add_goal(1, Box::new(EvokerCastingSpellGoal::new(mob_weak.clone())));
            goal_selector.add_goal(2, Box::new(ObtainRaidLeaderBannerGoal));
            fleeing::apply(goal_selector, EVOKER_FLEES);
            goal_selector.add_goal(3, Box::new(RaiderMoveThroughVillageGoal::new(1.05)));
            goal_selector.add_goal(3, Box::new(PathfindToRaidGoal::default()));
            goal_selector.add_goal(4, Box::new(EvokerSummonSpellGoal::new(mob_weak.clone())));
            goal_selector.add_goal(5, Box::new(EvokerAttackSpellGoal::new(mob_weak.clone())));
            goal_selector.add_goal(6, Box::new(EvokerWololoSpellGoal::new(mob_weak)));
            goal_selector.add_goal(7, Box::new(RaiderCelebrationGoal));
            goal_selector.add_goal(8, Box::new(WanderAroundGoal::new(0.6)));
            goal_selector.add_goal(
                9,
                LookAtEntityGoal::with_default(mob_interface_weak, &EntityType::PLAYER, 3.0),
            );
            goal_selector.add_goal(10, Box::new(RandomLookAroundGoal::default()));

            let target_selector = &mob_arc.mob_entity.target_selector;
            target_selector.add_goal(1, raider::revenge_goal());
            hunting::apply(target_selector, &mob_arc.mob_entity, EVOKER_HUNTS);
        };

        mob_arc
    }

    pub fn is_casting_spell(&self) -> bool {
        self.spell_casting_tick_count.load(Ordering::Relaxed) > 0
    }

    pub fn set_is_casting_spell(&self, spell: IllagerSpell) {
        self.current_spell.store(spell as u8, Ordering::Relaxed);
        let entity = &self.mob_entity.living_entity.entity;
        entity.set_synced_data(
            pumpkin_data::tracked_data::evoker::SPELL_CASTING_ID,
            spell as u8 as i8,
        );
    }

    pub fn get_current_spell(&self) -> IllagerSpell {
        IllagerSpell::from_u8(self.current_spell.load(Ordering::Relaxed))
    }

    pub fn get_spell_casting_time(&self) -> i32 {
        self.spell_casting_tick_count.load(Ordering::Relaxed)
    }

    pub fn set_spell_casting_time(&self, ticks: i32) {
        self.spell_casting_tick_count
            .store(ticks, Ordering::Relaxed);
    }

    /// Vanilla `getWololoTarget`.
    pub fn get_wololo_target(&self) -> Option<Arc<dyn EntityBase>> {
        let id = (*self
            .wololo_target_id
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner))?;
        self.mob_entity
            .living_entity
            .entity
            .world
            .load()
            .get_entity_by_id(id)
    }

    /// Vanilla `setWololoTarget`, by entity id.
    pub fn set_wololo_target(&self, id: Option<i32>) {
        *self
            .wololo_target_id
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = id;
    }
}

impl Mob for EvokerEntity {
    fn as_patrolling_monster(&self) -> Option<&dyn PatrollingMonster> {
        Some(self)
    }

    fn as_raider(&self) -> Option<&dyn Raider> {
        Some(self)
    }

    fn mob_write_nbt(&self, nbt: &mut NbtCompound) {
        self.write_raider_nbt(nbt);
        nbt.put_int("SpellTicks", self.get_spell_casting_time());
    }

    fn mob_read_nbt(&self, nbt: &NbtCompound) {
        self.read_raider_nbt(nbt);
        if let Some(ticks) = nbt.get_int("SpellTicks") {
            self.set_spell_casting_time(ticks);
        }
    }

    fn get_mob_entity(&self) -> &MobEntity {
        &self.mob_entity
    }

    fn mob_tick(&self, _caller: &dyn EntityBase) {
        let ticks = self.spell_casting_tick_count.load(Ordering::Relaxed);
        if ticks > 0 {
            self.spell_casting_tick_count
                .store(ticks - 1, Ordering::Relaxed);
        }
    }
}

impl PatrollingMonster for EvokerEntity {
    fn get_patrol_data(&self) -> &PatrolData {
        &self.raider_data.patrol_data
    }
}

impl Raider for EvokerEntity {
    fn get_raider_data(&self) -> &RaiderData {
        &self.raider_data
    }

    fn get_celebrate_sound(&self) -> Sound {
        Sound::EntityEvokerCelebrate
    }
}

/// Vanilla `SpellcasterIllager.SpellcasterUseSpellGoal`: the warmup, cooldown and casting
/// state every evoker spell goal shares.
struct SpellcasterUseSpell {
    attack_warmup_delay: i32,
    next_attack_tick_count: i32,
}

impl SpellcasterUseSpell {
    /// Vanilla `getCastWarmupTime` default.
    const CAST_WARMUP_TIME: i32 = 20;

    const fn new() -> Self {
        Self {
            attack_warmup_delay: 0,
            next_attack_tick_count: 0,
        }
    }

    fn has_live_target(evoker: &EvokerEntity) -> bool {
        evoker
            .mob_entity
            .get_target()
            .is_some_and(|target| target.get_entity().is_alive())
    }

    fn tick_count(evoker: &EvokerEntity) -> i32 {
        evoker
            .mob_entity
            .living_entity
            .entity
            .tick_count
            .load(Ordering::Relaxed)
    }

    /// Vanilla `canUse`.
    fn can_use(&self, evoker: &EvokerEntity) -> bool {
        Self::has_live_target(evoker)
            && !evoker.is_casting_spell()
            && Self::tick_count(evoker) >= self.next_attack_tick_count
    }

    /// Vanilla `canContinueToUse`.
    fn can_continue_to_use(&self, evoker: &EvokerEntity) -> bool {
        Self::has_live_target(evoker) && self.attack_warmup_delay > 0
    }

    /// Vanilla `start`. The goal does not tick every server tick, so the warmup is in goal ticks.
    fn start(
        &mut self,
        evoker: &EvokerEntity,
        cast_warmup_time: i32,
        casting_time: i32,
        casting_interval: i32,
        prepare_sound: Sound,
        spell: IllagerSpell,
    ) {
        self.attack_warmup_delay = to_goal_ticks(cast_warmup_time);
        evoker.set_spell_casting_time(casting_time);
        self.next_attack_tick_count = Self::tick_count(evoker) + casting_interval;
        evoker
            .mob_entity
            .living_entity
            .entity
            .play_sound(prepare_sound);
        evoker.set_is_casting_spell(spell);
    }

    /// Vanilla `tick`: true on the tick the spell is cast.
    const fn tick(&mut self) -> bool {
        self.attack_warmup_delay -= 1;
        self.attack_warmup_delay == 0
    }
}

pub struct EvokerCastingSpellGoal {
    evoker: Weak<EvokerEntity>,
}

impl EvokerCastingSpellGoal {
    #[must_use]
    pub const fn new(evoker: Weak<EvokerEntity>) -> Self {
        Self { evoker }
    }
}

impl Goal for EvokerCastingSpellGoal {
    fn can_start(&mut self, _mob: &dyn Mob) -> bool {
        self.evoker
            .upgrade()
            .is_some_and(|evoker| evoker.get_spell_casting_time() > 0)
    }

    fn start(&mut self, mob: &dyn Mob) {
        mob.get_mob_entity().steered().navigator().stop();
    }

    fn stop(&mut self, _mob: &dyn Mob) {
        if let Some(evoker) = self.evoker.upgrade() {
            evoker.set_is_casting_spell(IllagerSpell::None);
        }
    }

    /// Vanilla `Evoker.EvokerCastingSpellGoal.tick`: watch the target, or else the wololo sheep.
    fn tick(&mut self, mob: &dyn Mob) {
        let Some(evoker) = self.evoker.upgrade() else {
            return;
        };
        let mob_entity = mob.get_mob_entity();
        let look_at = mob_entity
            .get_target()
            .or_else(|| evoker.get_wololo_target());
        if let Some(look_at) = look_at {
            mob_entity
                .look_control
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .look_at_entity_with_range(
                    &look_at,
                    mob.get_max_head_rotation(),
                    mob.get_max_look_pitch_change(),
                );
        }
    }

    fn controls(&self) -> Controls {
        Controls::MOVE | Controls::LOOK
    }
}

pub struct EvokerAttackSpellGoal {
    evoker: Weak<EvokerEntity>,
    spell: SpellcasterUseSpell,
}

impl EvokerAttackSpellGoal {
    #[must_use]
    pub const fn new(evoker: Weak<EvokerEntity>) -> Self {
        Self {
            evoker,
            spell: SpellcasterUseSpell::new(),
        }
    }

    /// Vanilla `createSpellEntity`: drops from `max_y` to the first sturdy top face, at most to `min_y - 1`.
    fn create_spell_entity(
        evoker: &EvokerEntity,
        x: f64,
        z: f64,
        min_y: f64,
        max_y: f64,
        angle: f32,
        delay_ticks: u32,
    ) {
        let evoker_ent = &evoker.mob_entity.living_entity.entity;
        let world = evoker_ent.world.load_full();
        let mut pos = BlockPos::floored(x, max_y, z);
        let lowest_y = min_y.floor() as i32 - 1;

        loop {
            let below = pos.down();
            if world
                .get_block_state(&below)
                .is_side_solid(BlockDirection::Up)
            {
                let state = world.get_block_state(&pos);
                let top_offset = if state.is_air() {
                    0.0
                } else {
                    state
                        .get_block_collision_shapes()
                        .map(|shape| shape.max.y)
                        .reduce(f64::max)
                        .unwrap_or(0.0)
                };
                let entity = Entity::from_uuid(
                    Uuid::new_v4(),
                    world.clone(),
                    Vector3::new(x, f64::from(pos.0.y) + top_offset, z),
                    &EntityType::EVOKER_FANGS,
                );
                world.spawn_entity(Arc::new(EvokerFangsEntity::new(
                    entity,
                    delay_ticks,
                    angle,
                    Some(evoker_ent),
                )));
                return;
            }

            pos = below;
            if pos.0.y < lowest_y {
                return;
            }
        }
    }

    /// Vanilla `Evoker.EvokerAttackSpellGoal.performSpellCasting`.
    fn perform_spell_casting(evoker: &EvokerEntity) {
        let Some(target) = evoker.mob_entity.get_target() else {
            return;
        };
        let evoker_pos = evoker.mob_entity.living_entity.entity.pos.load();
        let target_pos = target.get_entity().pos.load();
        let min_y = target_pos.y.min(evoker_pos.y);
        let max_y = target_pos.y.max(evoker_pos.y) + 1.0;
        let angle_towards_target =
            (target_pos.z - evoker_pos.z).atan2(target_pos.x - evoker_pos.x) as f32;

        if evoker_pos.squared_distance_to_vec(&target_pos) < 9.0 {
            for i in 0..5 {
                let angle = angle_towards_target + i as f32 * std::f32::consts::PI * 0.4;
                Self::create_spell_entity(
                    evoker,
                    evoker_pos.x + f64::from(angle.cos()) * 1.5,
                    evoker_pos.z + f64::from(angle.sin()) * 1.5,
                    min_y,
                    max_y,
                    angle,
                    0,
                );
            }
            for i in 0..8 {
                let angle = angle_towards_target
                    + i as f32 * std::f32::consts::PI * 2.0 / 8.0
                    + std::f32::consts::TAU / 5.0;
                Self::create_spell_entity(
                    evoker,
                    evoker_pos.x + f64::from(angle.cos()) * 2.5,
                    evoker_pos.z + f64::from(angle.sin()) * 2.5,
                    min_y,
                    max_y,
                    angle,
                    3,
                );
            }
        } else {
            for i in 0..16_u32 {
                let reach = 1.25 * f64::from(i + 1);
                Self::create_spell_entity(
                    evoker,
                    evoker_pos.x + f64::from(angle_towards_target.cos()) * reach,
                    evoker_pos.z + f64::from(angle_towards_target.sin()) * reach,
                    min_y,
                    max_y,
                    angle_towards_target,
                    i,
                );
            }
        }
    }
}

impl Goal for EvokerAttackSpellGoal {
    fn can_start(&mut self, _mob: &dyn Mob) -> bool {
        self.evoker
            .upgrade()
            .is_some_and(|evoker| self.spell.can_use(&evoker))
    }

    fn should_continue(&mut self, _mob: &dyn Mob) -> bool {
        self.evoker
            .upgrade()
            .is_some_and(|evoker| self.spell.can_continue_to_use(&evoker))
    }

    fn start(&mut self, _mob: &dyn Mob) {
        if let Some(evoker) = self.evoker.upgrade() {
            self.spell.start(
                &evoker,
                SpellcasterUseSpell::CAST_WARMUP_TIME,
                40,
                100,
                Sound::EntityEvokerPrepareAttack,
                IllagerSpell::Fangs,
            );
        }
    }

    fn tick(&mut self, _mob: &dyn Mob) {
        if self.spell.tick()
            && let Some(evoker) = self.evoker.upgrade()
        {
            Self::perform_spell_casting(&evoker);
            evoker
                .mob_entity
                .living_entity
                .entity
                .play_sound(Sound::EntityEvokerCastSpell);
        }
    }
}

pub struct EvokerSummonSpellGoal {
    evoker: Weak<EvokerEntity>,
    spell: SpellcasterUseSpell,
}

impl EvokerSummonSpellGoal {
    #[must_use]
    pub const fn new(evoker: Weak<EvokerEntity>) -> Self {
        Self {
            evoker,
            spell: SpellcasterUseSpell::new(),
        }
    }
}

impl Goal for EvokerSummonSpellGoal {
    fn can_start(&mut self, _mob: &dyn Mob) -> bool {
        let Some(evoker) = self.evoker.upgrade() else {
            return false;
        };
        if !self.spell.can_use(&evoker) {
            return false;
        }

        // Count nearby Vexes
        let entity = &evoker.mob_entity.living_entity.entity;
        let bb = entity.bounding_box.load().expand(16.0, 16.0, 16.0);
        let world = entity.world.load();
        let nearby = world.get_entities_at_box(&bb);
        let vex_count = nearby
            .iter()
            .filter(|e| *e.get_entity().entity_type == EntityType::VEX)
            .count();

        let max_allowed = (rand::random::<u8>() % 8 + 1) as usize;
        vex_count < max_allowed
    }

    fn should_continue(&mut self, _mob: &dyn Mob) -> bool {
        self.evoker
            .upgrade()
            .is_some_and(|evoker| self.spell.can_continue_to_use(&evoker))
    }

    fn start(&mut self, _mob: &dyn Mob) {
        if let Some(evoker) = self.evoker.upgrade() {
            self.spell.start(
                &evoker,
                SpellcasterUseSpell::CAST_WARMUP_TIME,
                100,
                340,
                Sound::EntityEvokerPrepareSummon,
                IllagerSpell::SummonVex,
            );
        }
    }

    fn tick(&mut self, _mob: &dyn Mob) {
        if self.spell.tick() {
            let Some(evoker) = self.evoker.upgrade() else {
                return;
            };
            let evoker_ent = &evoker.mob_entity.living_entity.entity;
            evoker_ent.play_sound(Sound::EntityEvokerCastSpell);

            let world = evoker_ent.world.load();
            let evoker_pos = evoker_ent.pos.load();

            for _ in 0..3 {
                let offset_x = (rand::random::<i32>() % 5 - 2) as f64;
                let offset_z = (rand::random::<i32>() % 5 - 2) as f64;
                let spawn_pos = Vector3::new(
                    evoker_pos.x + offset_x,
                    evoker_pos.y + 1.0,
                    evoker_pos.z + offset_z,
                );

                let vex = from_type(&EntityType::VEX, spawn_pos, &world, Uuid::new_v4());
                world.spawn_entity(vex);
            }
        }
    }
}

pub struct EvokerWololoSpellGoal {
    evoker: Weak<EvokerEntity>,
    spell: SpellcasterUseSpell,
}

impl EvokerWololoSpellGoal {
    /// Vanilla `getCastWarmupTime` override.
    const CAST_WARMUP_TIME: i32 = 40;
    /// Vanilla `DyeColor.BLUE` and `DyeColor.RED` ids.
    const BLUE: u8 = 11;
    const RED: u8 = 14;

    #[must_use]
    pub const fn new(evoker: Weak<EvokerEntity>) -> Self {
        Self {
            evoker,
            spell: SpellcasterUseSpell::new(),
        }
    }

    /// Vanilla `wololoTargeting`: a living blue sheep within 16 blocks.
    fn is_wololo_target(evoker: &EvokerEntity, candidate: &dyn EntityBase) -> bool {
        let entity = candidate.get_entity();
        *entity.entity_type == EntityType::SHEEP
            && entity.is_alive()
            && entity
                .pos
                .load()
                .squared_distance_to_vec(&evoker.mob_entity.living_entity.entity.pos.load())
                <= 16.0 * 16.0
            && candidate
                .cast_any()
                .downcast_ref::<SheepEntity>()
                .is_some_and(|sheep| sheep.get_color() == Self::BLUE)
    }
}

impl Goal for EvokerWololoSpellGoal {
    /// Vanilla overrides `canUse` whole: no target, griefing allowed, and a random blue sheep.
    fn can_start(&mut self, _mob: &dyn Mob) -> bool {
        let Some(evoker) = self.evoker.upgrade() else {
            return false;
        };
        if evoker.mob_entity.get_target().is_some()
            || evoker.is_casting_spell()
            || SpellcasterUseSpell::tick_count(&evoker) < self.spell.next_attack_tick_count
        {
            return false;
        }

        let entity = &evoker.mob_entity.living_entity.entity;
        let world = entity.world.load();
        if !world.level_info.load().game_rules.mob_griefing {
            return false;
        }

        let bb = entity.bounding_box.load().expand(16.0, 4.0, 16.0);
        let sheep: Vec<_> = world
            .get_entities_at_box(&bb)
            .into_iter()
            .filter(|candidate| Self::is_wololo_target(&evoker, candidate.as_ref()))
            .collect();
        if sheep.is_empty() {
            return false;
        }
        let chosen = &sheep[evoker.get_random().random_range(0..sheep.len())];
        evoker.set_wololo_target(Some(chosen.get_entity().entity_id));
        true
    }

    fn should_continue(&mut self, _mob: &dyn Mob) -> bool {
        self.evoker.upgrade().is_some_and(|evoker| {
            evoker.get_wololo_target().is_some() && self.spell.attack_warmup_delay > 0
        })
    }

    fn start(&mut self, _mob: &dyn Mob) {
        if let Some(evoker) = self.evoker.upgrade() {
            self.spell.start(
                &evoker,
                Self::CAST_WARMUP_TIME,
                60,
                140,
                Sound::EntityEvokerPrepareWololo,
                IllagerSpell::Wololo,
            );
        }
    }

    fn stop(&mut self, _mob: &dyn Mob) {
        if let Some(evoker) = self.evoker.upgrade() {
            evoker.set_wololo_target(None);
        }
    }

    fn tick(&mut self, _mob: &dyn Mob) {
        if self.spell.tick()
            && let Some(evoker) = self.evoker.upgrade()
        {
            if let Some(target) = evoker.get_wololo_target()
                && target.get_entity().is_alive()
                && let Some(sheep) = target.cast_any().downcast_ref::<SheepEntity>()
            {
                sheep.set_color(Self::RED);
            }
            evoker
                .mob_entity
                .living_entity
                .entity
                .play_sound(Sound::EntityEvokerCastSpell);
        }
    }
}
