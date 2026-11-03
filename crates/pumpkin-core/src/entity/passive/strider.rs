use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Weak};

use pumpkin_data::attributes::Attributes;
use pumpkin_data::damage::DamageType;
use pumpkin_data::data_component_impl::EquipmentSlot;
use pumpkin_data::entity::EntityType;
use pumpkin_data::fluid::Fluid;
use pumpkin_data::item::Item;
use pumpkin_data::item_stack::ItemStack;
use pumpkin_data::sound::Sound;
use pumpkin_data::tag::{self, Taggable};
use pumpkin_data::{Block, BlockState};
use pumpkin_nbt::compound::NbtCompound;
use pumpkin_util::math::boundingbox::BoundingBox;
use pumpkin_util::math::position::BlockPos;
use pumpkin_util::math::vector3::Vector3;

use crate::block::PathComputationType;
use crate::entity::attributes::{Modifier, ModifierOperation, send_attribute_updates_for_living};
use crate::entity::{
    Entity, EntityBase,
    ageable::{AgeableData, AgeableMob},
    ai::goal::{
        Controls, Goal, ParentHandle,
        breed::BreedGoal,
        escape_danger::EscapeDangerGoal,
        follow_parent::FollowParentGoal,
        look_around::RandomLookAroundGoal,
        look_at_entity::LookAtEntityGoal,
        move_to_target_pos::{MoveToTargetPos, MoveToTargetPosGoal},
        tempt::TemptGoal,
        wander_around::WanderAroundGoal,
    },
    ai::pathfinder::{Navigator, node::PathType},
    item_steerable::{ItemBasedSteering, ItemSteerable},
    mob::{Mob, MobEntity, equipment::equip_on_target},
    passive::animal::Animal,
    player::Player,
};

const TEMPT_ITEMS: &[&Item] = &[&Item::WARPED_FUNGUS, &Item::WARPED_FUNGUS_ON_A_STICK];

const SUFFOCATING_MODIFIER_ID: &str = "minecraft:suffocating";
const SUFFOCATING_MODIFIER_AMOUNT: f32 = -0.34;
/// Vanilla `Strider.getLiquidCollisionShape`: `Block.column(16.0, 0.0, 8.0)`.
const LIQUID_COLLISION_SHAPE: BoundingBox = BoundingBox {
    min: Vector3::new(0.0, 0.0, 0.0),
    max: Vector3::new(1.0, 0.5, 1.0),
};

pub struct StriderEntity {
    pub mob_entity: MobEntity,
    pub ageable_data: AgeableData,
    pub steering: ItemBasedSteering,
    pub suffocating: AtomicBool,
}

impl StriderEntity {
    pub fn new(entity: Entity) -> Arc<Self> {
        let mob_entity = MobEntity::new(entity);
        let strider = Self {
            mob_entity,
            ageable_data: AgeableData::default(),
            steering: ItemBasedSteering::default(),
            suffocating: AtomicBool::new(false),
        };
        let mob_arc = Arc::new(strider);
        let mob_weak: Weak<dyn Mob> = {
            let mob_arc: Arc<dyn Mob> = mob_arc.clone();
            Arc::downgrade(&mob_arc)
        };

        let mut navigator = mob_arc
            .mob_entity
            .navigator
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        *navigator = Navigator::strider();
        navigator.set_pathfinding_malus(PathType::Water, -1.0);
        navigator.set_pathfinding_malus(PathType::Lava, 0.0);
        navigator.set_pathfinding_malus(PathType::DangerFire, 0.0);
        navigator.set_pathfinding_malus(PathType::DamageFire, 0.0);
        drop(navigator);

        {
            let goal_selector = &mob_arc.mob_entity.goals_selector;

            goal_selector.add_goal(1, EscapeDangerGoal::new(1.65));
            goal_selector.add_goal(2, BreedGoal::new(1.0));
            goal_selector.add_goal(3, Box::new(TemptGoal::new(1.4, TEMPT_ITEMS, false)));
            goal_selector.add_goal(4, StriderGoToLavaGoal::new(1.0));
            goal_selector.add_goal(5, Box::new(FollowParentGoal::new(1.0)));
            goal_selector.add_goal(7, Box::new(WanderAroundGoal::with_interval(1.0, 60)));
            goal_selector.add_goal(
                8,
                LookAtEntityGoal::with_default(mob_weak.clone(), &EntityType::PLAYER, 8.0),
            );
            goal_selector.add_goal(8, Box::new(RandomLookAroundGoal::default()));
            goal_selector.add_goal(
                9,
                LookAtEntityGoal::with_default(mob_weak, &EntityType::STRIDER, 8.0),
            );
        };

        mob_arc
    }

