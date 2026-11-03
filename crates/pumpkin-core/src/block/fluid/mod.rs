pub mod flowing_trait;
pub mod lava;
pub mod pathfinder;
pub mod physics;
pub mod water;

// Re-export for backward compatibility
pub mod flowing {
    pub use super::flowing_trait::*;
    pub use super::pathfinder::*;
    pub use super::physics::*;
}

use super::{BlockIsReplacing, registry::BlockActionResult};
use crate::entity::{EntityBase, player::Player};
use crate::{server::Server, world::World};
use pumpkin_data::BlockDirection;
use pumpkin_data::BlockStateId;
use pumpkin_data::block_properties::WaterLikeProperties;
use pumpkin_data::{fluid::Fluid, item::Item};
use pumpkin_protocol::java::server::play::SUseItemOn;
use pumpkin_util::math::boundingbox::BoundingBox;
use pumpkin_util::math::position::BlockPos;
use std::sync::Arc;

/// Vanilla `LiquidBlock.getCollisionShape` for an entity: a source block holds up a mob that
/// can stand on its fluid while the mob is above the mob's liquid shape.
pub(crate) fn collision_shape_for_entity(
    world: &World,
    entity: &dyn EntityBase,
    position: &BlockPos,
    state_id: BlockStateId,
) -> Option<BoundingBox> {
    if WaterLikeProperties::from_state_id(state_id).level != 0 {
        return None;
    }
    let mob = entity.get_mob()?;
    let shape = mob.get_liquid_collision_shape()?;
    // Vanilla `EntityCollisionContext.isAbove`.
    let entity_bottom = entity.get_entity().pos.load().y;
    if entity_bottom <= f64::from(position.0.y) + shape.max.y - f64::from(1.0e-5f32) {
        return None;
    }
    // Vanilla `EntityCollisionContext.canStandOnFluid`.
    let fluid = World::get_fluid_from_state_id(state_id);
    let fluid_above = world.get_fluid(&position.up());
    (mob.can_stand_on_fluid(fluid) && !fluid_above.matches_type(fluid))
        .then(|| shape.at_pos(*position))
}

pub trait FluidBehaviour: Send + Sync {
    fn normal_use(
        &self,
        _fluid: &Fluid,
        _player: &Player,
        _location: BlockPos,
        _server: &Server,
        _world: &Arc<World>,
    ) {
    }

    fn use_with_item(
        &self,
        _fluid: &Fluid,
        _player: &Player,
        _location: BlockPos,
        _item: &Item,
        _server: &Server,
        _world: &Arc<World>,
    ) -> BlockActionResult {
        BlockActionResult::Pass
    }

    fn placed(
        &self,
        _world: &Arc<World>,
        _fluid: &Fluid,
        _state_id: BlockStateId,
        _block_pos: &BlockPos,
        _old_state_id: BlockStateId,
        _notify: bool,
    ) {
    }

    #[expect(clippy::too_many_arguments)]
    fn on_place(
        &self,
        _server: &Server,
        _world: &Arc<World>,
        fluid: &Fluid,
        _face: BlockDirection,
        _block_pos: &BlockPos,
        _use_item_on: &SUseItemOn,
        _replacing: BlockIsReplacing,
    ) -> BlockStateId {
        fluid.states[fluid.default_state_index as usize].block_state_id
    }

    fn get_state_for_neighbour_update(
        &self,
        _world: &Arc<World>,
        _fluid: &Fluid,
        _block_pos: &BlockPos,
        _notify: bool,
    ) -> BlockStateId {
        BlockStateId::AIR
    }

    fn on_neighbor_update(
        &self,
        _world: &Arc<World>,
        _fluid: &Fluid,
        _block_pos: &BlockPos,
        _notify: bool,
    ) {
    }

    fn on_entity_collision(&self, _entity: &dyn EntityBase) {}

    fn on_scheduled_tick(&self, _world: &Arc<World>, _fluid: &Fluid, _block_pos: &BlockPos) {}

    fn random_tick(&self, _fluid: &Fluid, _world: &Arc<World>, _block_pos: &BlockPos) {}

    fn create_legacy_block(&self, _world: &Arc<World>, _block_pos: &BlockPos) {}
}
