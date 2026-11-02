use std::sync::{Arc, Weak};

use super::track_target::TrackTargetGoal;
use super::{Controls, Goal};
use crate::entity::EntityBase;
use crate::entity::ai::target_predicate::TargetPredicate;
use crate::entity::mob::Mob;
use crate::entity::mob::vex::VexEntity;
use crate::plugin::api::events::entity::entity_target::TargetReason;

/// Vanilla `Vex.VexCopyOwnerTargetGoal`: the vex attacks whatever its summoner is targeting.
pub struct VexCopyOwnerTargetGoal {
    vex: Weak<VexEntity>,
    track_target_goal: TrackTargetGoal,
    copy_owner_targeting: TargetPredicate,
}

impl VexCopyOwnerTargetGoal {
    #[must_use]
    pub fn new(vex: Weak<VexEntity>) -> Box<Self> {
        Box::new(Self {
            vex,
            track_target_goal: TrackTargetGoal::with_default(false),
            copy_owner_targeting: TargetPredicate::create_non_attackable()
                .ignore_visibility()
                .ignore_distance_scaling_factor(),
        })
    }

    /// The owner's target, when the owner is a mob (vanilla `Targeting`).
    fn owner_target(&self, mob: &dyn Mob) -> Option<Arc<dyn EntityBase>> {
        let owner_uuid = self.vex.upgrade()?.get_owner()?;
        let world = mob.get_entity().world.load_full();
        let owner = world.get_entity_by_uuid(owner_uuid)?;
        owner.get_mob()?.get_mob_entity().get_target()
    }
}

impl Goal for VexCopyOwnerTargetGoal {
    fn can_start(&mut self, mob: &dyn Mob) -> bool {
        let target = self.owner_target(mob);
        self.track_target_goal
            .can_track(mob, target.as_deref(), &self.copy_owner_targeting)
    }

    fn should_continue(&mut self, mob: &dyn Mob) -> bool {
        self.track_target_goal.should_continue(mob)
    }

    fn start(&mut self, mob: &dyn Mob) {
        let target = self.owner_target(mob);
        self.track_target_goal
            .start_targeting(mob, target, TargetReason::OwnerAttackedTarget);
        self.track_target_goal.start(mob);
    }

    fn stop(&mut self, mob: &dyn Mob) {
        self.track_target_goal.stop(mob);
    }

    fn controls(&self) -> Controls {
        self.track_target_goal.controls()
    }
}