    #[must_use]
    pub fn is_suffocating(&self) -> bool {
        self.suffocating.load(Ordering::Relaxed)
    }

    pub fn set_suffocating(&self, suffocating: bool) {
        // Vanilla only resyncs the flag and the speed modifier when they change.
        if self.suffocating.swap(suffocating, Ordering::Relaxed) == suffocating {
            return;
        }
        let entity = self.get_entity();
        entity.set_synced_data(
            pumpkin_data::tracked_data::strider::DATA_SUFFOCATING,
            suffocating,
        );
        let living = &self.mob_entity.living_entity;
        living.update_attribute(&Attributes::MOVEMENT_SPEED, |inst| {
            if suffocating {
                inst.add_or_replace_modifier(Modifier {
                    id: SUFFOCATING_MODIFIER_ID.to_string(),
                    amount: f64::from(SUFFOCATING_MODIFIER_AMOUNT),
                    operation: ModifierOperation::MultiplyBase,
                });
            } else {
                inst.remove_modifier(SUFFOCATING_MODIFIER_ID);
            }
        });
        send_attribute_updates_for_living(living, vec![Attributes::MOVEMENT_SPEED]);
    }

    /// The suffocation half of vanilla `Strider.tick`: cold away from warm blocks and lava,
    /// unless it rides a strider that is warm.
    fn update_suffocating(&self) {
        if self.mob_entity.is_no_ai() {
            return;
        }
        let entity = self.get_entity();
        let world = entity.world.load();
        let block_inside = world.get_block(&entity.block_pos.load());
        let block_on = world.get_block(&entity.get_pos_with_y_offset(f64::from(0.2f32)).0);
        let in_warm_blocks = block_inside.has_tag(&tag::Block::MINECRAFT_STRIDER_WARM_BLOCKS)
            || block_on.has_tag(&tag::Block::MINECRAFT_STRIDER_WARM_BLOCKS)
            || entity.lava_height.load() > 0.0;
        let on_warm_strider = entity.get_vehicle().is_some_and(|vehicle| {
            vehicle
                .cast_any()
                .downcast_ref::<Self>()
                .is_some_and(|strider| !strider.is_suffocating())
        });
        self.set_suffocating(!in_warm_blocks && !on_warm_strider);
    }

    /// Vanilla `Strider.floatStrider`: stand on the lava surface, or bob up toward it.
    fn float_strider(&self) {
        let entity = self.get_entity();
        if !entity.touching_lava.load(Ordering::Relaxed) {
            return;
        }
        let block_pos = entity.block_pos.load();
        let world = entity.world.load();
        let above_lava_shape = entity.pos.load().y
            > f64::from(block_pos.0.y) + LIQUID_COLLISION_SHAPE.max.y - f64::from(1.0e-5f32);
        if above_lava_shape
            && !world
                .get_fluid(&block_pos.up())
                .has_tag(&tag::Fluid::MINECRAFT_LAVA)
        {
            entity.on_ground.store(true, Ordering::Relaxed);
        } else {
            entity
                .velocity
                .store(entity.velocity.load() * 0.5 + Vector3::new(0.0, 0.05, 0.0));
        }
    }
}

/// Vanilla `Strider.StriderGoToLavaGoal`: walk back onto lava when off it.
pub struct StriderGoToLavaGoal {
    goal: MoveToTargetPosGoal<Self>,
}

impl StriderGoToLavaGoal {
    #[must_use]
    pub fn new(speed: f64) -> Box<Self> {
        let mut this = Box::new(Self {
            goal: MoveToTargetPosGoal::new(ParentHandle::none(), speed, 8, 2),
        });
        // SAFETY: `this` heap allocation address is pinned in Box and outlives `ParentHandle` references.
        this.goal.move_to_target_pos = unsafe { ParentHandle::new(&this) };
        this
    }
}

fn is_in_lava(mob: &dyn Mob) -> bool {
    mob.get_entity().touching_lava.load(Ordering::Relaxed)
}

impl Goal for StriderGoToLavaGoal {
    fn can_start(&mut self, mob: &dyn Mob) -> bool {
        !is_in_lava(mob) && self.goal.can_start(mob)
    }

    fn should_continue(&mut self, mob: &dyn Mob) -> bool {
        let world = mob.get_entity().world.load_full();
        !is_in_lava(mob) && self.is_target_pos(world, self.goal.target_pos)
    }

    fn start(&mut self, mob: &dyn Mob) {
        self.goal.start(mob);
    }

    fn stop(&mut self, mob: &dyn Mob) {
        self.goal.stop(mob);
    }

    fn tick(&mut self, mob: &dyn Mob) {
        self.goal.tick(mob);
    }

    fn should_run_every_tick(&self) -> bool {
        self.goal.should_run_every_tick()
    }

    fn controls(&self) -> Controls {
        self.goal.controls()
    }
}

impl MoveToTargetPos for StriderGoToLavaGoal {
    fn is_target_pos(&self, world: Arc<crate::world::World>, block_pos: BlockPos) -> bool {
        if world.get_block(&block_pos) != &Block::LAVA {
            return false;
        }
        let above = block_pos.up();
        let (block, state) = world.get_block_and_state(&above);
        world
            .block_registry
            .is_pathfindable(block, state, PathComputationType::Land)
    }

    fn get_move_to_target(&self, target_pos: BlockPos) -> BlockPos {
        target_pos
    }

    fn should_recalculate_path(&self, trying_time: i32) -> bool {
        trying_time % 20 == 0
    }
}

impl AgeableMob for StriderEntity {
    fn get_ageable_data(&self) -> &AgeableData {
        &self.ageable_data
    }

    fn default_baby_spawn_chance(&self) -> f32 {
        crate::entity::ageable::STRIDER_BABY_SPAWN_CHANCE
    }
}

impl Animal for StriderEntity {
    fn is_food(&self, item_stack: &ItemStack) -> bool {
        item_stack.item.has_tag(&tag::Item::MINECRAFT_STRIDER_FOOD)
            || item_stack.item == &Item::WARPED_FUNGUS
    }
}

impl Mob for StriderEntity {
    fn mob_tick(&self, caller: &dyn EntityBase) {
        self.update_suffocating();
        // Vanilla `isSensitiveToWater`.
        if self.get_entity().is_in_water_or_rain() {
            caller.damage(caller, 1.0, DamageType::DROWN);
        }
    }

    fn post_tick(&self) {
        self.float_strider();
    }

    fn can_stand_on_fluid(&self, fluid: &Fluid) -> bool {
        fluid.has_tag(&tag::Fluid::MINECRAFT_LAVA)
    }

    fn get_liquid_collision_shape(&self) -> Option<BoundingBox> {
        Some(LIQUID_COLLISION_SHAPE)
    }

    /// Vanilla `checkFallDamage`: lava clears the fall instead of landing on it.
    fn should_check_fall_damage(&self) -> bool {
        if is_in_lava(self) {
            self.mob_entity.living_entity.fall_distance.store(0.0);
            return false;
        }
        true
    }

    fn get_walk_target_value(&self, pos: &BlockPos) -> f32 {
        let entity = self.get_entity();
        if entity
            .world
            .load()
            .get_fluid(pos)
            .has_tag(&tag::Fluid::MINECRAFT_LAVA)
        {
            10.0
        } else if is_in_lava(self) {
            f32::NEG_INFINITY
        } else {
            0.0
        }
    }

    fn can_dispenser_equip_into_slot(&self, slot: &EquipmentSlot) -> bool {
        *slot == EquipmentSlot::SADDLE || self.mob_entity.can_pick_up_loot()
    }

    fn mob_controlling_passenger(&self) -> Option<Arc<dyn EntityBase>> {
        self.saddled_controlling_passenger(Some(&Item::WARPED_FUNGUS_ON_A_STICK))
    }

    fn mob_play_step_sound(&self, _pos: &BlockPos, _state: &BlockState) {
        let sound = if self.get_entity().touching_lava.load(Ordering::Relaxed) {
            Sound::EntityStriderStepLava
        } else {
            Sound::EntityStriderStep
        };
        self.play_entity_sound(sound, 1.0, 1.0);
    }

    fn as_ageable(&self) -> Option<&dyn AgeableMob> {
        Some(self)
    }

    fn as_animal(&self) -> Option<&dyn Animal> {
        Some(self)
    }

    fn mob_write_nbt(&self, nbt: &mut NbtCompound) {
        self.write_ageable_nbt(nbt);
    }

    fn mob_read_nbt(&self, nbt: &NbtCompound) {
        self.read_ageable_nbt(nbt);
        self.mob_entity
            .read_legacy_saddle(nbt.get_bool("Saddle") == Some(true));
    }

    fn get_mob_entity(&self) -> &MobEntity {
        &self.mob_entity
    }

    fn get_item_steerable(&self) -> Option<&dyn ItemSteerable> {
        Some(self)
    }

    fn can_use_slot(&self, slot: &EquipmentSlot) -> bool {
        *slot != EquipmentSlot::SADDLE
            || (self.mob_entity.living_entity.entity.is_alive() && !self.is_baby())
    }

    fn get_equip_sound(&self, slot: &EquipmentSlot) -> Option<Sound> {
        (*slot == EquipmentSlot::SADDLE).then_some(Sound::EntityStriderSaddle)
    }

    fn mob_init_data_tracker(&self) {
        let entity = self.get_entity();
        let is_baby = entity.age.load(Ordering::Relaxed) < 0;
        if is_baby {
            entity.set_synced_data(pumpkin_data::tracked_data::strider::DATA_BABY_ID, true);
        }
        entity.set_synced_data(
            pumpkin_data::tracked_data::strider::DATA_SUFFOCATING,
            self.is_suffocating(),
        );
    }

    fn mob_interact(&self, player: &Arc<Player>, item_stack: &mut ItemStack) -> bool {
        if !self.is_food(item_stack)
            && self.is_saddled()
            && !self.get_entity().has_passengers()
            && !player.get_entity().is_sneaking()
        {
            let world = player.world();
            let ent = &self.mob_entity.living_entity.entity;
            if let Some(vehicle) = world.get_entity_by_id(ent.entity_id)
                && let Some(passenger) = world.get_player_by_id(player.entity_id())
            {
                ent.add_passenger(vehicle, passenger as Arc<dyn EntityBase>);
            }
            return true;
        }

        if self.animal_interact(player, item_stack) {
            return true;
        }
        self.is_equippable_in_slot(item_stack, &EquipmentSlot::SADDLE)
            && equip_on_target(self, player, item_stack)
    }
}

impl ItemSteerable for StriderEntity {
    fn boost(&self) -> bool {
        self.steering.boost()
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}
